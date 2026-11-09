//! laneway: Fluent loader and translation key checker.
//!
//! The UI text lives in `locales/`: `languages.toml` lists the shipped languages and names the
//! default one, and every language has a `<code>.ftl` file. [`Locales`] loads that directory for
//! the server, [`check()`] is the CI check behind `laneway i18n check`.
//!
//! Lookups work like the web UI (`web/src/i18n`): the requested language first, then the default
//! language, then the id itself. Unicode isolation marks are off, same as in the web UI.

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use fluent_bundle::FluentResource;
use fluent_bundle::concurrent::FluentBundle;
use serde::Deserialize;
use unic_langid::LanguageIdentifier;

mod check;

pub use check::{Finding, Kind, Report, Severity, check, literal_ui_text};
pub use fluent_bundle::{FluentArgs, FluentValue};

/// File in the locales directory that lists the languages.
pub const MANIFEST: &str = "languages.toml";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("reading {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{}: {message}", path.display())]
    Manifest { path: PathBuf, message: String },
    #[error("{}: language `{code}` is listed in {MANIFEST} but has no file", path.display())]
    MissingFile { code: String, path: PathBuf },
    #[error("{}: {}", path.display(), join(errors))]
    Syntax { path: PathBuf, errors: Vec<SyntaxError> },
    #[error("{}: message `{id}` is defined more than once", path.display())]
    Duplicate { path: PathBuf, id: String },
}

fn join(errors: &[SyntaxError]) -> String {
    errors.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
}

/// A Fluent syntax error with its 1-based line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

/// The parsed `languages.toml`.
#[derive(Debug, Clone)]
pub(crate) struct Manifest {
    pub default: String,
    /// Code -> native name, in file order.
    pub languages: Vec<(String, String)>,
}

#[derive(Deserialize)]
struct RawManifest {
    default: String,
    languages: toml::Table,
}

pub(crate) fn read_manifest(dir: &Path) -> Result<Manifest, Error> {
    let path = dir.join(MANIFEST);
    let source = read(&path)?;
    let manifest_error = |message: String| Error::Manifest { path: path.clone(), message };
    let raw: RawManifest = toml::from_str(&source).map_err(|e| manifest_error(e.message().to_owned()))?;
    let mut languages = Vec::new();
    for (code, name) in raw.languages {
        let Some(name) = name.as_str() else {
            return Err(manifest_error(format!("the name of `{code}` must be a string")));
        };
        if code.parse::<LanguageIdentifier>().is_err() {
            return Err(manifest_error(format!("`{code}` is not a valid language code")));
        }
        languages.push((code, name.to_owned()));
    }
    if !languages.iter().any(|(code, _)| *code == raw.default) {
        return Err(manifest_error(format!("default language `{}` is not in [languages]", raw.default)));
    }
    Ok(Manifest { default: raw.default, languages })
}

pub(crate) fn read(path: &Path) -> Result<String, Error> {
    fs::read_to_string(path).map_err(|source| Error::Io { path: path.to_owned(), source })
}

/// 1-based line of a byte offset. Syntax errors use the start of the broken entry.
pub(crate) fn line_of(source: &str, offset: usize) -> usize {
    source.get(..offset).unwrap_or(source).matches('\n').count() + 1
}

/// All languages from a locales directory, ready to translate. `Send + Sync`.
pub struct Locales {
    default: String,
    languages: Vec<(String, String)>,
    bundles: HashMap<String, FluentBundle<FluentResource>>,
}

impl fmt::Debug for Locales {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Locales")
            .field("default", &self.default)
            .field("languages", &self.languages)
            .finish_non_exhaustive()
    }
}

impl Locales {
    /// Loads `languages.toml` and every listed `<code>.ftl` from `dir`.
    pub fn load(dir: &Path) -> Result<Self, Error> {
        let manifest = read_manifest(dir)?;
        let mut bundles = HashMap::new();
        for (code, _) in &manifest.languages {
            let path = dir.join(format!("{code}.ftl"));
            if !path.is_file() {
                return Err(Error::MissingFile { code: code.clone(), path });
            }
            let source = read(&path)?;
            let resource = FluentResource::try_new(source.clone()).map_err(|(_, errors)| Error::Syntax {
                path: path.clone(),
                errors: errors
                    .into_iter()
                    .map(|e| SyntaxError {
                        line: line_of(&source, e.slice.as_ref().map_or(e.pos.start, |s| s.start)),
                        message: e.kind.to_string(),
                    })
                    .collect(),
            })?;
            let langid: LanguageIdentifier = code.parse().map_err(|_| Error::Manifest {
                path: dir.join(MANIFEST),
                message: format!("`{code}` is not a valid language code"),
            })?;
            let mut bundle = FluentBundle::new_concurrent(vec![langid]);
            bundle.set_use_isolating(false);
            // NUMBER() and DATETIME(), like the web UI. Only fails if they already exist.
            let _ = bundle.add_builtins();
            if let Err(errors) = bundle.add_resource(resource) {
                let id = errors
                    .into_iter()
                    .find_map(|e| match e {
                        fluent_bundle::FluentError::Overriding { id, .. } => Some(id),
                        _ => None,
                    })
                    .unwrap_or_default();
                return Err(Error::Duplicate { path, id });
            }
            bundles.insert(code.clone(), bundle);
        }
        Ok(Self { default: manifest.default, languages: manifest.languages, bundles })
    }

