//! laneway: typed configuration.
//!
//! Every setting is declared exactly once in the [`settings!`] block below. That one
//! declaration produces the typed [`Config`] struct, env var parsing (incl. `_FILE`
//! variants for secrets), the env var reference in `docs/` and `examples/.env.example`.
//!
//! Precedence: env var > DB setting > default.

mod docs;
mod source;
mod value;

use std::fmt;
use std::path::PathBuf;

pub use docs::{render_env_example, render_markdown};
pub use source::{Origin, Source};
pub use url::Url;
pub use value::{Kind, Secret, SettingValue};

/// Groups settings in the docs and the settings UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Area {
    General,
    Database,
    Http,
    Storage,
    Logging,
}

impl Area {
    pub const ALL: &[Area] = &[Area::General, Area::Database, Area::Http, Area::Storage, Area::Logging];

    pub fn title(self) -> &'static str {
        match self {
            Area::General => "General",
            Area::Database => "Database",
            Area::Http => "HTTP",
            Area::Storage => "Storage",
            Area::Logging => "Logging",
        }
    }
}

/// Metadata of one setting.
#[derive(Debug, Clone, Copy)]
pub struct Setting {
    /// Env var name, e.g. `LANEWAY_HTTP_PORT`. Also the key in the DB settings table.
    pub env: &'static str,
    pub area: Area,
    pub kind: Kind,
    /// Default as text, parsed like any other value. `None` = no default.
    pub default: Option<&'static str>,
    /// Secrets accept a `_FILE` variant and are never shown in full.
    pub secret: bool,
    /// Must be set (no default and not optional).
    pub required: bool,
    /// Example value for the docs and `.env.example`.
    pub example: &'static str,
    /// Doc comment lines.
    pub description: &'static [&'static str],
}

impl Setting {
    pub fn description(&self) -> String {
        self.description.iter().map(|l| l.trim()).collect::<Vec<_>>().join(" ")
    }
}

/// Looks up a setting by env var name.
pub fn setting(env: &str) -> Option<&'static Setting> {
    SETTINGS.iter().find(|s| s.env == env)
}

/// One invalid or missing setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub env: &'static str,
    pub message: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.env, self.message)
    }
}

/// All problems found while loading the config, reported together.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub struct ConfigErrors(pub Vec<ConfigError>);

impl fmt::Display for ConfigErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "invalid configuration:")?;
        for e in &self.0 {
            writeln!(f, "  - {e}")?;
        }
        write!(f, "see docs/configuration/environment-variables.md")
    }
}

fn read<T: SettingValue>(source: &Source, setting: &Setting, errors: &mut Vec<ConfigError>) -> Option<T> {
    let message = match source.lookup(setting) {
        Err(message) => message,
        Ok(Some((raw, _))) => match T::parse(&raw, setting.kind) {
            Ok(value) => return Some(value),
            Err(message) => message,
        },
        Ok(None) => match T::unset() {
            Some(value) => return Some(value),
            None => format!("required, but not set ({})", setting.kind.describe()),
        },
    };
    errors.push(ConfigError { env: setting.env, message });
    None
}

