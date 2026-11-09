use std::io::Write as _;

use super::*;

fn required() -> Vec<(&'static str, String)> {
    vec![
        ("LANEWAY_PUBLIC_URL", "https://laneway.example.com".into()),
        ("LANEWAY_SECRET_KEY", "k".repeat(32)),
        ("LANEWAY_DATABASE_URL", "postgres://u:p@db/laneway".into()),
    ]
}

fn load(vars: Vec<(&'static str, String)>) -> Result<Config, ConfigErrors> {
    Config::from_source(&Source::from_env(vars))
}

#[test]
fn defaults_apply() {
    let config = load(required()).unwrap();
    assert_eq!(config.http_port, 3000);
    assert_eq!(config.process_role, ProcessRole::All);
    assert_eq!(config.log_level, LogLevel::Info);
    assert_eq!(config.default_locale, "en");
    assert!(!config.trust_proxy_headers);
    assert_eq!(config.data_directory, PathBuf::from("/data"));
}

#[test]
fn every_default_and_example_is_valid() {
    // If a default or example doesn't parse, the docs would lie.
    let mut vars: Vec<(&str, String)> = Vec::new();
    for s in SETTINGS {
        vars.push((s.env, s.example.to_owned()));
    }
    load(vars).unwrap();
    load(required()).unwrap();
}

#[test]
fn every_setting_is_documented_and_named_well() {
    for s in SETTINGS {
        assert!(s.env.starts_with("LANEWAY_"), "{}", s.env);
        assert_eq!(s.env, s.env.to_ascii_uppercase(), "{}", s.env);
        assert!(s.description().len() > 10, "{} needs a description", s.env);
        assert!(!s.example.is_empty(), "{} needs an example", s.env);
        if matches!(s.kind, Kind::Boolean) {
            assert!(s.default.is_some(), "{}: booleans need a default so they read as statements", s.env);
        }
    }
    let mut names: Vec<_> = SETTINGS.iter().map(|s| s.env).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), SETTINGS.len(), "duplicate env var name");
}

#[test]
fn missing_required_reports_all() {
    let err = load(vec![]).unwrap_err();
    let names: Vec<_> = err.0.iter().map(|e| e.env).collect();
    assert_eq!(names, ["LANEWAY_PUBLIC_URL", "LANEWAY_SECRET_KEY", "LANEWAY_DATABASE_URL"]);
    assert!(err.to_string().contains("required, but not set"));
}

#[test]
fn invalid_values_name_the_var_and_allowed_values() {
    let mut vars = required();
    vars.push(("LANEWAY_HTTP_PORT", "70000".into()));
    vars.push(("LANEWAY_PROCESS_ROLE", "web".into()));
    let err = load(vars).unwrap_err().to_string();
    assert!(err.contains("LANEWAY_HTTP_PORT: expected integer (1-65535), got \"70000\""), "{err}");
    assert!(err.contains("LANEWAY_PROCESS_ROLE: expected one of `app`, `worker`, `all`"), "{err}");
}

#[test]
fn empty_env_counts_as_unset() {
    let mut vars = required();
    vars.push(("LANEWAY_HTTP_PORT", String::new()));
    assert_eq!(load(vars).unwrap().http_port, 3000);
}

#[test]
fn secret_from_file() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    writeln!(file, "{}", "f".repeat(40)).unwrap();
    let mut vars = required();
    vars.retain(|(k, _)| *k != "LANEWAY_SECRET_KEY");
    vars.push(("LANEWAY_SECRET_KEY_FILE", file.path().display().to_string()));
    let config = load(vars).unwrap();
    assert_eq!(config.secret_key.expose(), "f".repeat(40));
}

#[test]
fn secret_and_file_both_set_is_an_error() {
    let mut vars = required();
    vars.push(("LANEWAY_SECRET_KEY_FILE", "/run/secrets/key".into()));
    let err = load(vars).unwrap_err().to_string();
    assert!(err.contains("not both"), "{err}");
}

#[test]
fn file_variant_only_for_secrets() {
    let source = Source::from_env([("LANEWAY_HTTP_PORT_FILE", "/x")]);
    assert_eq!(source.unknown_vars(SETTINGS), ["LANEWAY_HTTP_PORT_FILE"]);
}

