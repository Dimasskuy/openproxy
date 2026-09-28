//! Cryptographic operations for encrypting and decrypting backup bundles.
//!
//! Uses AES-256-GCM with PBKDF2/SHA-256 key derivation.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use openproxy_types::backup::{BACKUP_FORMAT_VERSION, BackupBundle, BackupPayload};
use openproxy_types::{CoreError, Result};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

pub const DEFAULT_KDF_ITERATIONS: u32 = 100_000;
pub const KDF_ALGO: &str = "sha256_iter";
const NONCE_LEN: usize = 12;
const SALT_LEN: usize = 16;
const KEY_LEN: usize = 32;

/// Derive a 256-bit AES key from a passphrase and salt using iterated SHA-256 rounds.
pub fn derive_key(passphrase: &str, salt: &[u8], iterations: u32) -> [u8; KEY_LEN] {
    let mut hasher = Sha256::new();
    hasher.update(salt);
    hasher.update(passphrase.as_bytes());
    let mut key: [u8; 32] = hasher.finalize().into();

    let rounds = iterations.max(1);
    for _ in 1..rounds {
        let mut round_hasher = Sha256::new();
        round_hasher.update(key);
        round_hasher.update(passphrase.as_bytes());
        key = round_hasher.finalize().into();
    }

    key
}

/// Encrypt a [`BackupPayload`] into an encrypted [`BackupBundle`] using `passphrase`.
pub fn encrypt_bundle_payload(payload: &BackupPayload, passphrase: &str) -> Result<BackupBundle> {
    if passphrase.trim().is_empty() {
        return Err(CoreError::Validation("Passphrase cannot be empty".into()));
    }

    let salt: [u8; SALT_LEN] = rand::random();
    let nonce_bytes: [u8; NONCE_LEN] = rand::random();

    let mut key = derive_key(passphrase, &salt, DEFAULT_KDF_ITERATIONS);
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| CoreError::Internal(format!("cipher init failed: {e}")))?;
    key.zeroize();

    let nonce = Nonce::try_from(nonce_bytes.as_slice())
        .map_err(|e| CoreError::Internal(format!("invalid nonce: {e}")))?;
    let json_bytes = serde_json::to_vec(payload)
        .map_err(|e| CoreError::Parse(format!("serialize payload: {e}")))?;

    let ciphertext = cipher
        .encrypt(&nonce, json_bytes.as_slice())
        .map_err(|e| CoreError::Internal(format!("backup encryption failed: {e}")))?;

    let now_utc = chrono::Utc::now().to_rfc3339();

    Ok(BackupBundle {
        version: BACKUP_FORMAT_VERSION,
        exported_at: now_utc,
        openproxy_version: env!("CARGO_PKG_VERSION").to_string(),
        encrypted: true,
        kdf: Some(KDF_ALGO.to_string()),
        kdf_salt: Some(BASE64.encode(salt)),
        kdf_iterations: Some(DEFAULT_KDF_ITERATIONS),
        nonce: Some(BASE64.encode(nonce_bytes)),
        ciphertext: Some(BASE64.encode(ciphertext)),
        payload: None,
    })
}

/// Decrypt an encrypted [`BackupBundle`] using `passphrase` or extract the unencrypted payload.
pub fn decrypt_bundle_payload(
    bundle: &BackupBundle,
    passphrase: Option<&str>,
) -> Result<BackupPayload> {
    if !bundle.encrypted {
        return bundle
            .payload
            .clone()
            .ok_or_else(|| CoreError::Validation("Backup payload is missing".into()));
    }

    let Some(passphrase) = passphrase else {
        return Err(CoreError::Validation(
            "Backup bundle is encrypted: passphrase is required".into(),
        ));
    };

    if passphrase.trim().is_empty() {
        return Err(CoreError::Validation(
            "Backup bundle is encrypted: passphrase is required".into(),
        ));
    }

    let Some(salt_b64) = &bundle.kdf_salt else {
        return Err(CoreError::Validation(
            "Missing kdf_salt in encrypted backup".into(),
        ));
    };
    let Some(nonce_b64) = &bundle.nonce else {
        return Err(CoreError::Validation(
            "Missing nonce in encrypted backup".into(),
        ));
    };
    let Some(ciphertext_b64) = &bundle.ciphertext else {
        return Err(CoreError::Validation(
            "Missing ciphertext in encrypted backup".into(),
        ));
    };

    let salt = BASE64
        .decode(salt_b64.trim())
        .map_err(|e| CoreError::Validation(format!("invalid salt base64: {e}")))?;
    let nonce_bytes = BASE64
        .decode(nonce_b64.trim())
        .map_err(|e| CoreError::Validation(format!("invalid nonce base64: {e}")))?;
    let ciphertext = BASE64
        .decode(ciphertext_b64.trim())
        .map_err(|e| CoreError::Validation(format!("invalid ciphertext base64: {e}")))?;

    if nonce_bytes.len() != NONCE_LEN {
        return Err(CoreError::Validation(format!(
            "invalid nonce length: expected {NONCE_LEN}, got {}",
            nonce_bytes.len()
        )));
    }

    let iterations = bundle.kdf_iterations.unwrap_or(DEFAULT_KDF_ITERATIONS);

    let mut key = derive_key(passphrase, &salt, iterations);
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| CoreError::Internal(format!("cipher init failed: {e}")))?;
    key.zeroize();

    let nonce = Nonce::try_from(nonce_bytes.as_slice())
        .map_err(|e| CoreError::Internal(format!("invalid nonce: {e}")))?;
    let decrypted_bytes = cipher.decrypt(&nonce, ciphertext.as_slice()).map_err(|_| {
        CoreError::Validation(
            "Decryption failed: incorrect passphrase or corrupted backup file".into(),
        )
    })?;

    let payload: BackupPayload = serde_json::from_slice(&decrypted_bytes)
        .map_err(|e| CoreError::Parse(format!("deserialize payload: {e}")))?;

    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let payload = BackupPayload {
            providers: vec![],
            accounts: vec![],
            models: vec![],
            combos: vec![],
            combo_targets: vec![],
            proxy_sources: vec![],
            api_keys: vec![],
            app_config: vec![],
        };

        let bundle = encrypt_bundle_payload(&payload, "super-secret-key").unwrap();
        assert!(bundle.encrypted);
        assert!(bundle.ciphertext.is_some());
        assert!(bundle.payload.is_none());

        // Decrypt with correct passphrase
        let decrypted = decrypt_bundle_payload(&bundle, Some("super-secret-key")).unwrap();
        assert_eq!(decrypted, payload);

        // Decrypt with wrong passphrase fails cleanly
        let wrong_err = decrypt_bundle_payload(&bundle, Some("wrong-key"));
        assert!(wrong_err.is_err());

        // Decrypt without passphrase fails cleanly
        let no_pass_err = decrypt_bundle_payload(&bundle, None);
        assert!(no_pass_err.is_err());
    }
}
