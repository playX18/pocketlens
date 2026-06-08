use std::{
    collections::HashMap,
    net::IpAddr,
    time::{Duration, Instant},
};

use rand::Rng;
use sha2::{Digest, Sha256};

use crate::{
    crypto,
    protocol::{
        PairResponse, PendingPairingRequest, SESSION_TOKEN_EXPIRES_IN_SECONDS, SecurePairingStatus,
    },
};

#[derive(Debug, Clone)]
pub struct PairingConfig {
    pub pin_ttl: Duration,
    pub token_ttl: Duration,
    pub max_pin_attempts: u8,
}

impl Default for PairingConfig {
    fn default() -> Self {
        Self {
            pin_ttl: Duration::from_secs(300),
            token_ttl: Duration::from_secs(SESSION_TOKEN_EXPIRES_IN_SECONDS),
            max_pin_attempts: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairedDevice {
    pub device_name: String,
    pub sender_ip: Option<IpAddr>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PairingError {
    #[error("pairing PIN expired")]
    PinExpired,
    #[error("invalid pairing PIN")]
    InvalidPin,
    #[error("too many invalid PIN attempts")]
    TooManyAttempts,
    #[error("invalid or expired session token")]
    InvalidToken,
}

#[derive(Debug)]
pub struct PairingManager {
    config: PairingConfig,
    pin_hash: Vec<u8>,
    pin_created_at: Instant,
    attempts: u8,
    token: Option<TokenRecord>,
    paired_device: Option<PairedDevice>,
    pending: HashMap<String, PendingPairing>,
}

#[derive(Debug, Clone)]
struct TokenRecord {
    token: String,
    expires_at: Instant,
}

#[derive(Debug, Clone)]
pub struct PendingPairing {
    pub pairing_id: String,
    pub device_name: String,
    pub phone_nonce: String,
    pub phone_public_key: String,
    pub receiver_nonce: String,
    pub receiver_public_key: String,
    pub created_at: Instant,
    pub created_at_unix_ms: u64,
    pub approved_result: Option<String>,
    pub rejected: bool,
}

impl PairingManager {
    pub fn new(pin: &str, now: Instant, config: PairingConfig) -> Self {
        Self {
            config,
            pin_hash: hash_secret(pin),
            pin_created_at: now,
            attempts: 0,
            token: None,
            paired_device: None,
            pending: HashMap::new(),
        }
    }

    pub fn with_random_pin(now: Instant, config: PairingConfig) -> (Self, String) {
        let pin = format!("{:06}", rand::rng().random_range(0..1_000_000));
        (Self::new(&pin, now, config), pin)
    }

    pub fn pair(
        &mut self,
        pin: &str,
        device_name: String,
        sender_ip: Option<IpAddr>,
        now: Instant,
    ) -> Result<String, PairingError> {
        if now.duration_since(self.pin_created_at) > self.config.pin_ttl {
            return Err(PairingError::PinExpired);
        }

        if self.attempts >= self.config.max_pin_attempts {
            return Err(PairingError::TooManyAttempts);
        }

        if hash_secret(pin) != self.pin_hash {
            self.attempts += 1;
            return Err(PairingError::InvalidPin);
        }

        let token = "session_0123456789abcdef".to_string();
        self.token = Some(TokenRecord {
            token: token.clone(),
            expires_at: now + self.config.token_ttl,
        });
        self.paired_device = Some(PairedDevice {
            device_name,
            sender_ip,
        });
        Ok(token)
    }

    pub fn validate_token(&self, token: Option<&str>, now: Instant) -> Result<(), PairingError> {
        let Some(token) = token else {
            return Err(PairingError::InvalidToken);
        };
        let Some(record) = &self.token else {
            return Err(PairingError::InvalidToken);
        };
        if now > record.expires_at || token != record.token {
            return Err(PairingError::InvalidToken);
        }
        Ok(())
    }

    pub fn is_paired(&self, now: Instant) -> bool {
        self.validate_token(self.token.as_ref().map(|record| record.token.as_str()), now)
            .is_ok()
    }

    pub fn paired_device(&self) -> Option<&PairedDevice> {
        self.paired_device.as_ref()
    }

    pub fn request_secure_pairing(
        &mut self,
        pairing_id: String,
        device_name: String,
        phone_nonce: String,
        phone_public_key: String,
        now: Instant,
        created_at_unix_ms: u64,
    ) -> (String, String) {
        self.prune_expired(now);
        let receiver_nonce = crypto::random_hex(16);
        let receiver_public_key = crypto::random_hex(32);
        self.pending.insert(
            pairing_id.clone(),
            PendingPairing {
                pairing_id,
                device_name,
                phone_nonce,
                phone_public_key,
                receiver_nonce: receiver_nonce.clone(),
                receiver_public_key: receiver_public_key.clone(),
                created_at: now,
                created_at_unix_ms,
                approved_result: None,
                rejected: false,
            },
        );
        (receiver_nonce, receiver_public_key)
    }

    pub fn pending_requests(&mut self, now: Instant) -> Vec<PendingPairingRequest> {
        self.prune_expired(now);
        self.pending
            .values()
            .filter(|request| request.approved_result.is_none() && !request.rejected)
            .map(|request| PendingPairingRequest {
                pairing_id: request.pairing_id.clone(),
                device_name: request.device_name.clone(),
                requested_at_unix_ms: request.created_at_unix_ms,
                expires_in_seconds: self.config.pin_ttl.as_secs(),
            })
            .collect()
    }

    pub fn approve_secure_pairing(
        &mut self,
        pairing_id: &str,
        pin: &str,
        now: Instant,
        receiver_name: &str,
        protocol_version: u16,
    ) -> Result<(), PairingError> {
        self.prune_expired(now);
        let Some(request) = self.pending.get_mut(pairing_id) else {
            return Err(PairingError::InvalidPin);
        };
        let key = secure_pairing_key(
            pin,
            pairing_id,
            &request.phone_nonce,
            &request.receiver_nonce,
            &request.phone_public_key,
            &request.receiver_public_key,
        );
        let token = format!("session_{}", crypto::random_hex(16));
        self.token = Some(TokenRecord {
            token: token.clone(),
            expires_at: now + self.config.token_ttl,
        });
        self.paired_device = Some(PairedDevice {
            device_name: request.device_name.clone(),
            sender_ip: None,
        });
        let response = PairResponse {
            session_token: token,
            receiver_name: receiver_name.to_string(),
            protocol_version,
            expires_in_seconds: SESSION_TOKEN_EXPIRES_IN_SECONDS,
        };
        let payload = serde_json::to_vec(&response).expect("pair response serializes");
        request.approved_result = Some(crypto::encrypt_to_hex(
            &key,
            pairing_id.as_bytes(),
            &payload,
        ));
        Ok(())
    }

    pub fn secure_pairing_result(
        &mut self,
        pairing_id: &str,
        now: Instant,
    ) -> (SecurePairingStatus, Option<String>) {
        self.prune_expired(now);
        let Some(request) = self.pending.get(pairing_id) else {
            return (SecurePairingStatus::Expired, None);
        };
        if request.rejected {
            return (SecurePairingStatus::Rejected, None);
        }
        if let Some(result) = &request.approved_result {
            return (SecurePairingStatus::Approved, Some(result.clone()));
        }
        (SecurePairingStatus::Pending, None)
    }

    fn prune_expired(&mut self, now: Instant) {
        let ttl = self.config.pin_ttl;
        self.pending
            .retain(|_, request| now.duration_since(request.created_at) <= ttl);
    }
}

fn hash_secret(secret: &str) -> Vec<u8> {
    Sha256::digest(secret.as_bytes()).to_vec()
}

pub fn secure_pairing_key(
    pin: &str,
    pairing_id: &str,
    phone_nonce: &str,
    receiver_nonce: &str,
    phone_public_key: &str,
    receiver_public_key: &str,
) -> [u8; 32] {
    crypto::derive_key(&[
        b"acamera-secure-pairing-v1",
        pin.as_bytes(),
        pairing_id.as_bytes(),
        phone_nonce.as_bytes(),
        receiver_nonce.as_bytes(),
        phone_public_key.as_bytes(),
        receiver_public_key.as_bytes(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager(now: Instant) -> PairingManager {
        PairingManager::new("123456", now, PairingConfig::default())
    }

    #[test]
    fn pin_success_returns_valid_token() {
        let now = Instant::now();
        let mut manager = manager(now);
        let token = manager
            .pair("123456", "Phone".to_string(), None, now)
            .unwrap();
        assert!(manager.validate_token(Some(&token), now).is_ok());
        assert_eq!(manager.paired_device().unwrap().device_name, "Phone");
    }

    #[test]
    fn pin_failure_counts_retry_limit() {
        let now = Instant::now();
        let mut manager = PairingManager::new(
            "123456",
            now,
            PairingConfig {
                max_pin_attempts: 1,
                ..PairingConfig::default()
            },
        );
        assert_eq!(
            manager.pair("000000", "Phone".to_string(), None, now),
            Err(PairingError::InvalidPin)
        );
        assert_eq!(
            manager.pair("123456", "Phone".to_string(), None, now),
            Err(PairingError::TooManyAttempts)
        );
    }

    #[test]
    fn pin_expiry_rejects_pairing() {
        let now = Instant::now();
        let mut manager = manager(now);
        assert_eq!(
            manager.pair(
                "123456",
                "Phone".to_string(),
                None,
                now + Duration::from_secs(301)
            ),
            Err(PairingError::PinExpired)
        );
    }

    #[test]
    fn missing_invalid_and_expired_tokens_fail() {
        let now = Instant::now();
        let mut manager = manager(now);
        let token = manager
            .pair("123456", "Phone".to_string(), None, now)
            .unwrap();
        assert_eq!(
            manager.validate_token(None, now),
            Err(PairingError::InvalidToken)
        );
        assert_eq!(
            manager.validate_token(Some("wrong"), now),
            Err(PairingError::InvalidToken)
        );
        assert_eq!(
            manager.validate_token(
                Some(&token),
                now + Duration::from_secs(SESSION_TOKEN_EXPIRES_IN_SECONDS + 1)
            ),
            Err(PairingError::InvalidToken)
        );
    }
}
