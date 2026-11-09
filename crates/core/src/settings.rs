//! Instance settings stored in the database (the `settings` table), on top of the env.
//!
//! Keys are env var names from [`laneway_config::SETTINGS`]. Values are validated with the
//! setting's own parser before they are saved, so a bad value can't stop laneway from starting.
//! Secret settings are encrypted with [`Cipher`] ([`Purpose::Settings`], the key as associated
//! data). Env-only settings and unknown keys are refused when saving and ignored when loading.

use std::collections::BTreeMap;

use laneway_config::{SETTINGS, Setting, Source};
use laneway_db::PgPool;
use laneway_db::settings::{self as store, SettingRow};
use uuid::Uuid;

use crate::crypto::{Cipher, Purpose};

/// The cipher for secret settings.
pub fn cipher(secret_key: &str) -> Cipher {
    Cipher::new(secret_key, Purpose::Settings)
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("unknown setting {0}, see docs/configuration/environment-variables.md")]
    Unknown(String),
    #[error("{0} can only be set with an env var")]
    EnvOnly(&'static str),
    #[error("{0}: the value is empty, unset the setting instead")]
    Empty(&'static str),
    #[error("{env}: {message}")]
    Invalid { env: &'static str, message: String },
    #[error(transparent)]
    Database(#[from] laneway_db::Error),
}

/// A stored setting that is ignored, with the reason. Logged as a warning at startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    pub key: String,
    pub reason: String,
}

/// Usable values from the `settings` table, decrypted, keyed by env var name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stored {
    pub values: BTreeMap<String, String>,
    pub skipped: Vec<Skipped>,
}

/// Loads every stored setting. Rows that can't be used (unknown key, env-only setting, can't
/// be decrypted, no longer valid) end up in [`Stored::skipped`] instead of failing, so the env
/// or the default applies.
pub async fn load(pool: &PgPool, cipher: &Cipher) -> Result<Stored, laneway_db::Error> {
    Ok(from_rows(SETTINGS, cipher, store::all(pool).await?))
}

/// Adds the stored settings to an env [`Source`]. Returns the skipped rows too.
pub async fn with_database(
    pool: &PgPool,
    cipher: &Cipher,
    source: Source,
) -> Result<(Source, Vec<Skipped>), laneway_db::Error> {
    let stored = load(pool, cipher).await?;
    Ok((source.with_database(stored.values), stored.skipped))
}

/// Validates and saves a setting. Secret settings are encrypted.
pub async fn set(
    pool: &PgPool,
    cipher: &Cipher,
    key: &str,
    value: &str,
    updated_by: Option<Uuid>,
) -> Result<(), SettingsError> {
    set_in(SETTINGS, pool, cipher, key, value, updated_by).await
}

/// Removes a stored setting, so the env or the default applies again. Any key can be removed,
/// also unknown ones left over from older versions. Returns `false` if it wasn't stored.
pub async fn unset(pool: &PgPool, key: &str) -> Result<bool, SettingsError> {
    Ok(store::delete(pool, key).await?)
}

fn find<'a>(settings: &'a [Setting], key: &str) -> Option<&'a Setting> {
    settings.iter().find(|s| s.env == key)
}

fn from_rows(settings: &[Setting], cipher: &Cipher, rows: Vec<SettingRow>) -> Stored {
    let mut stored = Stored::default();
    for row in rows {
        match usable_value(settings, cipher, &row) {
            Ok(value) => {
                stored.values.insert(row.key, value);
            }
            Err(reason) => stored.skipped.push(Skipped { key: row.key, reason }),
        }
    }
    stored
}

fn usable_value(settings: &[Setting], cipher: &Cipher, row: &SettingRow) -> Result<String, String> {
    let Some(setting) = find(settings, &row.key) else {
        return Err("unknown setting, ignoring the stored value".to_owned());
    };
    if setting.env_only {
        return Err("env var only, ignoring the stored value".to_owned());
    }
    let value = if row.encrypted {
        cipher
            .decrypt(&row.value, row.key.as_bytes())
            .map_err(|e| format!("{e}, ignoring the stored value"))?
    } else {
        row.value.clone()
    };
    setting
        .validate(&value)
        .map_err(|message| format!("stored value is invalid ({message}), using the env or the default"))?;
    Ok(value)
}

