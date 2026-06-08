use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use sha2::{Digest, Sha256};

const TAG_LEN: usize = 16;
const NONCE_LEN: usize = 12;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CryptoError {
    #[error("invalid hex input")]
    InvalidHex,
    #[error("ciphertext failed authentication")]
    AuthenticationFailed,
    #[error("encrypted packet is too short")]
    PacketTooShort,
}

pub fn random_hex(bytes: usize) -> String {
    let mut data = vec![0_u8; bytes];
    for byte in &mut data {
        *byte = rand::random();
    }
    hex_encode(&data)
}

pub fn derive_key(parts: &[&[u8]]) -> [u8; 32] {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part);
    }
    hash.finalize().into()
}

pub fn encrypt_to_hex(key: &[u8; 32], aad: &[u8], plaintext: &[u8]) -> String {
    let nonce = random_hex(NONCE_LEN);
    let nonce_bytes = hex_decode(&nonce).expect("generated nonce is valid hex");
    let ciphertext = aes_encrypt(key, &nonce_bytes, aad, plaintext);
    let mut envelope = nonce_bytes;
    envelope.extend(ciphertext);
    hex_encode(&envelope)
}

pub fn decrypt_from_hex(
    key: &[u8; 32],
    aad: &[u8],
    envelope_hex: &str,
) -> Result<Vec<u8>, CryptoError> {
    let envelope = hex_decode(envelope_hex)?;
    decrypt_envelope(key, aad, &envelope)
}

pub fn encrypt_packet(key: &[u8; 32], counter: u64, aad: &[u8], plaintext: &[u8]) -> Vec<u8> {
    let nonce = packet_nonce(counter);
    let ciphertext = aes_encrypt(key, &nonce, aad, plaintext);
    let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    out.extend(nonce);
    out.extend(ciphertext);
    out
}

pub fn decrypt_packet(key: &[u8; 32], aad: &[u8], packet: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if packet.len() < NONCE_LEN + TAG_LEN {
        return Err(CryptoError::PacketTooShort);
    }
    decrypt_envelope(key, aad, packet)
}

fn decrypt_envelope(key: &[u8; 32], aad: &[u8], envelope: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if envelope.len() < NONCE_LEN + TAG_LEN {
        return Err(CryptoError::PacketTooShort);
    }
    let nonce = &envelope[..NONCE_LEN];
    let ciphertext = &envelope[NONCE_LEN..];
    aes_decrypt(key, nonce, aad, ciphertext)
}

fn aes_encrypt(key: &[u8; 32], nonce: &[u8], aad: &[u8], plaintext: &[u8]) -> Vec<u8> {
    let cipher = Aes256Gcm::new_from_slice(key).expect("AES-256-GCM key length is fixed");
    cipher
        .encrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .expect("AES-GCM encryption should not fail for fixed-size nonce")
}

fn aes_decrypt(
    key: &[u8; 32],
    nonce: &[u8],
    aad: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = Aes256Gcm::new_from_slice(key).expect("AES-256-GCM key length is fixed");
    cipher
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| CryptoError::AuthenticationFailed)
}

fn packet_nonce(counter: u64) -> [u8; NONCE_LEN] {
    let mut nonce = [0_u8; NONCE_LEN];
    nonce[4..].copy_from_slice(&counter.to_be_bytes());
    nonce
}

pub fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

pub fn hex_decode(input: &str) -> Result<Vec<u8>, CryptoError> {
    let input = input.trim();
    if input.len() % 2 != 0 {
        return Err(CryptoError::InvalidHex);
    }
    let mut out = Vec::with_capacity(input.len() / 2);
    for pair in input.as_bytes().chunks(2) {
        out.push((hex_value(pair[0])? << 4) | hex_value(pair[1])?);
    }
    Ok(out)
}

fn hex_value(byte: u8) -> Result<u8, CryptoError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(CryptoError::InvalidHex),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_round_trips_and_detects_tampering() {
        let key = derive_key(&[b"pin", b"phone", b"receiver"]);
        let encrypted = encrypt_to_hex(&key, b"pairing", b"secret token");
        assert_eq!(
            decrypt_from_hex(&key, b"pairing", &encrypted).unwrap(),
            b"secret token"
        );

        let mut tampered = encrypted;
        tampered.replace_range(24..26, "00");
        assert_eq!(
            decrypt_from_hex(&key, b"pairing", &tampered),
            Err(CryptoError::AuthenticationFailed)
        );
    }

    #[test]
    fn packet_round_trips() {
        let key = derive_key(&[b"media"]);
        let packet = encrypt_packet(&key, 7, b"video", b"rtp");
        assert_eq!(decrypt_packet(&key, b"video", &packet).unwrap(), b"rtp");
    }

    #[test]
    fn aes_256_gcm_empty_plaintext_vector_matches_standard() {
        let key = [0_u8; 32];
        let envelope = "000000000000000000000000530f8afbc74536b9a963b4f1c4cb738b";
        assert_eq!(decrypt_from_hex(&key, b"", envelope).unwrap(), b"");
    }
}
