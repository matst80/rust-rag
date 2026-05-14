//! Symmetric encryption for at-rest secrets (OAuth tokens, etc.).
//!
//! Phase 1: single master key sourced from `OAUTH_TOKEN_ENC_KEY` (32 bytes,
//! base64 or hex). AES-256-GCM, random 12-byte nonce prefixed to ciphertext.
//! Storage format: `nonce(12) || ciphertext || tag`, then base64-url encoded.

use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, KeyInit, OsRng, rand_core::RngCore},
};
use anyhow::{Context, Result, anyhow};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

#[derive(Clone)]
pub struct EncryptionKey {
    cipher: Aes256Gcm,
}

impl std::fmt::Debug for EncryptionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptionKey").finish_non_exhaustive()
    }
}

impl EncryptionKey {
    /// Build a key from an env-style string. Accepts a 32-byte key encoded as
    /// base64 (standard or url-safe, with or without padding) or 64-char hex.
    pub fn from_secret_str(raw: &str) -> Result<Self> {
        let bytes = decode_key_material(raw.trim())
            .with_context(|| "OAUTH_TOKEN_ENC_KEY must be 32 bytes (base64 or hex)")?;
        if bytes.len() != 32 {
            return Err(anyhow!(
                "encryption key must decode to 32 bytes, got {}",
                bytes.len()
            ));
        }
        let key = Key::<Aes256Gcm>::from_slice(&bytes);
        Ok(Self {
            cipher: Aes256Gcm::new(key),
        })
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Result<String> {
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ct = self
            .cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| anyhow!("aes-gcm encrypt failed: {e}"))?;
        let mut out = Vec::with_capacity(12 + ct.len());
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ct);
        Ok(URL_SAFE_NO_PAD.encode(out))
    }

    pub fn decrypt(&self, encoded: &str) -> Result<Vec<u8>> {
        let raw = URL_SAFE_NO_PAD
            .decode(encoded.trim())
            .context("ciphertext is not valid base64-url")?;
        if raw.len() < 12 + 16 {
            return Err(anyhow!("ciphertext too short"));
        }
        let (nonce_bytes, ct) = raw.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        self.cipher
            .decrypt(nonce, ct)
            .map_err(|e| anyhow!("aes-gcm decrypt failed: {e}"))
    }
}

fn decode_key_material(raw: &str) -> Result<Vec<u8>> {
    if raw.len() == 64 && raw.chars().all(|c| c.is_ascii_hexdigit()) {
        let mut bytes = Vec::with_capacity(32);
        for chunk in raw.as_bytes().chunks(2) {
            let s = std::str::from_utf8(chunk)?;
            bytes.push(u8::from_str_radix(s, 16)?);
        }
        return Ok(bytes);
    }
    if let Ok(b) = URL_SAFE_NO_PAD.decode(raw) {
        return Ok(b);
    }
    base64::engine::general_purpose::STANDARD
        .decode(raw)
        .context("not valid base64 or hex")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let key = EncryptionKey::from_secret_str(
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        )
        .unwrap();
        let pt = b"hello google oauth refresh token";
        let ct = key.encrypt(pt).unwrap();
        let back = key.decrypt(&ct).unwrap();
        assert_eq!(back, pt);
    }

    #[test]
    fn rejects_bad_key_length() {
        assert!(EncryptionKey::from_secret_str("AAAA").is_err());
    }

    #[test]
    fn rejects_tampered_ciphertext() {
        let key = EncryptionKey::from_secret_str(
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        )
        .unwrap();
        let mut ct = key.encrypt(b"abc").unwrap();
        ct.push('x');
        assert!(key.decrypt(&ct).is_err());
    }
}