async fn set_in(
    settings: &[Setting],
    pool: &PgPool,
    cipher: &Cipher,
    key: &str,
    value: &str,
    updated_by: Option<Uuid>,
) -> Result<(), SettingsError> {
    let setting = find(settings, key).ok_or_else(|| SettingsError::Unknown(key.to_owned()))?;
    if setting.env_only {
        return Err(SettingsError::EnvOnly(setting.env));
    }
    if value.is_empty() {
        return Err(SettingsError::Empty(setting.env));
    }
    setting.validate(value).map_err(|message| SettingsError::Invalid { env: setting.env, message })?;
    if setting.secret {
        let encrypted = cipher.encrypt(value, setting.env.as_bytes());
        store::upsert(pool, setting.env, &encrypted, true, updated_by).await?;
    } else {
        store::upsert(pool, setting.env, value, false, updated_by).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use laneway_config::{Config, Kind, Origin, setting, validate_as};

    use super::*;

    const KEY: &str = "test-secret-key-not-for-production-0000";

    /// No setting is a database-stored secret yet, so the tests bring their own.
    static TEST_SETTINGS: &[Setting] = &[
        Setting {
            env: "LANEWAY_TEST_PASSWORD",
            area: laneway_config::Area::General,
            kind: Kind::Secret { min_length: 8 },
            default: None,
            secret: true,
            required: false,
            env_only: false,
            example: "password",
            description: &["Test secret."],
            validator: validate_as::<laneway_config::Secret>,
        },
        Setting {
            env: "LANEWAY_TEST_COUNT",
            area: laneway_config::Area::General,
            kind: Kind::Integer { min: 1, max: 10 },
            default: Some("1"),
            secret: false,
            required: false,
            env_only: false,
            example: "1",
            description: &["Test number."],
            validator: validate_as::<u32>,
        },
        Setting {
            env: "LANEWAY_TEST_BOOTSTRAP",
            area: laneway_config::Area::General,
            kind: Kind::String,
            default: None,
            secret: false,
            required: false,
            env_only: true,
            example: "x",
            description: &["Test env-only setting."],
            validator: validate_as::<String>,
        },
    ];

    fn env() -> Vec<(&'static str, String)> {
        vec![
            ("LANEWAY_PUBLIC_URL", "https://laneway.example.com".into()),
            ("LANEWAY_SECRET_KEY", KEY.into()),
            ("LANEWAY_DATABASE_URL", "postgres://u:p@db/laneway".into()),
        ]
    }

    async fn raw(pool: &PgPool, key: &str) -> SettingRow {
        store::all(pool).await.unwrap().into_iter().find(|r| r.key == key).expect("stored")
    }

    #[sqlx::test(migrator = "laneway_db::MIGRATOR")]
    async fn secrets_are_stored_encrypted(pool: PgPool) {
        let c = cipher(KEY);
        let user = laneway_db::new_id();
        set_in(TEST_SETTINGS, &pool, &c, "LANEWAY_TEST_PASSWORD", "hunter22", Some(user)).await.unwrap();

        let row = raw(&pool, "LANEWAY_TEST_PASSWORD").await;
        assert!(row.encrypted);
        assert!(crate::crypto::is_encrypted(&row.value), "{}", row.value);
        assert!(!row.value.contains("hunter22"));
        assert_eq!(row.updated_by, Some(user));

        let stored = from_rows(TEST_SETTINGS, &c, store::all(&pool).await.unwrap());
        assert_eq!(stored.values["LANEWAY_TEST_PASSWORD"], "hunter22");
        assert_eq!(stored.skipped, []);
    }

    #[sqlx::test(migrator = "laneway_db::MIGRATOR")]
    async fn plain_settings_are_stored_as_text(pool: PgPool) {
        let c = cipher(KEY);
        set_in(TEST_SETTINGS, &pool, &c, "LANEWAY_TEST_COUNT", "7", None).await.unwrap();
        let row = raw(&pool, "LANEWAY_TEST_COUNT").await;
        assert_eq!((row.value.as_str(), row.encrypted, row.updated_by), ("7", false, None));

        // Setting it again replaces the value.
        set_in(TEST_SETTINGS, &pool, &c, "LANEWAY_TEST_COUNT", "8", None).await.unwrap();
        let stored = from_rows(TEST_SETTINGS, &c, store::all(&pool).await.unwrap());
        assert_eq!(stored.values.len(), 1);
        assert_eq!(stored.values["LANEWAY_TEST_COUNT"], "8");

        assert!(unset(&pool, "LANEWAY_TEST_COUNT").await.unwrap());
        assert!(!unset(&pool, "LANEWAY_TEST_COUNT").await.unwrap());
        assert_eq!(store::all(&pool).await.unwrap(), []);
    }

    #[sqlx::test(migrator = "laneway_db::MIGRATOR")]
    async fn refuses_unknown_env_only_empty_and_invalid(pool: PgPool) {
        let c = cipher(KEY);
        let set = |key: &'static str, value: &'static str| {
            let pool = pool.clone();
            let c = c.clone();
            async move { set_in(TEST_SETTINGS, &pool, &c, key, value, None).await.unwrap_err() }
        };
        assert!(matches!(set("LANEWAY_NOPE", "1").await, SettingsError::Unknown(k) if k == "LANEWAY_NOPE"));
        assert!(matches!(set("LANEWAY_TEST_BOOTSTRAP", "x").await, SettingsError::EnvOnly(_)));
        assert!(matches!(set("LANEWAY_TEST_COUNT", "").await, SettingsError::Empty(_)));
        let err = set("LANEWAY_TEST_COUNT", "11").await;
        assert_eq!(err.to_string(), "LANEWAY_TEST_COUNT: expected integer (1-10), got \"11\"");
        let err = set("LANEWAY_TEST_PASSWORD", "short").await;
        assert!(matches!(err, SettingsError::Invalid { .. }), "{err}");
        assert_eq!(store::all(&pool).await.unwrap(), [], "nothing was saved");

        // The real settings list refuses bootstrap settings.
        let err = super::set(&pool, &c, "LANEWAY_HTTP_PORT", "4000", None).await.unwrap_err();
        assert_eq!(err.to_string(), "LANEWAY_HTTP_PORT can only be set with an env var");
        let err = super::set(&pool, &c, "LANEWAY_SECRET_KEY", &"x".repeat(40), None).await.unwrap_err();
        assert!(matches!(err, SettingsError::EnvOnly(_)));
    }

    #[sqlx::test(migrator = "laneway_db::MIGRATOR")]
    async fn unusable_rows_are_skipped(pool: PgPool) {
        let c = cipher(KEY);
        // Saved with another secret key.
        let other = cipher("another-secret-key-not-for-production-00");
        let foreign = other.encrypt("hunter22", b"LANEWAY_TEST_PASSWORD");
        store::upsert(&pool, "LANEWAY_TEST_PASSWORD", &foreign, true, None).await.unwrap();
        // Written by an older version, or by hand.
        store::upsert(&pool, "LANEWAY_TEST_COUNT", "99", false, None).await.unwrap();
        store::upsert(&pool, "LANEWAY_TEST_BOOTSTRAP", "x", false, None).await.unwrap();
        store::upsert(&pool, "LANEWAY_GONE", "x", false, None).await.unwrap();

        let stored = from_rows(TEST_SETTINGS, &c, store::all(&pool).await.unwrap());
        assert_eq!(stored.values, BTreeMap::new());
        let reasons: BTreeMap<_, _> =
            stored.skipped.iter().map(|s| (s.key.as_str(), s.reason.as_str())).collect();
        assert_eq!(reasons.len(), 4);
        assert!(reasons["LANEWAY_TEST_PASSWORD"].contains("LANEWAY_SECRET_KEY probably changed"));
        assert!(reasons["LANEWAY_TEST_COUNT"].contains("stored value is invalid"));
        assert!(reasons["LANEWAY_TEST_BOOTSTRAP"].contains("env var only"));
        assert!(reasons["LANEWAY_GONE"].contains("unknown setting"));
    }

    #[sqlx::test(migrator = "laneway_db::MIGRATOR")]
    async fn config_is_rebuilt_from_env_and_database(pool: PgPool) {
        let c = cipher(KEY);
        set(&pool, &c, "LANEWAY_WORKER_CONCURRENCY_COUNT", "9", None).await.unwrap();
        set(&pool, &c, "LANEWAY_DEFAULT_LOCALE", "de", None).await.unwrap();
        // An env-only value that got into the table anyway, and a value that no longer parses.
        store::upsert(&pool, "LANEWAY_HTTP_PORT", "4000", false, None).await.unwrap();
        store::upsert(&pool, "LANEWAY_TRUST_PROXY_HEADERS", "maybe", false, None).await.unwrap();

        let mut vars = env();
        vars.push(("LANEWAY_DEFAULT_LOCALE", "fr".into()));
        let (source, skipped) = with_database(&pool, &c, Source::from_env(vars)).await.unwrap();
        let config = Config::from_source(&source).unwrap();
        assert_eq!(config.worker_concurrency, 9, "from the database");
        assert_eq!(config.default_locale, "fr", "env wins");
        assert_eq!(config.http_port, 3000, "env-only, database ignored");
        assert!(!config.trust_proxy_headers, "invalid stored value falls back to the default");

        let locale = setting("LANEWAY_DEFAULT_LOCALE").unwrap();
        assert!(source.is_locked(locale));
        assert_eq!(source.database_value(locale), Some("de"));
        let workers = setting("LANEWAY_WORKER_CONCURRENCY_COUNT").unwrap();
        assert_eq!(source.lookup(workers).unwrap().unwrap().1, Origin::Database);
        let keys: Vec<_> = skipped.iter().map(|s| s.key.as_str()).collect();
        assert_eq!(keys, ["LANEWAY_HTTP_PORT", "LANEWAY_TRUST_PROXY_HEADERS"]);
    }
}