/// Declares settings. Each entry becomes a `Config` field and a `SETTINGS` row.
macro_rules! settings {
    ($(
        $(#[doc = $doc:literal])+
        $field:ident: $ty:ty {
            env: $env:literal,
            area: $area:ident,
            kind: $kind:expr,
            default: $default:expr,
            secret: $secret:literal,
            example: $example:literal $(,)?
        }
    )*) => {
        /// The full, validated configuration.
        #[derive(Debug, Clone)]
        pub struct Config {
            $( $(#[doc = $doc])+ pub $field: $ty, )*
        }

        /// Every setting, in declaration order.
        pub static SETTINGS: &[Setting] = &[$(
            Setting {
                env: $env,
                area: Area::$area,
                kind: $kind,
                default: $default,
                secret: $secret,
                required: !<$ty as SettingValue>::OPTIONAL && Option::<&str>::is_none(&$default),
                example: $example,
                description: &[$($doc),+],
            },
        )*];

        impl Config {
            /// Parse and validate. Collects every error instead of stopping at the first.
            pub fn from_source(source: &Source) -> Result<Self, ConfigErrors> {
                let mut errors = Vec::new();
                $(
                    let $field: Option<$ty> =
                        read(source, setting($env).expect("declared above"), &mut errors);
                )*
                if !errors.is_empty() {
                    return Err(ConfigErrors(errors));
                }
                Ok(Self { $( $field: $field.expect("no errors, so every value is set"), )* })
            }
        }
    };
}

/// Declares a string enum usable as a setting type.
macro_rules! string_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident = $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const VALUES: &[&str] = &[$($text),+];

            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }
        }

        impl SettingValue for $name {
            fn parse(raw: &str, kind: Kind) -> Result<Self, String> {
                match raw.trim() {
                    $($text => Ok(Self::$variant),)+
                    _ => Err(format!("expected {}, got {raw:?}", kind.describe())),
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

string_enum! {
    /// Which part of laneway a process runs.
    ProcessRole { App = "app", Worker = "worker", All = "all" }
}

impl ProcessRole {
    pub fn runs_app(self) -> bool {
        matches!(self, Self::App | Self::All)
    }

    pub fn runs_worker(self) -> bool {
        matches!(self, Self::Worker | Self::All)
    }
}

string_enum! {
    LogLevel { Debug = "debug", Info = "info", Warn = "warn", Error = "error" }
}

settings! {
    /// Public base URL of the instance, used for links, emails and OAuth.
    public_url: Url {
        env: "LANEWAY_PUBLIC_URL",
        area: General,
        kind: Kind::Url,
        default: None,
        secret: false,
        example: "https://laneway.example.com",
    }

    /// Master key for sessions and for encrypting secrets at rest. Generate one with
    /// `openssl rand -hex 32`. Changing it logs everyone out and makes stored secrets unreadable.
    secret_key: Secret {
        env: "LANEWAY_SECRET_KEY",
        area: General,
        kind: Kind::Secret { min_length: 32 },
        default: None,
        secret: true,
        example: "replace-with-output-of-openssl-rand-hex-32",
    }

    /// Which part of laneway this container runs: the web app + API, the background
    /// worker, or both in one process (fine for small installs).
    process_role: ProcessRole {
        env: "LANEWAY_PROCESS_ROLE",
        area: General,
        kind: Kind::Enum(ProcessRole::VALUES),
        default: Some("all"),
        secret: false,
        example: "all",
    }

    /// Language for new users and logged-out pages.
    default_locale: String {
        env: "LANEWAY_DEFAULT_LOCALE",
        area: General,
        kind: Kind::Locale,
        default: Some("en"),
        secret: false,
        example: "en",
    }

    /// Postgres 18 connection URL. Older Postgres versions are refused at startup.
    database_url: Url {
        env: "LANEWAY_DATABASE_URL",
        area: Database,
        kind: Kind::PostgresUrl,
        default: None,
        secret: true,
        example: "postgres://laneway:password@postgres:5432/laneway",
    }


    /// Port the app listens on inside the container.
    http_port: u16 {
        env: "LANEWAY_HTTP_PORT",
        area: Http,
        kind: Kind::Integer { min: 1, max: 65535 },
        default: Some("3000"),
        secret: false,
        example: "3000",
    }

    /// Trust `X-Forwarded-For` / `X-Forwarded-Proto` headers. Only turn this on behind a
    /// reverse proxy that sets them, otherwise clients can fake their IP.
    trust_proxy_headers: bool {
        env: "LANEWAY_TRUST_PROXY_HEADERS",
        area: Http,
        kind: Kind::Boolean,
        default: Some("false"),
        secret: false,
        example: "false",
    }

    /// Where board git repos and attachments are stored. Mount a volume here.
    data_directory: PathBuf {
        env: "LANEWAY_DATA_DIRECTORY",
        area: Storage,
        kind: Kind::Path,
        default: Some("/data"),
        secret: false,
        example: "/data",
    }


    /// Log verbosity.
    log_level: LogLevel {
        env: "LANEWAY_LOG_LEVEL",
        area: Logging,
        kind: Kind::Enum(LogLevel::VALUES),
        default: Some("info"),
        secret: false,
        example: "info",
    }

    /// How many days of log files to keep in `<data directory>/logs`. Logs rotate daily,
    /// older files are deleted.
    log_retention_days: u32 {
        env: "LANEWAY_LOG_RETENTION_DAYS",
        area: Logging,
        kind: Kind::Integer { min: 1, max: 3650 },
        default: Some("14"),
        secret: false,
        example: "14",
    }

}

#[cfg(test)]
mod tests;
