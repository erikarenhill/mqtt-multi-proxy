//! Password encryption utilities for secure storage
//!
//! Uses AES-256-GCM encryption with a key derived from the MQTT_PROXY_SECRET environment variable.
//! Encrypted passwords are prefixed with "ENC:" and base64 encoded.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::env;
use tracing::warn;

const ENCRYPTED_PREFIX: &str = "ENC:";
const NONCE_SIZE: usize = 12; // 96 bits for AES-GCM
const ENV_SECRET_KEY: &str = "MQTT_PROXY_SECRET";

/// Derives a 256-bit key from the secret using SHA-256
fn derive_key(secret: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    hasher.update(b"mqtt-proxy-password-encryption"); // Salt
    hasher.finalize().into()
}

/// Gets the encryption key from the environment variable
fn get_encryption_key() -> Option<[u8; 32]> {
    env::var(ENV_SECRET_KEY).ok().map(|s| derive_key(&s))
}

/// Encrypts a password using AES-256-GCM
///
/// Returns the encrypted password prefixed with "ENC:" or the original password
/// if encryption is not configured (no MQTT_PROXY_SECRET env var).
pub fn encrypt_password(password: &str) -> String {
    // Don't encrypt empty passwords
    if password.is_empty() {
        return password.to_string();
    }

    // Already encrypted
    if password.starts_with(ENCRYPTED_PREFIX) {
        return password.to_string();
    }

    let Some(key) = get_encryption_key() else {
        // No encryption key configured, return plaintext
        // This is logged once at startup, not on every call
        return password.to_string();
    };

    let cipher = Aes256Gcm::new_from_slice(&key).expect("Invalid key length");

    // Generate random nonce
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Encrypt
    match cipher.encrypt(nonce, password.as_bytes()) {
        Ok(ciphertext) => {
            // Combine nonce + ciphertext and base64 encode
            let mut combined = nonce_bytes.to_vec();
            combined.extend(ciphertext);
            format!("{}{}", ENCRYPTED_PREFIX, BASE64.encode(combined))
        }
        Err(e) => {
            warn!("Failed to encrypt password: {}", e);
            password.to_string()
        }
    }
}

/// Decrypts a password that was encrypted with encrypt_password
///
/// If the password doesn't start with "ENC:", it's returned as-is (plaintext).
/// If decryption fails, returns None.
pub fn decrypt_password(encrypted: &str) -> Option<String> {
    // Empty password
    if encrypted.is_empty() {
        return Some(encrypted.to_string());
    }

    // Not encrypted, return as-is
    if !encrypted.starts_with(ENCRYPTED_PREFIX) {
        return Some(encrypted.to_string());
    }

    let Some(key) = get_encryption_key() else {
        warn!(
            "Cannot decrypt password: {} environment variable not set",
            ENV_SECRET_KEY
        );
        return None;
    };

    // Remove prefix and decode base64
    let encoded = &encrypted[ENCRYPTED_PREFIX.len()..];
    let combined = match BASE64.decode(encoded) {
        Ok(data) => data,
        Err(e) => {
            warn!("Failed to decode encrypted password: {}", e);
            return None;
        }
    };

    // Split nonce and ciphertext
    if combined.len() < NONCE_SIZE {
        warn!("Encrypted password too short");
        return None;
    }

    let (nonce_bytes, ciphertext) = combined.split_at(NONCE_SIZE);
    let nonce = Nonce::from_slice(nonce_bytes);

    let cipher = Aes256Gcm::new_from_slice(&key).expect("Invalid key length");

    match cipher.decrypt(nonce, ciphertext) {
        Ok(plaintext) => String::from_utf8(plaintext).ok(),
        Err(e) => {
            warn!("Failed to decrypt password: {}", e);
            None
        }
    }
}

/// Checks if password encryption is configured (MQTT_PROXY_SECRET is set)
pub fn is_encryption_configured() -> bool {
    env::var(ENV_SECRET_KEY).is_ok()
}

/// Logs a warning if encryption is not configured
pub fn warn_if_encryption_not_configured() {
    if !is_encryption_configured() {
        warn!(
            "Password encryption not configured. Set {} environment variable to enable.",
            ENV_SECRET_KEY
        );
        warn!("Passwords will be stored in plaintext.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_test_secret<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        env::set_var(ENV_SECRET_KEY, "test-secret-key-12345");
        let result = f();
        env::remove_var(ENV_SECRET_KEY);
        result
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        with_test_secret(|| {
            let password = "my-secret-password";
            let encrypted = encrypt_password(password);

            assert!(encrypted.starts_with(ENCRYPTED_PREFIX));
            assert_ne!(encrypted, password);

            let decrypted = decrypt_password(&encrypted).unwrap();
            assert_eq!(decrypted, password);
        });
    }

    #[test]
    fn test_empty_password() {
        with_test_secret(|| {
            let encrypted = encrypt_password("");
            assert_eq!(encrypted, "");

            let decrypted = decrypt_password("").unwrap();
            assert_eq!(decrypted, "");
        });
    }

    #[test]
    fn test_plaintext_passthrough() {
        with_test_secret(|| {
            let plaintext = "not-encrypted";
            let result = decrypt_password(plaintext).unwrap();
            assert_eq!(result, plaintext);
        });
    }

    #[test]
    fn test_already_encrypted() {
        with_test_secret(|| {
            let password = "test";
            let encrypted = encrypt_password(password);
            let double_encrypted = encrypt_password(&encrypted);

            // Should not double-encrypt
            assert_eq!(encrypted, double_encrypted);
        });
    }

    #[test]
    fn test_no_secret_configured() {
        env::remove_var(ENV_SECRET_KEY);

        let password = "plaintext-password";
        let result = encrypt_password(password);

        // Without secret, password should remain plaintext
        assert_eq!(result, password);
    }
}
