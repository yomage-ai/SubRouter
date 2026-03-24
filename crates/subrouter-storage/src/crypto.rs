use std::sync::Arc;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use rand::RngCore;
use serde::Serialize;
use serde::de::DeserializeOwned;
use thiserror::Error;

const NONCE_SIZE: usize = 12;
const KEY_SIZE: usize = 32;

#[derive(Clone)]
pub struct SecretCipher {
    cipher: Arc<Aes256Gcm>,
}

impl SecretCipher {
    pub fn new(encoded_key: &str) -> Result<Self, CryptoError> {
        let key = decode_key(encoded_key)?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|_| CryptoError::InvalidKeyLength(key.len()))?;
        Ok(Self {
            cipher: Arc::new(cipher),
        })
    }

    pub fn encrypt_bytes(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let mut nonce_bytes = [0_u8; NONCE_SIZE];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let ciphertext = self
            .cipher
            .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
            .map_err(|_| CryptoError::EncryptFailed)?;

        let mut payload = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
        payload.extend_from_slice(&nonce_bytes);
        payload.extend_from_slice(&ciphertext);
        Ok(payload)
    }

    pub fn decrypt_bytes(&self, payload: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if payload.len() < NONCE_SIZE {
            return Err(CryptoError::MalformedCiphertext);
        }

        let (nonce, ciphertext) = payload.split_at(NONCE_SIZE);
        self.cipher
            .decrypt(Nonce::from_slice(nonce), ciphertext)
            .map_err(|_| CryptoError::DecryptFailed)
    }

    pub fn encrypt_string(&self, plaintext: &str) -> Result<Vec<u8>, CryptoError> {
        self.encrypt_bytes(plaintext.as_bytes())
    }

    pub fn decrypt_string(&self, payload: &[u8]) -> Result<String, CryptoError> {
        let bytes = self.decrypt_bytes(payload)?;
        String::from_utf8(bytes).map_err(CryptoError::InvalidUtf8)
    }

    pub fn encrypt_json<T: Serialize>(&self, value: &T) -> Result<Vec<u8>, CryptoError> {
        let json = serde_json::to_vec(value)?;
        self.encrypt_bytes(&json)
    }

    pub fn decrypt_json<T: DeserializeOwned>(&self, payload: &[u8]) -> Result<T, CryptoError> {
        let bytes = self.decrypt_bytes(payload)?;
        Ok(serde_json::from_slice(&bytes)?)
    }
}

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("key must decode to 32 bytes, got {0}")]
    InvalidKeyLength(usize),
    #[error("key must be valid base64 or hex")]
    InvalidKeyEncoding,
    #[error("ciphertext payload is malformed")]
    MalformedCiphertext,
    #[error("encryption failed")]
    EncryptFailed,
    #[error("decryption failed")]
    DecryptFailed,
    #[error(transparent)]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
}

fn decode_key(encoded_key: &str) -> Result<Vec<u8>, CryptoError> {
    let encoded_key = encoded_key.trim();

    let decoded = hex::decode(encoded_key)
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(encoded_key))
        .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(encoded_key))
        .map_err(|_| CryptoError::InvalidKeyEncoding)?;

    if decoded.len() != KEY_SIZE {
        return Err(CryptoError::InvalidKeyLength(decoded.len()));
    }

    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const HEX_KEY: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

    #[test]
    fn cipher_round_trips_text() {
        let cipher = SecretCipher::new(HEX_KEY).unwrap();
        let payload = cipher.encrypt_string("hello").unwrap();

        assert_eq!(cipher.decrypt_string(&payload).unwrap(), "hello");
    }

    #[test]
    fn cipher_round_trips_json() {
        let cipher = SecretCipher::new(HEX_KEY).unwrap();
        let payload = cipher.encrypt_json(&json!({"hello": "world"})).unwrap();
        let value: serde_json::Value = cipher.decrypt_json(&payload).unwrap();

        assert_eq!(value["hello"], "world");
    }
}