#[test]
fn env_beats_database_beats_default() {
    let workers = setting("LANEWAY_WORKER_CONCURRENCY_COUNT").unwrap();
    assert_eq!(load(required()).unwrap().worker_concurrency, 4);

    let db = Source::from_env(required()).with_database([("LANEWAY_WORKER_CONCURRENCY_COUNT", "8")]);
    assert_eq!(Config::from_source(&db).unwrap().worker_concurrency, 8);
    assert_eq!(db.lookup(workers).unwrap(), Some(("8".to_owned(), Origin::Database)));
    assert!(!db.is_locked(workers));

    let mut vars = required();
    vars.push(("LANEWAY_WORKER_CONCURRENCY_COUNT", "16".into()));
    let both = Source::from_env(vars).with_database([("LANEWAY_WORKER_CONCURRENCY_COUNT", "8")]);
    assert_eq!(Config::from_source(&both).unwrap().worker_concurrency, 16);
    assert!(both.is_locked(workers));
    assert_eq!(both.database_value(workers), Some("8"));
}

#[test]
fn env_only_settings_ignore_the_database() {
    let port = setting("LANEWAY_HTTP_PORT").unwrap();
    assert!(port.env_only);
    let source = Source::from_env(required()).with_database([
        ("LANEWAY_HTTP_PORT", "4000"),
        ("LANEWAY_LOG_LEVEL", "error"),
        ("LANEWAY_SECRET_KEY", "d".repeat(40).as_str()),
    ]);
    let config = Config::from_source(&source).unwrap();
    assert_eq!(config.http_port, 3000);
    assert_eq!(config.log_level, LogLevel::Info);
    assert_eq!(config.secret_key.expose(), "k".repeat(32));
    assert_eq!(source.lookup(port).unwrap(), Some(("3000".to_owned(), Origin::Default)));
    assert_eq!(source.database_value(port), None);

    // A required env-only setting can't be filled in from the database.
    let mut vars = required();
    vars.retain(|(k, _)| *k != "LANEWAY_DATABASE_URL");
    let source =
        Source::from_env(vars).with_database([("LANEWAY_DATABASE_URL", "postgres://u:p@db/laneway")]);
    let err = Config::from_source(&source).unwrap_err();
    assert_eq!(err.0[0].env, "LANEWAY_DATABASE_URL");
}

#[test]
fn bootstrap_settings_are_env_only() {
    let env_only: Vec<_> = SETTINGS.iter().filter(|s| s.env_only).map(|s| s.env).collect();
    assert_eq!(
        env_only,
        [
            "LANEWAY_SECRET_KEY",
            "LANEWAY_PROCESS_ROLE",
            "LANEWAY_INITIAL_ADMIN_EMAIL",
            "LANEWAY_INITIAL_ADMIN_PASSWORD",
            "LANEWAY_DATABASE_URL",
            "LANEWAY_DATABASE_MAX_CONNECTIONS_COUNT",
            "LANEWAY_HTTP_PORT",
            "LANEWAY_DATA_DIRECTORY",
            "LANEWAY_LOG_LEVEL",
            "LANEWAY_LOG_RETENTION_DAYS",
        ]
    );
}

#[test]
fn validate_uses_the_setting_type() {
    let port = setting("LANEWAY_HTTP_PORT").unwrap();
    assert!(port.validate("8080").is_ok());
    assert!(port.validate("70000").is_err());
    assert!(port.validate("http").is_err());
    let locale = setting("LANEWAY_DEFAULT_LOCALE").unwrap();
    assert!(locale.validate("de").is_ok());
    assert_eq!(
        locale.validate("German").unwrap_err(),
        "expected locale code (e.g. `en`, `de`, `pt-BR`), got \"German\""
    );
    let url = setting("LANEWAY_PUBLIC_URL").unwrap();
    assert!(url.validate("ftp://x").is_err());
    let flag = setting("LANEWAY_TRUST_PROXY_HEADERS").unwrap();
    assert!(flag.validate("yes").is_ok());
    assert!(flag.validate("maybe").is_err());
}

#[test]
fn unknown_vars_are_reported() {
    let source = Source::from_env([
        ("LANEWAY_HTTP_PROT", "1"),
        ("LANEWAY_SECRET_KEY_FILE", "/x"),
        ("LANEWAY_DEV_BUILD_JOBS", "2"),
        ("PATH", "/bin"),
    ]);
    assert_eq!(source.unknown_vars(SETTINGS), ["LANEWAY_HTTP_PROT"]);
}

#[test]
fn docs_mention_every_var() {
    let md = render_markdown();
    let env = render_env_example();
    for s in SETTINGS {
        assert!(md.contains(s.env), "{} missing from docs", s.env);
        assert!(env.contains(s.env), "{} missing from .env.example", s.env);
    }
    assert!(env.contains("\nLANEWAY_PUBLIC_URL=https://laneway.example.com\n"));
    assert!(env.contains("\n#LANEWAY_HTTP_PORT=3000\n"));
    assert!(env.contains("# type: integer (1-65535), env var only\n"));
    assert!(md.contains("Env var only."));
}
