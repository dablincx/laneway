//! Logging: readable text on stdout (for `docker logs`) plus two daily rotated files in
//! `<data directory>/logs`: `laneway.<date>.log` (text) and `laneway.<date>.json` (JSON lines).

use std::path::Path;

use anyhow::{Context, Result};
use laneway_config::Config;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, fmt};

/// Keeps the file writers alive. Dropping it flushes the remaining log lines.
#[derive(Debug)]
pub struct LogGuards(#[allow(dead_code)] Vec<WorkerGuard>);

/// `RUST_LOG` (if set) overrides `LANEWAY_LOG_LEVEL`, for debugging.
pub fn init(config: &Config) -> Result<LogGuards> {
    let filter = || {
        EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(format!("{},tower_http=warn", config.log_level)))
    };
    let dir = config.data_directory.join("logs");
    let keep = usize::try_from(config.log_retention_days).unwrap_or(usize::MAX);
    let (text, text_guard) = tracing_appender::non_blocking(appender(&dir, "log", keep)?);
    let (json, json_guard) = tracing_appender::non_blocking(appender(&dir, "json", keep)?);

    tracing_subscriber::registry()
        .with(fmt::layer().with_filter(filter()))
        .with(fmt::layer().with_ansi(false).with_writer(text).with_filter(filter()))
        .with(fmt::layer().json().with_writer(json).with_filter(filter()))
        .try_init()
        .context("logging already initialized")?;
    Ok(LogGuards(vec![text_guard, json_guard]))
}

fn appender(dir: &Path, extension: &str, keep: usize) -> Result<RollingFileAppender> {
    std::fs::create_dir_all(dir).with_context(|| format!("can't create {}", dir.display()))?;
    RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("laneway")
        .filename_suffix(extension)
        .max_log_files(keep)
        .build(dir)
        .with_context(|| format!("can't write logs to {}", dir.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appenders_use_same_name_with_different_extension() {
        let dir = tempfile::tempdir().unwrap();
        for ext in ["log", "json"] {
            let mut file = appender(dir.path(), ext, 14).unwrap();
            std::io::Write::write_all(&mut file, b"x\n").unwrap();
        }
        let mut names: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names.len(), 2);
        let stem = |n: &str| n.rsplit_once('.').unwrap().0.to_owned();
        assert_eq!(stem(&names[0]), stem(&names[1]));
        let ext = |n: &str| Path::new(n).extension().unwrap().to_str().unwrap().to_owned();
        assert!(names[0].starts_with("laneway."));
        assert_eq!((ext(&names[0]), ext(&names[1])), ("json".into(), "log".into()));
    }
}
