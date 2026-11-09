//! `laneway i18n check`: everything about the translations CI can catch.
//!
//! [`check`] works on a repo root with the usual layout:
//!
//! - `locales/`: `languages.toml` and one `<code>.ftl` per language.
//! - `web/src/**/*.{ts,tsx}` and `crates/*/src/**/*.rs`: scanned for message ids.
//! - `web/src/**/*.tsx`: scanned for literal UI text.
//!
//! What it reports:
//!
//! | kind | severity | what |
//! |---|---|---|
//! | `manifest` | error | `languages.toml` can't be read, or a listed language has no file |
//! | `syntax` | error | Fluent syntax errors and duplicate messages, with file and line |
//! | `stale` | error | a translation has a message the default language doesn't have |
//! | `variables` | error | a translation uses other `{ $variables }` than the default language |
//! | `unused` | error | a default-language message whose id appears nowhere in the source |
//! | `literal-text` | error | JSX text in the web UI that doesn't come from `t(...)` |
//! | `missing` | warning | a translation lacks a message (it falls back to the default) |
//! | `unlisted` | warning | a `.ftl` file that isn't in `languages.toml` |
//!
//! "Used" means the id appears as a quoted string literal (`"common-version"` or
//! `'common-version'`) in any scanned file, tests included. Ids built at runtime (`"board-" + x`)
//! are not detected, so write ids out in full.
//!
//! The literal text check is regex based, not a parser: it flags text between a tag and a closing
//! tag (`<p>Hello</p>`), text after an opening tag at the end of a line, and a text line right
//! after a line that ends in a tag. Put `i18n-ignore` in a comment on the line to skip it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use fluent_syntax::ast;
use regex::Regex;

use crate::{Error, MANIFEST, line_of, read, read_manifest};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Manifest,
    Syntax,
    Stale,
    Variables,
    Unused,
    LiteralText,
    Missing,
    Unlisted,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Manifest => "manifest",
            Self::Syntax => "syntax",
            Self::Stale => "stale",
            Self::Variables => "variables",
            Self::Unused => "unused",
            Self::LiteralText => "literal-text",
            Self::Missing => "missing",
            Self::Unlisted => "unlisted",
        }
    }

    fn severity(self) -> Severity {
        match self {
            Self::Missing | Self::Unlisted => Severity::Warning,
            _ => Severity::Error,
        }
    }
}

/// One problem. `path` is relative to the checked root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub kind: Kind,
    pub path: PathBuf,
    pub line: Option<usize>,
    pub message: String,
}

impl Finding {
    pub fn severity(&self) -> Severity {
        self.kind.severity()
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let severity = match self.severity() {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(f, "{severity}[{}] {}", self.kind.as_str(), self.path.display())?;
        if let Some(line) = self.line {
            write!(f, ":{line}")?;
        }
        write!(f, ": {}", self.message)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    /// Errors first, then warnings, each in the order they were found.
    pub findings: Vec<Finding>,
}

impl Report {
    pub fn errors(&self) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(|f| f.severity() == Severity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(|f| f.severity() == Severity::Warning)
    }

    pub fn has_errors(&self) -> bool {
        self.errors().next().is_some()
    }
}

/// A message as the check sees it.
struct Message {
    line: Option<usize>,
    variables: BTreeSet<String>,
}

/// One parsed `.ftl` file: id -> message.
type Messages = BTreeMap<String, Message>;

struct Checker<'a> {
    root: &'a Path,
    findings: Vec<Finding>,
}

impl Checker<'_> {
    fn push(&mut self, kind: Kind, path: &Path, line: Option<usize>, message: String) {
        let path = path.strip_prefix(self.root).unwrap_or(path).to_owned();
        self.findings.push(Finding { kind, path, line, message });
    }