    /// Code of the default (and fallback) language.
    pub fn default_locale(&self) -> &str {
        &self.default
    }

    /// `(code, native name)` of every language, in `languages.toml` order.
    pub fn languages(&self) -> impl Iterator<Item = (&str, &str)> {
        self.languages.iter().map(|(code, name)| (code.as_str(), name.as_str()))
    }

    /// Best supported language for an `Accept-Language` header or a comma separated list of
    /// codes, see [`negotiate`].
    pub fn negotiate(&self, requested: &str) -> &str {
        let available: Vec<&str> = self.languages.iter().map(|(code, _)| code.as_str()).collect();
        negotiate(requested, &available, &self.default)
    }

    /// Formats message `id` in `locale`. Falls back to the default language if the message is
    /// missing, and returns `id` itself if no language has it. `locale` goes through
    /// [`Self::negotiate`], so `de-AT` uses `de` and unsupported locales use the default.
    pub fn translate(&self, locale: &str, id: &str, args: Option<&FluentArgs<'_>>) -> String {
        let locale = self.negotiate(locale);
        let chain = if locale == self.default { &[locale][..] } else { &[locale, self.default.as_str()] };
        for code in chain {
            let Some(bundle) = self.bundles.get(*code) else { continue };
            if let Some(pattern) = bundle.get_message(id).and_then(|m| m.value()) {
                let mut errors = Vec::new();
                return bundle.format_pattern(pattern, args, &mut errors).into_owned();
            }
        }
        id.to_owned()
    }
}

