//! Where raw setting values come from: env vars (incl. `_FILE` variants), then DB settings.

use std::collections::BTreeMap;
use std::fs;

use crate::Setting;

/// Where a value came from. The UI shows env-set fields as locked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Env,
    EnvFile,
    Database,
    Default,
}

/// Raw values, before parsing. Precedence: env var > DB setting > default.
#[derive(Debug, Clone, Default)]
pub struct Source {
    env: BTreeMap<String, String>,
    database: BTreeMap<String, String>,
}

impl Source {
    /// Build from the process environment.
    pub fn from_process_env() -> Self {
        Self::from_env(std::env::vars())
    }

    /// Build from any list of env vars (handy for tests).
    pub fn from_env<I, K, V>(vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        Self { env: vars.into_iter().map(|(k, v)| (k.into(), v.into())).collect(), database: BTreeMap::new() }
    }

    /// Add settings stored in the DB, keyed by env var name.
    #[must_use]
    pub fn with_database<I, K, V>(mut self, values: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.database = values.into_iter().map(|(k, v)| (k.into(), v.into())).collect();
        self
    }

    /// `true` if the env sets this var, so the UI must not change it.
    pub fn is_locked(&self, setting: &Setting) -> bool {
        self.env_value(setting.env).is_some()
            || (setting.secret && self.env_value(&file_var(setting.env)).is_some())
    }

    /// Look up the raw value of a setting. `Ok(None)` = unset everywhere.
    pub fn lookup(&self, setting: &Setting) -> Result<Option<(String, Origin)>, String> {
        let direct = self.env_value(setting.env);
        let file_name = file_var(setting.env);
        let from_file = if setting.secret { self.env_value(&file_name) } else { None };

        match (direct, from_file) {
            (Some(_), Some(_)) => Err(format!("set either {} or {file_name}, not both", setting.env)),
            (Some(value), None) => Ok(Some((value.to_owned(), Origin::Env))),
            (None, Some(path)) => {
                let content =
                    fs::read_to_string(path).map_err(|e| format!("can't read {file_name} ({path}): {e}"))?;
                Ok(Some((content.trim_end_matches(['\n', '\r']).to_owned(), Origin::EnvFile)))
            }
            (None, None) => Ok(self
                .database
                .get(setting.env)
                .map(|v| (v.clone(), Origin::Database))
                .or_else(|| setting.default.map(|d| (d.to_owned(), Origin::Default)))),
        }
    }

    /// `LANEWAY_*` env vars that don't match any setting, so typos get noticed.
    pub fn unknown_vars<'a>(&'a self, settings: &[Setting]) -> Vec<&'a str> {
        self.env
            .keys()
            .filter(|k| k.starts_with("LANEWAY_") && !k.starts_with("LANEWAY_DEV_"))
            .filter(|k| {
                !settings.iter().any(|s| s.env == k.as_str() || (s.secret && file_var(s.env) == k.as_str()))
            })
            .map(String::as_str)
            .collect()
    }

    /// Empty env values count as unset, so `FOO=` in a compose file doesn't break parsing.
    fn env_value(&self, name: &str) -> Option<&str> {
        self.env.get(name).map(String::as_str).filter(|v| !v.is_empty())
    }
}

pub(crate) fn file_var(env: &str) -> String {
    format!("{env}_FILE")
}