    /// Parses one `.ftl` file, reporting syntax errors and duplicates. Keeps what did parse.
    fn parse(&mut self, path: &Path) -> Option<Messages> {
        let source = match read(path) {
            Ok(source) => source,
            Err(e) => {
                self.push(Kind::Manifest, path, None, e.to_string());
                return None;
            }
        };
        let resource = fluent_syntax::parser::parse(source.as_str()).unwrap_or_else(|(resource, errors)| {
            for e in errors {
                self.push(
                    Kind::Syntax,
                    path,
                    Some(line_of(&source, e.slice.as_ref().map_or(e.pos.start, |s| s.start))),
                    e.kind.to_string(),
                );
            }
            resource
        });
        let lines = definition_lines(&source);
        let mut messages = Messages::new();
        for entry in resource.body {
            let ast::Entry::Message(message) = entry else { continue };
            let id = message.id.name.to_owned();
            let line = lines.get(id.as_str()).copied();
            if messages.contains_key(&id) {
                self.push(Kind::Syntax, path, line, format!("message `{id}` is defined more than once"));
                continue;
            }
            let mut variables = BTreeSet::new();
            if let Some(value) = &message.value {
                pattern_variables(value, &mut variables);
            }
            for attribute in &message.attributes {
                pattern_variables(&attribute.value, &mut variables);
            }
            messages.insert(id, Message { line, variables });
        }
        Some(messages)
    }
}

/// Runs every check on the repo at `root`. See the module docs for the layout and the checks.
pub fn check(root: &Path) -> Report {
    let mut checker = Checker { root, findings: Vec::new() };
    check_locales(&mut checker);
    for path in source_files(&root.join("web/src"), &["tsx"]) {
        match read(&path) {
            Ok(source) => {
                for (line, text) in literal_ui_text(&source) {
                    let message = format!("literal UI text `{text}`, use t(...) and locales/*.ftl");
                    checker.push(Kind::LiteralText, &path, Some(line), message);
                }
            }
            Err(e) => checker.push(Kind::LiteralText, &path, None, e.to_string()),
        }
    }
    let mut findings = checker.findings;
    findings.sort_by_key(|f| std::cmp::Reverse(f.severity()));
    Report { findings }
}

fn check_locales(checker: &mut Checker<'_>) {
    let dir = checker.root.join("locales");
    let manifest = match read_manifest(&dir) {
        Ok(manifest) => manifest,
        Err(e) => {
            let message = match e {
                Error::Manifest { message, .. } => message,
                other => other.to_string(),
            };
            checker.push(Kind::Manifest, &dir.join(MANIFEST), None, message);
            return;
        }
    };

    let mut parsed = BTreeMap::new();
    for (code, _) in &manifest.languages {
        let path = dir.join(format!("{code}.ftl"));
        if !path.is_file() {
            checker.push(
                Kind::Manifest,
                &path,
                None,
                format!("`{code}` is listed in {MANIFEST} but has no file"),
            );
            continue;
        }
        if let Some(messages) = checker.parse(&path) {
            parsed.insert(code.clone(), (path, messages));
        }
    }

    for path in source_files(&dir, &["ftl"]) {
        let code = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        if !manifest.languages.iter().any(|(c, _)| c == code) {
            checker.push(
                Kind::Unlisted,
                &path,
                None,
                format!("not listed in {MANIFEST}, so it isn't loaded"),
            );
        }
    }

    let Some((default_path, default)) = parsed.remove(&manifest.default) else { return };
    for (code, (path, messages)) in &parsed {
        for (id, message) in messages {
            let Some(original) = default.get(id) else {
                let text = format!("`{id}` doesn't exist in {}.ftl, remove it", manifest.default);
                checker.push(Kind::Stale, path, message.line, text);
                continue;
            };
            if message.variables != original.variables {
                let text = format!(
                    "`{id}` uses {} but {}.ftl uses {}",
                    describe(&message.variables),
                    manifest.default,
                    describe(&original.variables)
                );
                checker.push(Kind::Variables, path, message.line, text);
            }
        }
        for id in default.keys().filter(|id| !messages.contains_key(*id)) {
            checker.push(Kind::Missing, path, None, format!("`{id}` is not translated to `{code}` yet"));
        }
    }

    let root = checker.root;
    let mut sources = source_files(&root.join("web/src"), &["ts", "tsx"]);
    for crate_dir in subdirectories(&root.join("crates")) {
        sources.extend(source_files(&crate_dir.join("src"), &["rs"]));
    }
    let text: String =
        sources.iter().filter_map(|p| fs::read_to_string(p).ok()).collect::<Vec<_>>().join("\n");
    for (id, message) in &default {
        if !text.contains(&format!("\"{id}\"")) && !text.contains(&format!("'{id}'")) {
            let text = "never used in web/src or crates/*/src, remove it or use it".to_owned();
            checker.push(Kind::Unused, &default_path, message.line, format!("`{id}` is {text}"));
        }
    }
}

