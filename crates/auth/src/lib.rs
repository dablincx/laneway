//! laneway: authentication primitives. Password hashing and random tokens.
//!
//! Tokens (sessions, setup token, later API keys) are 256 random bits, handed out as
//! URL-safe base64 and stored only as a SHA-256 hash. Looking a token up by its hash means a
//! leaked database doesn't leak working tokens, and there's no secret compare to get wrong.

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

/// Shortest password accepted.
pub const MIN_PASSWORD_LENGTH: usize = 10;
/// Longest password accepted (argon2 is slow on purpose, so cap the input).
pub const MAX_PASSWORD_LENGTH: usize = 512;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("password hashing failed: {0}")]
    Hash(String),
    #[error("no randomness available: {0}")]
    Random(String),
}

/// Why a new password was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordProblem {
    TooShort,
    TooLong,
}

/// Checks a new password. Length only: long passphrases beat composition rules.
pub fn check_new_password(password: &str) -> Result<(), PasswordProblem> {
    let len = password.chars().count();
    if len < MIN_PASSWORD_LENGTH {
        Err(PasswordProblem::TooShort)
    } else if len > MAX_PASSWORD_LENGTH {
        Err(PasswordProblem::TooLong)
    } else {
        Ok(())
    }
}

/// argon2id PHC string with a random salt.
pub fn hash_password(password: &str) -> Result<String, Error> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|h| h.to_string())
        .map_err(|e| Error::Hash(e.to_string()))
}

/// `true` if the password matches the stored hash. A malformed hash counts as no match.
pub fn verify_password(password: &str, hash: &str) -> bool {
    if password.len() > MAX_PASSWORD_LENGTH * 4 {
        return false;
    }
    PasswordHash::new(hash)
        .is_ok_and(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
}

/// Burns about as much time as a real check, so "unknown user" and "wrong password" take the
/// same time and can't be told apart.
pub fn dummy_verify(password: &str) {
    static DUMMY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let hash = DUMMY.get_or_init(|| hash_password("laneway-dummy-password").unwrap_or_default());
    let _ = verify_password(password, hash);
}

/// A new random token and the hash to store for it.
#[derive(Debug, Clone)]
pub struct NewToken {
    /// Give this to the client. Never store it.
    pub token: String,
    pub hash: Vec<u8>,
}

/// Generates a 256-bit random token.
pub fn new_token() -> Result<NewToken, Error> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| Error::Random(e.to_string()))?;
    let token = URL_SAFE_NO_PAD.encode(bytes);
    let hash = hash_token(&token);
    Ok(NewToken { token, hash })
}

/// SHA-256 of a token, as stored in the database.
pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_round_trip() {
        let hash = hash_password("correct horse battery").unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_password("correct horse battery", &hash));
        assert!(!verify_password("wrong horse battery", &hash));
        assert!(!verify_password("correct horse battery", "not a hash"));
    }

    #[test]
    fn salts_differ() {
        assert_ne!(hash_password("same password").unwrap(), hash_password("same password").unwrap());
    }

    #[test]
    fn password_rules() {
        assert_eq!(check_new_password("short"), Err(PasswordProblem::TooShort));
        assert_eq!(check_new_password(&"x".repeat(513)), Err(PasswordProblem::TooLong));
        assert_eq!(check_new_password("long enough pass"), Ok(()));
    }

    #[test]
    fn tokens_are_random_and_hashed() {
        let a = new_token().unwrap();
        let b = new_token().unwrap();
        assert_ne!(a.token, b.token);
        assert_eq!(a.token.len(), 43);
        assert_eq!(hash_token(&a.token), a.hash);
        assert_eq!(a.hash.len(), 32);
    }
}
