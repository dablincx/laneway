//! Database settings at startup, and the `laneway settings` admin commands.

use std::fmt::Write as _;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Subcommand;
use laneway_config::{Config, Origin, SETTINGS, Source};
use laneway_core::settings::{self, Skipped};
use laneway_db::PgPool;

#[derive(Debug, Subcommand)]
pub enum SettingsCommand {
    /// Show every setting, its effective value (secrets masked) and where it comes from.
    List,
    /// Save a setting in the database. Takes effect when laneway restarts.
    Set {
        /// Env var name, e.g. `LANEWAY_DEFAULT_LOCALE`.
        name: String,
        /// The value. Read from stdin when left out, so secrets stay out of the shell history.
        value: Option<String>,
    },
    /// Remove a setting from the database, so the env var or the default applies again.
    Unset {
        /// Env var name, e.g. `LANEWAY_DEFAULT_LOCALE`.
        name: String,
    },
}

/// Rebuilds the config from the env plus the settings stored in the database. Stored values
/// that can't be used are logged and skipped, so laneway still starts with the env or default.
pub async fn load_with_database(pool: &PgPool, env_config: &Config) -> Result<Config> {
    let cipher = settings::cipher(env_config.secret_key.expose());
    let (source, skipped) = settings::with_database(pool, &cipher, Source::from_process_env()).await?;
    for Skipped { key, reason } in skipped {
        tracing::warn!("database setting {key}: {reason}");
    }
    Ok(Config::from_source(&source)?)
}

pub async fn run(command: SettingsCommand) -> Result<ExitCode> {
    let env_config = Config::from_source(&Source::from_process_env())?;
    let pool =
        laneway_db::connect(env_config.database_url.as_str(), env_config.database_max_connections).await?;
    laneway_db::migrate(&pool).await?;
    let cipher = settings::cipher(env_config.secret_key.expose());
    let code = match command {
        SettingsCommand::List => {
            let (source, skipped) =
                settings::with_database(&pool, &cipher, Source::from_process_env()).await?;
            print!("{}", render_list(&source, &skipped));
            ExitCode::SUCCESS
        }
        SettingsCommand::Set { name, value } => {
            let value = match value {
                Some(value) => value,
                None => read_stdin_line()?,
            };
            settings::set(&pool, &cipher, &name, &value, None).await?;
            println!("saved {name}, restart laneway to apply it");
            let source = Source::from_process_env();
            if laneway_config::setting(&name).is_some_and(|s| source.is_locked(s)) {
                println!("note: {name} is also set in the env, which wins over the database");
            }
            ExitCode::SUCCESS
        }
        SettingsCommand::Unset { name } => {
            if settings::unset(&pool, &name).await? {
                println!("removed {name}, restart laneway to apply it");
            } else {
                println!("{name} isn't stored in the database");
            }
            ExitCode::SUCCESS
        }
    };
    pool.close().await;
    Ok(code)
}

fn read_stdin_line() -> Result<String> {
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).context("reading the value from stdin")?;
    Ok(line.trim_end_matches(['\n', '\r']).to_owned())
}

