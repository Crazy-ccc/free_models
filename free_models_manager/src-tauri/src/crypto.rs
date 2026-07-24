use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};
use std::fs;
use base64::{Engine as _, engine::general_purpose::{STANDARD, STANDARD_NO_PAD}};

#[derive(Clone)]
pub struct KeyPair {
    pub signing_key: SigningKey,
    pub fingerprint: String,
}

impl KeyPair {

    pub fn from_ssh_files(priv_key_path: &str, pub_key_path: &str) -> Result<Self, String> {
        let priv_content = fs::read_to_string(priv_key_path).map_err(|e| e.to_string())?;
        let pub_content = fs::read_to_string(pub_key_path).map_err(|e| e.to_string())?;

        let seed = parse_openssh_private_key(&priv_content)?;
        let pub_key_bytes = parse_openssh_public_key(&pub_content)?;

        let signing_key = SigningKey::from_bytes(&seed);
        let expected_verifying_key = VerifyingKey::from_bytes(&pub_key_bytes)
            .map_err(|e| e.to_string())?;
        let verifying_key = signing_key.verifying_key();

        if verifying_key.to_bytes() != expected_verifying_key.to_bytes() {
            return Err("Public key does not match private key".to_string());
        }

        let fingerprint = compute_fingerprint(verifying_key);
        Ok(KeyPair { signing_key, fingerprint })
    }

    pub fn sign(&self, message: &str) -> String {
        let signature = self.signing_key.sign(message.as_bytes());
        STANDARD.encode(signature.to_bytes())
    }
}

fn read_wire_string(data: &[u8], pos: &mut usize) -> Result<Vec<u8>, String> {
    if *pos + 4 > data.len() {
        return Err("Unexpected end of data reading wire string length".to_string());
    }
    let len = u32::from_be_bytes(data[*pos..*pos + 4].try_into().unwrap()) as usize;
    *pos += 4;
    if *pos + len > data.len() {
        return Err("Unexpected end of data reading wire string".to_string());
    }
    let result = data[*pos..*pos + len].to_vec();
    *pos += len;
    Ok(result)
}

fn parse_openssh_private_key(content: &str) -> Result<[u8; 32], String> {
    let lines: Vec<&str> = content.lines().collect();
    let base64_data: String = lines
        .iter()
        .filter(|l| !l.starts_with("-----"))
        .map(|l| l.trim())
        .collect();

    let binary = STANDARD.decode(&base64_data).map_err(|e| e.to_string())?;

    if binary.len() < 15 || &binary[..15] != b"openssh-key-v1\x00" {
        return Err("Invalid OpenSSH private key magic".to_string());
    }

    let mut pos = 15usize;

    let cipher_name = read_wire_string(&binary, &mut pos)?;
    if cipher_name != b"none" {
        return Err(format!(
            "Unsupported cipher: {:?}. Only 'none' is supported",
            cipher_name
        ));
    }

    let kdf_name = read_wire_string(&binary, &mut pos)?;
    if kdf_name != b"none" {
        return Err(format!(
            "Unsupported kdf: {:?}. Only 'none' is supported",
            kdf_name
        ));
    }

    let _kdf_options = read_wire_string(&binary, &mut pos)?;

    // number_of_keys (uint32, should be 1)
    if pos + 4 > binary.len() {
        return Err("Unexpected end of data reading number_of_keys".to_string());
    }
    let number_of_keys = u32::from_be_bytes(binary[pos..pos + 4].try_into().unwrap());
    pos += 4;
    if number_of_keys != 1 {
        return Err(format!("Expected 1 key, got {}", number_of_keys));
    }

    let _public_key_blob = read_wire_string(&binary, &mut pos)?;

    let private_section = read_wire_string(&binary, &mut pos)?;

    // Decrypted private section format (cipher=none):
    //   4 bytes: checkint1
    //   4 bytes: checkint2
    //   wire string: key type ("ssh-ed25519")
    //   wire string: public key (32 bytes)
    //   wire string: private key (64 bytes = seed[32] + pubkey[32])
    //   wire string: comment
    //   padding
    if private_section.len() < 4 + 4 {
        return Err("Private key section too short".to_string());
    }

    let checkint1 = u32::from_be_bytes(private_section[0..4].try_into().unwrap());
    let checkint2 = u32::from_be_bytes(private_section[4..8].try_into().unwrap());

    if checkint1 != checkint2 {
        return Err("Private key checkint mismatch".to_string());
    }

    let mut priv_pos = 8usize;

    let key_type = read_wire_string(&private_section, &mut priv_pos)?;
    if key_type != b"ssh-ed25519" {
        return Err(format!(
            "Unsupported key type in private section: {:?}",
            key_type
        ));
    }

    let _pub_from_priv = read_wire_string(&private_section, &mut priv_pos)?;

    let priv_key_data = read_wire_string(&private_section, &mut priv_pos)?;
    if priv_key_data.len() < 32 {
        return Err("Private key data too short for Ed25519 seed".to_string());
    }

    let seed: [u8; 32] = priv_key_data[..32]
        .try_into()
        .map_err(|_| "Failed to extract Ed25519 seed".to_string())?;

    Ok(seed)
}

fn parse_openssh_public_key(content: &str) -> Result<[u8; 32], String> {
    let parts: Vec<&str> = content.split_whitespace().collect();
    if parts.len() < 2 {
        return Err("Invalid public key format".to_string());
    }

    let key_data = STANDARD.decode(parts[1]).map_err(|e| e.to_string())?;

    // Wire format: 4 bytes type length + type string + 4 bytes key data length + key data
    if key_data.len() < 4 {
        return Err("Public key data too short".to_string());
    }

    let mut pos = 0usize;

    let type_len = u32::from_be_bytes(key_data[pos..pos + 4].try_into().unwrap()) as usize;
    pos += 4;
    if pos + type_len > key_data.len() {
        return Err("Unexpected end of public key data".to_string());
    }
    let key_type = &key_data[pos..pos + type_len];
    if key_type != b"ssh-ed25519" {
        return Err(format!(
            "Unsupported key type: {:?}. Only ssh-ed25519 is supported",
            key_type
        ));
    }
    pos += type_len;

    if pos + 4 > key_data.len() {
        return Err("Unexpected end of public key data".to_string());
    }
    let key_len = u32::from_be_bytes(key_data[pos..pos + 4].try_into().unwrap()) as usize;
    pos += 4;

    if pos + key_len > key_data.len() {
        return Err("Unexpected end of public key data".to_string());
    }

    if key_len != 32 {
        return Err(format!(
            "Expected 32-byte Ed25519 public key, got {} bytes",
            key_len
        ));
    }

    let pub_key: [u8; 32] = key_data[pos..pos + 32]
        .try_into()
        .map_err(|_| "Failed to extract Ed25519 public key".to_string())?;
    Ok(pub_key)
}

fn compute_fingerprint(verifying_key: VerifyingKey) -> String {
    let hash = Sha256::digest(verifying_key.to_bytes());
    format!("SHA256:{}", STANDARD_NO_PAD.encode(hash))
}