fn describe(variables: &BTreeSet<String>) -> String {
    if variables.is_empty() {
        "no variables".to_owned()
    } else {
        variables.iter().map(|v| format!("${v}")).collect::<Vec<_>>().join(", ")
    }
}

/// Line of each `id = ...` definition. The AST has no positions, so this scans the lines.
fn definition_lines(source: &str) -> BTreeMap<&str, usize> {
    static DEFINITION: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^([a-zA-Z][a-zA-Z0-9_-]*)[ \t]*=").expect("valid regex"));
    let mut lines = BTreeMap::new();
    for (index, line) in source.lines().enumerate() {
        if let Some(captures) = DEFINITION.captures(line) {
            let id = captures.get(1).expect("group 1").as_str();
            lines.entry(id).or_insert(index + 1);
        }
    }
    lines
}

fn pattern_variables(pattern: &ast::Pattern<&str>, out: &mut BTreeSet<String>) {
    for element in &pattern.elements {
        if let ast::PatternElement::Placeable { expression } = element {
            expression_variables(expression, out);
        }
    }
}

fn expression_variables(expression: &ast::Expression<&str>, out: &mut BTreeSet<String>) {
    match expression {
        ast::Expression::Select { selector, variants } => {
            inline_variables(selector, out);
            for variant in variants {
                pattern_variables(&variant.value, out);
            }
        }
        ast::Expression::Inline(inline) => inline_variables(inline, out),
    }
}

fn inline_variables(inline: &ast::InlineExpression<&str>, out: &mut BTreeSet<String>) {
    match inline {
        ast::InlineExpression::VariableReference { id } => {
            out.insert(id.name.to_owned());
        }
        ast::InlineExpression::FunctionReference { arguments, .. }
        | ast::InlineExpression::TermReference { arguments: Some(arguments), .. } => {
            for argument in &arguments.positional {
                inline_variables(argument, out);
            }
            for argument in &arguments.named {
                inline_variables(&argument.value, out);
            }
        }
        ast::InlineExpression::Placeable { expression } => expression_variables(expression, out),
        _ => {}
    }
}

