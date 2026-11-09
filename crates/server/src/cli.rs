use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use laneway_config::{Config, SETTINGS, Source};

use crate::settings::{self, SettingsCommand};
use crate::{http, logging, worker};

/// Paths of the generated config files, relative to the repo root.
const ENV_DOCS_PATH: &str = "docs/configuration/environment-variables.md";
const ENV_EXAMPLE_PATH: &str = "examples/.env.example";

#[derive(Debug, Parser)]
#[command(name = "laneway", version, about = "self-hostable kanban with an MCP server")]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run laneway (default). `LANEWAY_PROCESS_ROLE` picks app, worker or both.
    Serve,
    /// Check that the local app answers on /healthz (for Docker healthchecks).
    Health,
    /// Print the OpenAPI spec of `/api/v1` as JSON (used to generate the web UI's API client).
    Openapi,
    /// Configuration tools.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Settings stored in the database (until the settings UI exists).
    Settings {
        #[command(subcommand)]
        command: SettingsCommand,
    },
    /// Translation tools.
    I18n {
        #[command(subcommand)]
        command: I18nCommand,
    },
}

#[derive(Debug, Subcommand)]
enum I18nCommand {
    /// Check `locales/` and the source for broken, stale, unused or hardcoded UI text (for CI).
    Check {
        /// Repo root.
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Validate the current env and print the result.
    Check,
    /// Regenerate the env var reference and `.env.example`.
    Docs {
        /// Don't write, fail if the files are out of date (for CI).
        #[arg(long)]
        check: bool,
        /// Repo root.
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
}

pub async fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => serve().await,
        Command::Health => health(),
        Command::Openapi => {
            println!("{}", serde_json::to_string_pretty(&crate::api::openapi())?);
            Ok(ExitCode::SUCCESS)
        }
        Command::Config { command: ConfigCommand::Check } => check(),
        Command::Config { command: ConfigCommand::Docs { check, root } } => docs(&root, check),
        Command::Settings { command } => settings::run(command).await,
        Command::I18n { command: I18nCommand::Check { root } } => Ok(i18n_check(&root)),
    }
}

fn load() -> Result<(Config, Vec<String>)> {
    let source = Source::from_process_env();
    let unknown = source.unknown_vars(SETTINGS).into_iter().map(str::to_owned).collect();
    let config = Config::from_source(&source)?;
    Ok((config, unknown))
}

async fn serve() -> Result<ExitCode> {
    let (config, unknown) = load()?;
    let _log_guards = logging::init(&config)?;
    for var in unknown {
        tracing::warn!("unknown env var {var}, typo?");
    }
    tracing::info!(version = env!("CARGO_PKG_VERSION"), role = %config.process_role, "starting laneway");

    let pool = laneway_db::connect(config.database_url.as_str(), config.database_max_connections).await?;
    laneway_db::migrate(&pool).await?;
    tracing::info!("database ready");
    let config = settings::load_with_database(&pool, &config).await?;
    initial_admin(&config, &pool).await?;
    if config.process_role.runs_app()
        && let Some(token) = laneway_core::accounts::issue_setup_token(&pool).await?
    {
        let url =
            config.public_url.join("setup").map_or_else(|_| config.public_url.to_string(), |u| u.to_string());
        tracing::warn!("no admin yet. open {url} and use this one-time setup token: {token}");
    }

    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let mut tasks = tokio::task::JoinSet::new();
    if config.process_role.runs_app() {
        tasks.spawn(http::serve(config.clone(), pool.clone(), stop_rx.clone()));
    }
    if config.process_role.runs_worker() {
        tasks.spawn(worker::run(config.clone(), pool.clone(), stop_rx));
    }

    // Stop on a signal, or as soon as one part dies (so Docker restarts the container).
    let mut failed = false;
    tokio::select! {
        () = shutdown_signal() => tracing::info!("shutting down"),
        Some(result) = tasks.join_next() => {
            failed = true;
            match result {
                Ok(Ok(())) => tracing::error!("a laneway task stopped unexpectedly"),
                Ok(Err(e)) => tracing::error!("{e:#}"),
                Err(e) => tracing::error!("a laneway task panicked: {e}"),
            }
        }
    }
    let _ = stop_tx.send(true);
    while let Some(result) = tasks.join_next().await {
        if !matches!(result, Ok(Ok(()))) {
            failed = true;
        }
    }
    pool.close().await;
    Ok(if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS })
}

