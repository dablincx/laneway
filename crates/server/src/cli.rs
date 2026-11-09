use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use laneway_config::{Config, SETTINGS, Source};

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
    /// Configuration tools.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
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
        Command::Config { command: ConfigCommand::Check } => check(),
        Command::Config { command: ConfigCommand::Docs { check, root } } => docs(&root, check),
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