/// Files under `dir` (recursive) with one of `extensions`, sorted. Skips `node_modules` and
/// hidden directories. A missing `dir` has no files.
fn source_files(dir: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_owned()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                if !name.starts_with('.') && name != "node_modules" {
                    pending.push(path);
                }
            } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| extensions.contains(&e)) {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn subdirectories(dir: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(dir)
        .map(|entries| entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
        .unwrap_or_default();
    dirs.sort();
    dirs
}

/// Literal text in JSX, as `(1-based line, text)`. Crude on purpose, see the module docs.
pub fn literal_ui_text(source: &str) -> Vec<(usize, String)> {
    // `>text</`, where the `>` is not part of `=>` or `->`.
    static BETWEEN_TAGS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?:^|[^=-])>([^<>{}]+)</").expect("valid regex"));
    // `<tag ...>text` at the end of a line or before a `{`.
    static AFTER_OPENING_TAG: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"<[A-Za-z][\w.:-]*(?:\s[^<>]*)?>([^<>{}]+)(?:\{|$)").expect("valid regex")
    });

    // Text-bearing attributes with a plain string value: `placeholder="Search"`.
    static TEXT_ATTRIBUTE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"\b(?:placeholder|title|aria-label|alt)\s*=\s*(?:"([^"]*)"|'([^']*)')"#).expect("valid regex")
    });

    let mut found = Vec::new();
    let mut previous_ends_with_tag = false;
    for (index, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let ends_with_tag = trimmed.ends_with('>') && !trimmed.ends_with("=>") && !trimmed.ends_with("->");
        let skip =
            line.contains("i18n-ignore") || ["//", "/*", "*", "{/*"].iter().any(|c| trimmed.starts_with(c));
        if !skip {
            let mut texts: Vec<&str> = BETWEEN_TAGS
                .captures_iter(line)
                .filter_map(|c| c.get(1).map(|m| m.as_str()))
                .filter(|t| is_text(t))
                .collect();
            texts.extend(
                TEXT_ATTRIBUTE
                    .captures_iter(line)
                    .filter_map(|c| c.get(1).or_else(|| c.get(2)).map(|m| m.as_str()))
                    .filter(|t| is_text(t)),
            );
            let after_tag = AFTER_OPENING_TAG.captures(trimmed).and_then(|c| c.get(1)).map(|m| m.as_str());
            if let Some(text) = after_tag
                && is_text(text)
                && !looks_like_code(text)
            {
                texts.push(text);
            }
            if previous_ends_with_tag && !trimmed.starts_with(['<', '{', ')', '}']) {
                let text = trimmed.split(['<', '{']).next().unwrap_or_default();
                if is_text(text) && !looks_like_code(text) {
                    texts.push(text);
                }
            }
            let mut seen = BTreeSet::new();
            for text in texts {
                let text = text.trim();
                if seen.insert(text) {
                    found.push((index + 1, text.to_owned()));
                }
            }
        }
        previous_ends_with_tag = ends_with_tag;
    }
    found
}

/// Has letters once HTML entities (`&nbsp;`) are removed.
fn is_text(text: &str) -> bool {
    static ENTITY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"&#?\w+;").expect("valid regex"));
    ENTITY.replace_all(text, "").chars().any(char::is_alphabetic)
}