/// Picks the best of `available` for `requested`, an `Accept-Language` header
/// (`de-AT,de;q=0.9,en;q=0.8`) or a plain list (`de-AT, fr`). Tags are tried by quality, highest
/// first; each tag matches exactly (ignoring case, `_` counts as `-`), then by its language
/// prefix (`de-AT` -> `de`). Nothing matches: `default`.
pub fn negotiate<'a>(requested: &str, available: &[&'a str], default: &'a str) -> &'a str {
    let mut tags: Vec<(String, f32)> = requested
        .split(',')
        .filter_map(|part| {
            let mut pieces = part.split(';');
            let tag = pieces.next()?.trim().replace('_', "-");
            let quality = pieces
                .find_map(|p| p.trim().strip_prefix("q="))
                .map_or(Some(1.0), |q| q.trim().parse::<f32>().ok())?;
            (!tag.is_empty() && tag != "*" && quality > 0.0).then_some((tag, quality))
        })
        .collect();
    // Stable, so equal qualities keep their order.
    tags.sort_by(|a, b| b.1.total_cmp(&a.1));
    for (tag, _) in &tags {
        if let Some(code) = available.iter().find(|code| code.eq_ignore_ascii_case(tag)) {
            return code;
        }
        let language = tag.split('-').next().unwrap_or_default();
        if let Some(code) = available.iter().find(|code| code.eq_ignore_ascii_case(language)) {
            return code;
        }
    }
    default
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn write_locales(dir: &Path, manifest: &str, files: &[(&str, &str)]) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(MANIFEST), manifest).unwrap();
        for (name, content) in files {
            fs::write(dir.join(name), content).unwrap();
        }
    }

    const MANIFEST_EN_DE: &str = "default = \"en\"\n[languages]\nen = \"English\"\nde = \"Deutsch\"\n";

    fn sample() -> (tempfile::TempDir, Locales) {
        let dir = tempfile::tempdir().unwrap();
        write_locales(
            dir.path(),
            MANIFEST_EN_DE,
            &[
                ("en.ftl", "hello = hello { $name }\nonly-en = english only\nattr-only =\n    .title = t\n"),
                ("de.ftl", "hello = hallo { $name }\n"),
            ],
        );
        let locales = Locales::load(dir.path()).unwrap();
        (dir, locales)
    }

    fn args(name: &str) -> FluentArgs<'static> {
        let mut args = FluentArgs::new();
        args.set("name", name.to_owned());
        args
    }

    #[test]
    fn locales_are_send_and_sync() {
        fn assert<T: Send + Sync>() {}
        assert::<Locales>();
    }

    #[test]
    fn translates_without_isolation_marks() {
        let (_dir, locales) = sample();
        assert_eq!(locales.translate("de", "hello", Some(&args("Ana"))), "hallo Ana");
        assert_eq!(locales.translate("en", "hello", Some(&args("Ana"))), "hello Ana");
    }

    #[test]
    fn falls_back_to_default_language() {
        let (_dir, locales) = sample();
        assert_eq!(locales.translate("de", "only-en", None), "english only");
    }

    #[test]
    fn unknown_id_returns_id() {
        let (_dir, locales) = sample();
        assert_eq!(locales.translate("de", "nope", None), "nope");
        // A message with only attributes has no value to show.
        assert_eq!(locales.translate("en", "attr-only", None), "attr-only");
    }

    #[test]
    fn unsupported_locale_uses_default_and_regions_use_language() {
        let (_dir, locales) = sample();
        assert_eq!(locales.translate("xx", "hello", Some(&args("Ana"))), "hello Ana");
        assert_eq!(locales.translate("de-AT", "hello", Some(&args("Ana"))), "hallo Ana");
    }

    #[test]
    fn keeps_manifest_order() {
        let (_dir, locales) = sample();
        assert_eq!(locales.default_locale(), "en");
        assert_eq!(locales.languages().collect::<Vec<_>>(), [("en", "English"), ("de", "Deutsch")]);
    }

    #[test]
    fn negotiates() {
        let available = ["en", "de", "fr"];
        assert_eq!(negotiate("de", &available, "en"), "de");
        assert_eq!(negotiate("de-AT", &available, "en"), "de");
        assert_eq!(negotiate("DE_at", &available, "en"), "de");
        assert_eq!(negotiate("es, fr", &available, "en"), "fr");
        assert_eq!(negotiate("es-ES,es;q=0.9,fr;q=0.5,de;q=0.8", &available, "en"), "de");
        assert_eq!(negotiate("de;q=0, fr", &available, "en"), "fr");
        assert_eq!(negotiate("es", &available, "en"), "en");
        assert_eq!(negotiate("", &available, "en"), "en");
        assert_eq!(negotiate("*", &available, "en"), "en");
    }

    #[test]
    fn missing_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        write_locales(dir.path(), MANIFEST_EN_DE, &[("en.ftl", "a = b\n")]);
        let err = Locales::load(dir.path()).unwrap_err();
        assert!(matches!(&err, Error::MissingFile { code, .. } if code == "de"), "{err}");
        assert!(err.to_string().contains("de.ftl"), "{err}");
    }

    #[test]
    fn syntax_error_names_file_and_line() {
        let dir = tempfile::tempdir().unwrap();
        write_locales(
            dir.path(),
            MANIFEST_EN_DE,
            &[("en.ftl", "a = b\n"), ("de.ftl", "a = b\n\nbroken {\n")],
        );
        let err = Locales::load(dir.path()).unwrap_err();
        let Error::Syntax { path, errors } = &err else { panic!("{err}") };
        assert!(path.ends_with("de.ftl"));
        assert_eq!(errors[0].line, 3, "{err}");
        assert!(err.to_string().contains("de.ftl: line 3"), "{err}");
    }

    #[test]
    fn duplicate_message_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        write_locales(dir.path(), MANIFEST_EN_DE, &[("en.ftl", "a = b\na = c\n"), ("de.ftl", "a = b\n")]);
        let err = Locales::load(dir.path()).unwrap_err();
        assert!(matches!(&err, Error::Duplicate { id, .. } if id == "a"), "{err}");
    }

    #[test]
    fn bad_manifest_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        write_locales(dir.path(), "default = \"xx\"\n[languages]\nen = \"English\"\n", &[("en.ftl", "")]);
        let err = Locales::load(dir.path()).unwrap_err();
        assert!(err.to_string().contains("default language `xx`"), "{err}");

        write_locales(dir.path(), "default = \"en\"\n", &[]);
        assert!(matches!(Locales::load(dir.path()), Err(Error::Manifest { .. })));

        let missing = tempfile::tempdir().unwrap();
        assert!(matches!(Locales::load(missing.path()), Err(Error::Io { .. })));
    }

    #[test]
    fn loads_the_repo_locales() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../locales");
        let locales = Locales::load(&dir).unwrap();
        // No real ids here: a quoted id in this file would count as "used" for the check.
        assert_eq!(locales.default_locale(), "en");
        assert!(locales.languages().any(|(code, _)| code == "de"));
    }
}
