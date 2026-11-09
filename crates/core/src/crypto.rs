//! Encryption of secrets at rest (e.g. secret settings in the `settings` table).
//!
//! The key is derived from `LANEWAY_SECRET_KEY` with HKDF-SHA256 and a fixed info string per
//! purpose ([`Purpose`]), so one master key gives independent keys for different uses. Values
//! are encrypted with AES-256-GCM and a random 96-bit nonce, and stored as text:
//!
//! ```text
//! enc:v1:<nonce, base64>:<ciphertext + tag, base64>
//! ```
//!
//! The associated data passed to [`Cipher::encrypt`] (e.g. the setting key) is authenticated
//! but not stored, so a value copied to another key fails to decrypt.

use std::fmt;

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use hkdf::Hkdf;
use sha2::Sha256;

/// Prefix of every encrypted value, including the format version.
pub const PREFIX: &str = "enc:v1:";

/// What a derived key is used for. Each purpose has its own fixed HKDF info string. Changing
/// one makes everything encrypted with it unreadable, so add a new variant instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// Secret values in the `settings` table.
    Settings,
}

impl Purpose {
    /// HKDF info string.
    pub fn info(self) -> &'static str {
        match self {
            Purpose::Settings => "laneway settings v1",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CryptoError {
    #[error("not an encrypted value (expected `{PREFIX}<nonce>:<ciphertext>`)")]
    Format,
    #[error("can't decrypt the value, LANEWAY_SECRET_KEY probably changed since it was saved")]
    Decrypt,
}

/// Encrypts and decrypts values for one [`Purpose`].
#[derive(Clone)]
pub struct Cipher {
    aead: Aes256Gcm,
}

impl fmt::Debug for Cipher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Cipher(***)")
    }
}

impl Cipher {
    /// Derives the key for `purpose` from the master secret key.
    ///
    /// # Panics
    ///
    /// Never: 32 bytes is a valid HKDF-SHA256 output length.
    pub fn new(secret_key: &str, purpose: Purpose) -> Self {
        let mut key = [0u8; 32];
        Hkdf::<Sha256>::new(None, secret_key.as_bytes())
            .expand(purpose.info().as_bytes(), &mut key)
            .expect("32 bytes is a valid HKDF-SHA256 output length");
        Self { aead: Aes256Gcm::new(&Key::<Aes256Gcm>::from(key)) }
    }

    /// Encrypts `plaintext` with a fresh random nonce. `associated_data` must be passed again
    /// to decrypt.
    ///
    /// # Panics
    ///
    /// Never in practice: AES-GCM only fails for messages over 64 GiB.
    pub fn encrypt(&self, plaintext: &str, associated_data: &[u8]) -> String {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = self
            .aead
            .encrypt(&nonce, Payload { msg: plaintext.as_bytes(), aad: associated_data })
            .expect("AES-GCM encryption of an in-memory buffer can't fail");
        format!("{PREFIX}{}:{}", BASE64.encode(nonce.as_slice()), BASE64.encode(ciphertext))
    }

    /// Decrypts a value made by [`Cipher::encrypt`].
    pub fn decrypt(&self, text: &str, associated_data: &[u8]) -> Result<String, CryptoError> {
        let rest = text.strip_prefix(PREFIX).ok_or(CryptoError::Format)?;
        let (nonce, ciphertext) = rest.split_once(':').ok_or(CryptoError::Format)?;
        let nonce = BASE64.decode(nonce).map_err(|_| CryptoError::Format)?;
        let nonce: [u8; 12] = nonce.try_into().map_err(|_| CryptoError::Format)?;
        let ciphertext = BASE64.decode(ciphertext).map_err(|_| CryptoError::Format)?;
        let plaintext = self
            .aead
            .decrypt(&Nonce::from(nonce), Payload { msg: &ciphertext, aad: associated_data })
            .map_err(|_| CryptoError::Decrypt)?;
        String::from_utf8(plaintext).map_err(|_| CryptoError::Decrypt)
    }
}

/// `true` if the text looks like a value made by [`Cipher::encrypt`].
pub fn is_encrypted(text: &str) -> bool {
    text.starts_with(PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "test-secret-key-not-for-production-0000";

    fn cipher() -> Cipher {
        Cipher::new(KEY, Purpose::Settings)
    }

    #[test]
    fn round_trip() {
        let c = cipher();
        for plaintext in ["", "hunter2", "a password with spaces and: colons"] {
            let encrypted = c.encrypt(plaintext, b"LANEWAY_X");
            assert!(is_encrypted(&encrypted), "{encrypted}");
            assert!(!encrypted.contains(plaintext) || plaintext.is_empty());
            assert_eq!(c.decrypt(&encrypted, b"LANEWAY_X").unwrap(), plaintext);
        }
        // A cipher built again from the same key reads it too (as after a restart).
        let encrypted = c.encrypt("value", b"");
        assert_eq!(cipher().decrypt(&encrypted, b"").unwrap(), "value");
    }

    #[test]
    fn format() {
        let encrypted = cipher().encrypt("value", b"");
        let parts: Vec<&str> = encrypted.split(':').collect();
        assert_eq!(parts[..2], ["enc", "v1"]);
        assert_eq!(BASE64.decode(parts[2]).unwrap().len(), 12, "96-bit nonce");
        // 5 bytes of plaintext + 16 bytes of tag.
        assert_eq!(BASE64.decode(parts[3]).unwrap().len(), 21);
    }

    #[test]
    fn wrong_key_fails() {
        let encrypted = cipher().encrypt("value", b"");
        let other = Cipher::new("another-secret-key-not-for-production-00", Purpose::Settings);
        let err = other.decrypt(&encrypted, b"").unwrap_err();
        assert_eq!(err, CryptoError::Decrypt);
        assert!(err.to_string().contains("LANEWAY_SECRET_KEY probably changed"), "{err}");
    }

    #[test]
    fn wrong_associated_data_fails() {
        let encrypted = cipher().encrypt("value", b"LANEWAY_A");
        assert_eq!(cipher().decrypt(&encrypted, b"LANEWAY_B"), Err(CryptoError::Decrypt));
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let encrypted = cipher().encrypt("value", b"");
        let (head, ciphertext) = encrypted.rsplit_once(':').unwrap();
        let mut bytes = BASE64.decode(ciphertext).unwrap();
        bytes[0] ^= 1;
        let tampered = format!("{head}:{}", BASE64.encode(bytes));
        assert_eq!(cipher().decrypt(&tampered, b""), Err(CryptoError::Decrypt));
    }

    #[test]
    fn nonces_differ_each_time() {
        let c = cipher();
        let a = c.encrypt("value", b"");
        let b = c.encrypt("value", b"");
        assert_ne!(a, b);
        assert_ne!(a.split(':').nth(2), b.split(':').nth(2));
    }

    #[test]
    fn malformed_values_are_format_errors() {
        let c = cipher();
        for bad in ["plain", "enc:v1:", "enc:v1:abc", "enc:v1:!!!:abc", "enc:v1:AAAA:AAAA", "enc:v2:a:b"] {
            assert_eq!(c.decrypt(bad, b""), Err(CryptoError::Format), "{bad}");
        }
    }

    #[test]
    fn debug_hides_the_key() {
        assert_eq!(format!("{:?}", cipher()), "Cipher(***)");
    }
}