fn looks_like_code(text: &str) -> bool {
    text.contains(['(', ')', '=', ';', '[', ']', '"', '\'', '`', '&', '|'])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::write_locales;

    const MANIFEST_EN_DE: &str = "default = \"en\"\n[languages]\nen = \"English\"\nde = \"Deutsch\"\n";

    /// A repo with the given locale files and one source file that uses `used`.
    fn repo(files: &[(&str, &str)], used: &[&str]) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        write_locales(&root.path().join("locales"), MANIFEST_EN_DE, files);
        let calls = used.iter().map(|id| format!("t(\"{id}\");\n")).collect::<Vec<_>>().concat();
        fs::create_dir_all(root.path().join("web/src")).unwrap();
        fs::write(root.path().join("web/src/use.ts"), calls).unwrap();
        root
    }

    fn kinds(report: &Report) -> Vec<(Kind, String)> {
        report.findings.iter().map(|f| (f.kind, f.to_string())).collect()
    }

    #[test]
    fn clean_repo_passes() {
        let root = repo(&[("en.ftl", "a = A { $x }\n"), ("de.ftl", "a = Ä { $x }\n")], &["a"]);
        let report = check(root.path());
        assert!(report.findings.is_empty(), "{:?}", kinds(&report));
    }

    #[test]
    fn reports_syntax_errors_with_line() {
        let root = repo(&[("en.ftl", "a = A\n"), ("de.ftl", "a = A\n\nb = {\n")], &["a"]);
        let report = check(root.path());
        let finding = report.errors().find(|f| f.kind == Kind::Syntax).expect("syntax error");
        assert_eq!(finding.path, Path::new("locales/de.ftl"));
        assert_eq!(finding.line, Some(3));
        assert!(report.has_errors());
    }

    #[test]
    fn reports_duplicates() {
        let root = repo(&[("en.ftl", "a = A\na = B\n"), ("de.ftl", "a = A\n")], &["a"]);
        let report = check(root.path());
        assert!(report.errors().any(|f| f.kind == Kind::Syntax && f.message.contains("more than once")));
    }

    #[test]
    fn reports_stale_translations() {
        let root = repo(&[("en.ftl", "a = A\n"), ("de.ftl", "a = A\n\nold = Alt\n")], &["a"]);
        let report = check(root.path());
        let finding = report.errors().next().expect("stale");
        assert_eq!(finding.kind, Kind::Stale);
        assert_eq!(finding.line, Some(3));
        assert!(finding.message.contains("`old`"));
    }

    #[test]
    fn reports_variable_mismatches() {
        let en = "a = A { $x }\nb = B { $count ->\n    [one] one\n   *[other] { $count }\n}\nc = C { $x }\n";
        let de = "a = A { $y }\nb = B { NUMBER($count) }\nc = C\n";
        let root = repo(&[("en.ftl", en), ("de.ftl", de)], &["a", "b", "c"]);
        let report = check(root.path());
        let ids: Vec<&str> = report
            .errors()
            .filter(|f| f.kind == Kind::Variables)
            .map(|f| {
                if f.message.contains("`a`") {
                    "a"
                } else if f.message.contains("`c`") {
                    "c"
                } else {
                    "b"
                }
            })
            .collect();
        // b uses $count in both, inside a select and inside a function call.
        assert_eq!(ids, ["a", "c"], "{:?}", kinds(&report));
        let a = report.errors().next().unwrap();
        assert!(a.message.contains("$y") && a.message.contains("$x"), "{a}");
    }

    #[test]
    fn missing_translations_are_warnings() {
        let root = repo(&[("en.ftl", "a = A\nb = B\n"), ("de.ftl", "a = A\n")], &["a", "b"]);
        let report = check(root.path());
        assert!(!report.has_errors(), "{:?}", kinds(&report));
        let warning = report.warnings().next().expect("missing");
        assert_eq!(warning.kind, Kind::Missing);
        assert!(warning.message.contains("`b`"));
    }

    #[test]
    fn reports_unused_ids() {
        let root = repo(&[("en.ftl", "a = A\n\nunused = U\n"), ("de.ftl", "a = A\n")], &["a"]);
        let report = check(root.path());
        let finding = report.errors().next().expect("unused");
        assert_eq!(finding.kind, Kind::Unused);
        assert_eq!(finding.path, Path::new("locales/en.ftl"));
        assert_eq!(finding.line, Some(3));
    }

    #[test]
    fn ids_count_as_used_in_rust_and_single_quotes() {
        let root = repo(&[("en.ftl", "a = A\nb = B\nc = C\n"), ("de.ftl", "")], &["a"]);
        let src = root.path().join("crates/server/src/nested");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("x.rs"), "translate(\"en\", \"b\", None);").unwrap();
        fs::write(root.path().join("web/src/y.tsx"), "t('c')").unwrap();
        // Unquoted mentions and other directories don't count.
        fs::write(root.path().join("web/src/z.ts"), "// d\n").unwrap();
        fs::create_dir_all(root.path().join("web/node_modules")).unwrap();
        let report = check(root.path());
        assert!(!report.has_errors(), "{:?}", kinds(&report));
    }

    #[test]
    fn substring_is_not_a_use() {
        let root = repo(&[("en.ftl", "a = A\nab = AB\n"), ("de.ftl", "")], &["ab-c", "a"]);
        let report = check(root.path());
        assert_eq!(report.errors().map(|f| f.message.clone()).collect::<Vec<_>>().len(), 1);
        assert!(report.errors().next().unwrap().message.contains("`ab`"));
    }

    #[test]
    fn reports_manifest_problems() {
        let root = repo(&[("en.ftl", "a = A\n")], &["a"]);
        let report = check(root.path());
        let finding = report.errors().next().expect("missing de.ftl");
        assert_eq!(finding.kind, Kind::Manifest);
        assert_eq!(finding.path, Path::new("locales/de.ftl"));

        fs::write(root.path().join("locales/languages.toml"), "default = 1").unwrap();
        let report = check(root.path());
        assert_eq!(report.errors().next().unwrap().kind, Kind::Manifest);
    }

    #[test]
    fn unlisted_files_are_warnings() {
        let root = repo(&[("en.ftl", "a = A\n"), ("de.ftl", "a = A\n"), ("es.ftl", "a = A\n")], &["a"]);
        let report = check(root.path());
        assert!(!report.has_errors());
        assert_eq!(report.warnings().next().unwrap().kind, Kind::Unlisted);
    }

    #[test]
    fn errors_come_before_warnings_and_display_is_readable() {
        let root = repo(&[("en.ftl", "a = A\nb = B\n"), ("de.ftl", "c = C\n")], &["a", "b"]);
        let report = check(root.path());
        let severities: Vec<Severity> = report.findings.iter().map(Finding::severity).collect();
        let mut sorted = severities.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(severities, sorted);
        assert_eq!(
            report.findings[0].to_string(),
            "error[stale] locales/de.ftl:1: `c` doesn't exist in en.ftl, remove it"
        );
    }

    #[test]
    fn reports_literal_jsx_text() {
        let root = repo(&[("en.ftl", "a = A\n"), ("de.ftl", "a = A\n")], &["a"]);
        fs::write(root.path().join("web/src/page.tsx"), "export const P = () => <p>Hello</p>;\n").unwrap();
        let report = check(root.path());
        let finding = report.errors().next().expect("literal text");
        assert_eq!(finding.kind, Kind::LiteralText);
        assert_eq!(finding.path, Path::new("web/src/page.tsx"));
        assert_eq!(finding.line, Some(1));
        assert!(finding.message.contains("`Hello`"));
    }

    fn texts(source: &str) -> Vec<(usize, String)> {
        literal_ui_text(source)
    }

    #[test]
    fn literal_text_in_attributes() {
        assert_eq!(texts(r#"<input placeholder="Search" />"#), [(1, "Search".to_owned())]);
        assert_eq!(texts("<img alt='A cat' />"), [(1, "A cat".to_owned())]);
        assert_eq!(texts(r#"<button aria-label="Close" title="Close it">"#).len(), 2);
        // Translated values, empty values and other attributes are fine.
        assert_eq!(texts(r#"<input placeholder={t("x")} aria-label={label} />"#), []);
        assert_eq!(texts(r#"<img alt="" className="size-4" id="title" />"#), []);
        assert_eq!(texts(r#"<input placeholder="Search" /> // i18n-ignore"#), []);
    }

    #[test]
    fn literal_text_detection() {
        assert_eq!(texts("<p>Hello</p>"), [(1, "Hello".to_owned())]);
        assert_eq!(texts("<b>a</b> and <i>b</i>"), [(1, "a".to_owned()), (1, "b".to_owned())]);
        assert_eq!(texts("<p className=\"x\">\n  Hello world\n</p>"), [(2, "Hello world".to_owned())]);
        assert_eq!(texts("<p>Hello {name}"), [(1, "Hello".to_owned())]);
        assert_eq!(texts("<p>\n  Hi {name}</p>"), [(2, "Hi".to_owned())]);
        assert_eq!(texts("<p>Hello</p> // i18n-ignore"), []);
    }

    #[test]
    fn literal_text_ignores_code() {
        let code = r#"
import type * as React from "react";
const bundles = new Map<string, FluentBundle>();
const chain = [a, b].filter((b) => b !== undefined);
type A = Promise<X> | Promise<Y>;
const ok = a > limit && b < max;
export function f(): Array<string> {
  return [];
}
<h1 className="text-2xl">{t("x-title")}</h1>
<p className="text-sm">
  {instance.isPending && t("x-loading")}
</p>
<span>&nbsp;</span>
<span>42</span>
// <p>Hello</p>
{/* <p>Hello</p> */}
<Comp data-slot="button" className={cn(buttonVariants({ variant, size, className }))} {...props} />
"#;
        assert_eq!(texts(code), []);
    }

    #[test]
    fn the_repo_passes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let report = check(&root);
        let errors: Vec<String> = report.errors().map(ToString::to_string).collect();
        assert!(errors.is_empty(), "{errors:#?}");
    }
}
