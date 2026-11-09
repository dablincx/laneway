//! Value types a setting can have, and how each one is parsed from text.

use std::fmt;
use std::path::PathBuf;

/// What kind of value a setting holds. Used for the docs and error messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    String,
    Secret { min_length: usize },
    Url,
    PostgresUrl,
    Integer { min: i64, max: i64 },
    Boolean,
    Enum(&'static [&'static str]),
    Path,
    Locale,
}

impl Kind {
    /// Short human description, e.g. "integer (1-65535)".
    pub fn describe(&self) -> String {
        match self {
            Kind::String => "string".into(),
            Kind::Secret { min_length } => format!("string, at least {min_length} characters"),
            Kind::Url => "URL".into(),
            Kind::PostgresUrl => "Postgres URL".into(),
            Kind::Integer { min, max } => format!("integer ({min}-{max})"),
            Kind::Boolean => "boolean (`true` or `false`)".into(),
            Kind::Enum(values) => {
                format!("one of {}", values.iter().map(|v| format!("`{v}`")).collect::<Vec<_>>().join(", "))
            }
            Kind::Path => "path".into(),
            Kind::Locale => "locale code (e.g. `en`, `de`, `pt-BR`)".into(),
        }
    }
}

/// A Rust type that can be read from a setting.
pub trait SettingValue: Sized {
    /// `true` for `Option<T>`: the setting may stay unset.
    const OPTIONAL: bool = false;

    /// Parse the raw text. Returns a message describing what was expected on failure.
    fn parse(raw: &str, kind: Kind) -> Result<Self, String>;

    /// Value used when the setting is unset and has no default (only `Option<T>`).
    fn unset() -> Option<Self> {
        None
    }
}

fn expected(kind: Kind, raw: &str) -> String {
    format!("expected {}, got {raw:?}", kind.describe())
}

impl SettingValue for String {
    fn parse(raw: &str, kind: Kind) -> Result<Self, String> {
        match kind {
            Kind::Enum(values) if !values.contains(&raw) => Err(expected(kind, raw)),
            Kind::Locale if !is_locale(raw) => Err(expected(kind, raw)),
            _ => Ok(raw.to_owned()),
        }
    }
}

impl SettingValue for i64 {
    fn parse(raw: &str, kind: Kind) -> Result<Self, String> {
        let value: i64 = raw.trim().parse().map_err(|_| expected(kind, raw))?;
        if let Kind::Integer { min, max } = kind
            && !(min..=max).contains(&value)
        {
            return Err(expected(kind, raw));
        }
        Ok(value)
    }
}

impl SettingValue for u16 {
    fn parse(raw: &str, kind: Kind) -> Result<Self, String> {
        let value = i64::parse(raw, kind)?;
        u16::try_from(value).map_err(|_| expected(kind, raw))
    }
}

impl SettingValue for u32 {
    fn parse(raw: &str, kind: Kind) -> Result<Self, String> {
        let value = i64::parse(raw, kind)?;
        u32::try_from(value).map_err(|_| expected(kind, raw))
    }
}

impl SettingValue for bool {
    fn parse(raw: &str, kind: Kind) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Ok(true),
            "false" | "0" | "no" | "off" => Ok(false),
            _ => Err(expected(kind, raw)),
        }
    }
}

impl SettingValue for PathBuf {
    fn parse(raw: &str, kind: Kind) -> Result<Self, String> {
        if raw.trim().is_empty() {
            return Err(expected(kind, raw));
        }
        Ok(PathBuf::from(raw))
    }
}

impl SettingValue for url::Url {
    fn parse(raw: &str, kind: Kind) -> Result<Self, String> {
        let url = url::Url::parse(raw.trim()).map_err(|_| expected(kind, raw))?;
        let ok = match kind {
            Kind::PostgresUrl => matches!(url.scheme(), "postgres" | "postgresql"),
            _ => matches!(url.scheme(), "http" | "https"),
        };
        if ok { Ok(url) } else { Err(expected(kind, raw)) }
    }
}

impl SettingValue for Secret {
    fn parse(raw: &str, kind: Kind) -> Result<Self, String> {
        if let Kind::Secret { min_length } = kind
            && raw.chars().count() < min_length
        {
            return Err(format!(
                "expected {}, got a value with {} characters",
                kind.describe(),
                raw.chars().count()
            ));
        }
        Ok(Secret(raw.to_owned()))
    }
}

impl<T: SettingValue> SettingValue for Option<T> {
    const OPTIONAL: bool = true;

    fn parse(raw: &str, kind: Kind) -> Result<Self, String> {
        T::parse(raw, kind).map(Some)
    }

    fn unset() -> Option<Self> {
        Some(None)
    }
}

/// Loose BCP 47 check: `en`, `de`, `pt-BR`, `zh-Hans`.
fn is_locale(raw: &str) -> bool {
    let mut parts = raw.split('-');
    let Some(lang) = parts.next() else { return false };
    (2..=3).contains(&lang.len())
        && lang.chars().all(|c| c.is_ascii_lowercase())
        && parts.all(|p| (2..=8).contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// A secret value. Never printed by `Debug` or `Display`.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locales() {
        for ok in ["en", "de", "fr", "pt-BR", "zh-Hans"] {
            assert!(is_locale(ok), "{ok}");
        }
        for bad in ["", "EN", "english", "e", "en_US", "en-"] {
            assert!(!is_locale(bad), "{bad}");
        }
    }

    #[test]
    fn booleans() {
        assert!(bool::parse("TRUE", Kind::Boolean).unwrap());
        assert!(!bool::parse("off", Kind::Boolean).unwrap());
        assert!(bool::parse("maybe", Kind::Boolean).is_err());
    }

    #[test]
    fn integer_range() {
        let kind = Kind::Integer { min: 1, max: 10 };
        assert_eq!(i64::parse("10", kind).unwrap(), 10);
        assert!(i64::parse("11", kind).is_err());
        assert!(i64::parse("x", kind).is_err());
    }

    #[test]
    fn urls() {
        assert!(<url::Url as SettingValue>::parse("https://example.com", Kind::Url).is_ok());
        assert!(<url::Url as SettingValue>::parse("ftp://example.com", Kind::Url).is_err());
        assert!(<url::Url as SettingValue>::parse("postgres://u:p@db/laneway", Kind::PostgresUrl).is_ok());
        assert!(<url::Url as SettingValue>::parse("https://db", Kind::PostgresUrl).is_err());
    }

    #[test]
    fn secret_is_redacted() {
        let s = Secret::parse("x".repeat(32).as_str(), Kind::Secret { min_length: 32 }).unwrap();
        assert_eq!(format!("{s:?} {s}"), "Secret(***) ***");
        assert!(Secret::parse("short", Kind::Secret { min_length: 32 }).is_err());
    }
}
