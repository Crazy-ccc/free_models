use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::Rng;

/// 加密明文，返回 base64 编码的 (nonce + ciphertext)
pub fn encrypt(plaintext: &str, key: &[u8; 32]) -> Result<String, String> {
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce_bytes = [0u8; 12];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("Encryption failed: {}", e))?;
    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);
    Ok(BASE64.encode(&combined))
}

/// 解密 base64 编码的 (nonce + ciphertext)，返回明文
pub fn decrypt(encoded: &str, key: &[u8; 32]) -> Result<String, String> {
    let combined = BASE64
        .decode(encoded)
        .map_err(|e| format!("Base64 decode failed: {}", e))?;
    if combined.len() < 12 {
        return Err("Invalid ciphertext length".to_string());
    }
    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let cipher = Aes256Gcm::new(key.into());
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("Decryption failed: {}", e))?;
    String::from_utf8(plaintext).map_err(|e| format!("UTF-8 decode failed: {}", e))
}

/// 从 hex 编码的字符串解析 32 字节密钥
pub fn parse_key(hex_key: &str) -> Result<[u8; 32], String> {
    let bytes = hex::decode(hex_key).map_err(|e| format!("Invalid hex key: {}", e))?;
    if bytes.len() != 32 {
        return Err(format!("Key must be 32 bytes (64 hex chars), got {} bytes", bytes.len()));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_preserves_plaintext() {
        let key = [42u8; 32];
        let enc = encrypt("secret-credential", &key).unwrap();
        assert_eq!(decrypt(&enc, &key).unwrap(), "secret-credential");
    }

    #[test]
    fn wrong_key_fails() {
        let enc = encrypt("data", &[1u8; 32]).unwrap();
        assert!(decrypt(&enc, &[2u8; 32]).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let key = [9u8; 32];
        let enc = encrypt("payload", &key).unwrap();
        let mut raw = BASE64.decode(&enc).unwrap();
        let last = raw.len() - 1;
        raw[last] ^= 0xff;
        let bad = BASE64.encode(&raw);
        assert!(decrypt(&bad, &key).is_err());
    }

    #[test]
    fn decrypt_rejects_invalid_base64() {
        assert!(decrypt("!!!not-base64!!!", &[1u8; 32]).is_err());
    }

    #[test]
    fn decrypt_rejects_short_input() {
        let short = BASE64.encode([0u8; 8]);
        assert!(decrypt(&short, &[1u8; 32]).is_err());
    }

    #[test]
    fn parse_key_accepts_repeated_hex() {
        let s = "aa".repeat(32);
        assert_eq!(parse_key(&s).unwrap(), [0xaa; 32]);
    }

    #[test]
    fn parse_key_rejects_wrong_length() {
        assert!(parse_key("aabb").is_err());
    }

    #[test]
    fn parse_key_rejects_invalid_hex() {
        assert!(parse_key(&"zz".repeat(32)).is_err());
    }
}