/// The `laneway settings list` table.
fn render_list(source: &Source, skipped: &[Skipped]) -> String {
    let rows: Vec<[String; 3]> = SETTINGS
        .iter()
        .map(|s| {
            let (value, origin) = match source.lookup(s) {
                Ok(Some((value, origin))) => {
                    (if s.secret { "***".to_owned() } else { value }, origin.label().to_owned())
                }
                Ok(None) => ("(unset)".to_owned(), String::new()),
                Err(message) => (format!("(error: {message})"), String::new()),
            };
            let mut from = origin;
            let env_wins = source
                .lookup(s)
                .is_ok_and(|v| v.is_some_and(|(_, o)| matches!(o, Origin::Env | Origin::EnvFile)));
            if env_wins && source.database_value(s).is_some() {
                from.push_str(", locked by env");
            }
            if s.env_only {
                if !from.is_empty() {
                    from.push_str(", ");
                }
                from.push_str("env var only");
            }
            [s.env.to_owned(), value, from]
        })
        .collect();

    let name_width = rows.iter().map(|r| r[0].len()).max().unwrap_or(0);
    let value_width = rows.iter().map(|r| r[1].chars().count()).max().unwrap_or(0).min(48);
    let mut out = String::new();
    let _ = writeln!(out, "{:<name_width$}  {:<value_width$}  SOURCE", "SETTING", "VALUE");
    for [name, value, from] in rows {
        let _ = writeln!(out, "{}", format!("{name:<name_width$}  {value:<value_width$}  {from}").trim_end());
    }
    for Skipped { key, reason } in skipped {
        let _ = writeln!(out, "warning: database setting {key}: {reason}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> Vec<(&'static str, String)> {
        vec![
            ("LANEWAY_PUBLIC_URL", "https://laneway.example.com".into()),
            ("LANEWAY_SECRET_KEY", "test-secret-key-not-for-production-0000".into()),
            ("LANEWAY_DATABASE_URL", "postgres://u:p@db/laneway".into()),
            ("LANEWAY_DEFAULT_LOCALE", "fr".into()),
        ]
    }

    fn line<'a>(out: &'a str, name: &str) -> &'a str {
        out.lines().find(|l| l.starts_with(&format!("{name} "))).unwrap_or_else(|| panic!("{name} in\n{out}"))
    }

    fn squash(line: &str) -> String {
        line.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn list_shows_values_sources_and_masks_secrets() {
        let source = Source::from_env(env())
            .with_database([("LANEWAY_WORKER_CONCURRENCY_COUNT", "8"), ("LANEWAY_DEFAULT_LOCALE", "de")]);
        let skipped = [Skipped { key: "LANEWAY_HTTP_PORT".into(), reason: "env var only, ignoring".into() }];
        let out = render_list(&source, &skipped);

        assert!(out.starts_with("SETTING "), "{out}");
        assert_eq!(
            squash(line(&out, "LANEWAY_PUBLIC_URL")),
            "LANEWAY_PUBLIC_URL https://laneway.example.com env"
        );
        assert_eq!(squash(line(&out, "LANEWAY_SECRET_KEY")), "LANEWAY_SECRET_KEY *** env, env var only");
        assert_eq!(squash(line(&out, "LANEWAY_DATABASE_URL")), "LANEWAY_DATABASE_URL *** env, env var only");
        assert_eq!(
            squash(line(&out, "LANEWAY_WORKER_CONCURRENCY_COUNT")),
            "LANEWAY_WORKER_CONCURRENCY_COUNT 8 database"
        );
        assert_eq!(
            squash(line(&out, "LANEWAY_DEFAULT_LOCALE")),
            "LANEWAY_DEFAULT_LOCALE fr env, locked by env"
        );
        assert_eq!(squash(line(&out, "LANEWAY_HTTP_PORT")), "LANEWAY_HTTP_PORT 3000 default, env var only");
        assert!(!out.contains("test-secret-key"), "{out}");
        assert!(
            out.ends_with("warning: database setting LANEWAY_HTTP_PORT: env var only, ignoring\n"),
            "{out}"
        );
    }

    #[test]
    fn parses_settings_commands() {
        use clap::Parser;

        use crate::cli::Cli;

        for args in [
            &["laneway", "settings", "list"][..],
            &["laneway", "settings", "set", "LANEWAY_DEFAULT_LOCALE", "de"],
            &["laneway", "settings", "set", "LANEWAY_DEFAULT_LOCALE"],
            &["laneway", "settings", "unset", "LANEWAY_DEFAULT_LOCALE"],
        ] {
            Cli::try_parse_from(args).unwrap();
        }
        assert!(Cli::try_parse_from(["laneway", "settings", "unset"]).is_err());
    }
}
