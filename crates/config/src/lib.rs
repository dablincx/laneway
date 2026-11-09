//! laneway: typed configuration.
//!
//! Every setting is declared exactly once in the [`settings!`] block below. That one
//! declaration produces the typed [`Config`] struct, env var parsing (incl. `_FILE`
//! variants for secrets), the env var reference in `docs/` and `examples/.env.example`.
//!
//! Precedence: env var > DB setting > default. Env-only settings (bootstrap values needed
//! before the DB is reachable, or that only apply at startup) ignore DB values.

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
    Worker,
    Logging,
}

impl Area {
    pub const ALL: &[Area] =
        &[Area::General, Area::Database, Area::Http, Area::Storage, Area::Worker, Area::Logging];

    pub fn title(self) -> &'static str {
        match self {
            Area::General => "General",
            Area::Database => "Database",
            Area::Http => "HTTP",
            Area::Storage => "Storage",
            Area::Worker => "Worker",
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
    /// Only read from the env. Values stored in the DB are ignored and the settings UI
    /// doesn't offer it. For bootstrap settings needed before the DB is reachable, and for
    /// ones that only apply at startup.
    pub env_only: bool,
    /// Example value for the docs and `.env.example`.
    pub example: &'static str,
    /// Doc comment lines.
    pub description: &'static [&'static str],
    /// Parses a raw value with the setting's own Rust type, discarding the result.
    pub validator: fn(&str, Kind) -> Result<(), String>,
}

impl Setting {
    pub fn description(&self) -> String {
        self.description.iter().map(|l| l.trim()).collect::<Vec<_>>().join(" ")
    }

    /// Checks a raw value the same way startup does. Returns the error message on failure.
    pub fn validate(&self, raw: &str) -> Result<(), String> {
        (self.validator)(raw, self.kind)
    }
}

/// [`Setting::validator`] for a setting of Rust type `T`.
pub fn validate_as<T: SettingValue>(raw: &str, kind: Kind) -> Result<(), String> {
    T::parse(raw, kind).map(drop)
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
            env_only: $env_only:literal,
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
                env_only: $env_only,
                example: $example,
                description: &[$($doc),+],
                validator: validate_as::<$ty>,
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
        env_only: true,
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
        env_only: true,
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
        env_only: true,
        example: "all",
    }

    /// Language for new users and logged-out pages.
    default_locale: String {
        env: "LANEWAY_DEFAULT_LOCALE",
        area: General,
        kind: Kind::Locale,
        default: Some("en"),
        secret: false,
        env_only: false,
        example: "en",
    }

    /// Creates the first admin on start with this email (and `LANEWAY_INITIAL_ADMIN_PASSWORD`),
    /// so the setup page is skipped. Only used while no admin exists. The username is taken
    /// from the part before the @.
    initial_admin_email: Option<String> {
        env: "LANEWAY_INITIAL_ADMIN_EMAIL",
        area: General,
        kind: Kind::String,
        default: None,
        secret: false,
        env_only: true,
        example: "admin@example.com",
    }

    /// Password for the admin created from `LANEWAY_INITIAL_ADMIN_EMAIL`. Remove both vars
    /// once the admin exists, changing them later has no effect.
    initial_admin_password: Option<Secret> {
        env: "LANEWAY_INITIAL_ADMIN_PASSWORD",
        area: General,
        kind: Kind::Secret { min_length: 10 },
        default: None,
        secret: true,
        env_only: true,
        example: "a-long-password-only-used-once",
    }

    /// Postgres 18 connection URL. Older Postgres versions are refused at startup.
    database_url: Url {
        env: "LANEWAY_DATABASE_URL",
        area: Database,
        kind: Kind::PostgresUrl,
        default: None,
        secret: true,
        env_only: true,
        example: "postgres://laneway:password@postgres:5432/laneway",
    }


    /// Max open database connections per laneway process.
    database_max_connections: u32 {
        env: "LANEWAY_DATABASE_MAX_CONNECTIONS_COUNT",
        area: Database,
        kind: Kind::Integer { min: 1, max: 1000 },
        default: Some("10"),
        secret: false,
        env_only: true,
        example: "10",
    }

    /// Port the app listens on inside the container.
    http_port: u16 {
        env: "LANEWAY_HTTP_PORT",
        area: Http,
        kind: Kind::Integer { min: 1, max: 65535 },
        default: Some("3000"),
        secret: false,
        env_only: true,
        example: "3000",
    }

    /// Trust the `X-Forwarded-For` header from a reverse proxy. Turn this on behind a reverse proxy
    /// that sets it, so laneway sees real client IPs (sessions list, per-IP login limit). Leave it
    /// off without a proxy, otherwise clients can fake their IP.
    trust_proxy_headers: bool {
        env: "LANEWAY_TRUST_PROXY_HEADERS",
        area: Http,
        kind: Kind::Boolean,
        default: Some("false"),
        secret: false,
        env_only: false,
        example: "false",
    }

    /// Where board git repos and attachments are stored. Mount a volume here.
    data_directory: PathBuf {
        env: "LANEWAY_DATA_DIRECTORY",
        area: Storage,
        kind: Kind::Path,
        default: Some("/data"),
        secret: false,
        env_only: true,
        example: "/data",
    }


    /// How many background jobs one worker process runs at the same time.
    worker_concurrency: u32 {
        env: "LANEWAY_WORKER_CONCURRENCY_COUNT",
        area: Worker,
        kind: Kind::Integer { min: 1, max: 256 },
        default: Some("4"),
        secret: false,
        env_only: false,
        example: "4",
    }

    /// Log verbosity.
    log_level: LogLevel {
        env: "LANEWAY_LOG_LEVEL",
        area: Logging,
        kind: Kind::Enum(LogLevel::VALUES),
        default: Some("info"),
        secret: false,
        env_only: true,
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
        env_only: true,
        example: "14",
    }

}

#[cfg(test)]
mod tests;