/// Creates the admin from `LANEWAY_INITIAL_ADMIN_EMAIL` / `_PASSWORD` if both are set and no
/// admin exists yet.
async fn initial_admin(config: &Config, pool: &laneway_db::PgPool) -> Result<()> {
    let (email, password) = match (&config.initial_admin_email, &config.initial_admin_password) {
        (Some(email), Some(password)) => (email, password),
        (None, None) => return Ok(()),
        _ => anyhow::bail!(
            "set both LANEWAY_INITIAL_ADMIN_EMAIL and LANEWAY_INITIAL_ADMIN_PASSWORD, or neither"
        ),
    };
    let username = initial_username(email);
    let account = laneway_core::accounts::NewAccount {
        email: email.clone(),
        name: username.clone(),
        username,
        password: password.expose().to_owned(),
    };
    if laneway_core::accounts::ensure_initial_admin(pool, &account)
        .await
        .context("creating the admin from LANEWAY_INITIAL_ADMIN_EMAIL")?
        .is_some()
    {
        tracing::info!(
            "created the first admin from LANEWAY_INITIAL_ADMIN_EMAIL, you can remove both vars now"
        );
    }
    Ok(())
}

/// `Alex.Smith+x@example.com` -> `Alex.Smith-x`: the part before the @, with characters
/// usernames don't allow replaced by `-`.
fn initial_username(email: &str) -> String {
    let local = email.split('@').next().unwrap_or_default();
    let cleaned: String = local
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-') { c } else { '-' })
        .collect();
    let trimmed = cleaned.trim_start_matches(|c: char| !c.is_ascii_alphanumeric());
    let name: String = trimmed.chars().take(32).collect();
    if name.chars().count() >= 2 { name } else { "admin".to_owned() }
}

/// Plain HTTP/1.0 request, so the image doesn't need curl. Reads only the port, not the whole
/// config, so it works even if other settings are broken.
fn health() -> Result<ExitCode> {
    use std::io::{Read, Write};
    use std::time::Duration;

    let port = match std::env::var("LANEWAY_HTTP_PORT") {
        Ok(p) if !p.is_empty() => p,
        _ => setting_default("LANEWAY_HTTP_PORT").to_owned(),
    };
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port.parse::<u16>()?))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(b"GET /healthz HTTP/1.0\r\nHost: localhost\r\n\r\n")?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    if response.starts_with("HTTP/1.0 200") || response.starts_with("HTTP/1.1 200") {
        Ok(ExitCode::SUCCESS)
    } else {
        eprintln!("unhealthy: {}", response.lines().next().unwrap_or("no response"));
        Ok(ExitCode::FAILURE)
    }
}

fn setting_default(env: &str) -> &'static str {
    laneway_config::setting(env).and_then(|s| s.default).expect("setting has a default")
}

fn check() -> Result<ExitCode> {
    let (config, unknown) = load()?;
    for var in unknown {
        println!("warning: unknown env var {var}, typo?");
    }
    println!("config ok (role: {}, port: {})", config.process_role, config.http_port);
    Ok(ExitCode::SUCCESS)
}

fn docs(root: &Path, check: bool) -> Result<ExitCode> {
    let files = [
        (root.join(ENV_DOCS_PATH), laneway_config::render_markdown()),
        (root.join(ENV_EXAMPLE_PATH), laneway_config::render_env_example()),
    ];
    let mut outdated = false;
    for (path, content) in files {
        let current = fs::read_to_string(&path).unwrap_or_default();
        if current == content {
            continue;
        }
        if check {
            eprintln!("{} is out of date, run `./dev.sh cargo run -- config docs`", path.display());
            outdated = true;
        } else {
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir)?;
            }
            fs::write(&path, content).with_context(|| format!("writing {}", path.display()))?;
            println!("wrote {}", path.display());
        }
    }
    Ok(if outdated { ExitCode::FAILURE } else { ExitCode::SUCCESS })
}

fn i18n_check(root: &Path) -> ExitCode {
    let report = laneway_i18n::check(root);
    for finding in &report.findings {
        eprintln!("{finding}");
    }
    let errors = report.errors().count();
    let warnings = report.warnings().count();
    println!("i18n check: {errors} errors, {warnings} warnings");
    if report.has_errors() { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_username_from_email() {
        assert_eq!(initial_username("alex@example.com"), "alex");
        assert_eq!(initial_username("Alex.Smith+work@example.com"), "Alex.Smith-work");
        assert_eq!(initial_username("_x@example.com"), "admin");
        assert_eq!(initial_username(&format!("{}@example.com", "a".repeat(40))).len(), 32);
    }

    #[test]
    fn parses_i18n_check() {
        let cli = Cli::try_parse_from(["laneway", "i18n", "check", "--root", "some/dir"]).unwrap();
        let Some(Command::I18n { command: I18nCommand::Check { root } }) = cli.command else {
            panic!("{cli:?}")
        };
        assert_eq!(root, Path::new("some/dir"));
    }

    #[test]
    fn i18n_check_passes_on_the_repo() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        assert_eq!(i18n_check(&root), ExitCode::SUCCESS);
    }

    #[test]
    fn i18n_check_fails_on_errors() {
        let root = tempfile::tempdir().unwrap();
        // No locales/ at all.
        assert_eq!(i18n_check(root.path()), ExitCode::FAILURE);
    }
}
