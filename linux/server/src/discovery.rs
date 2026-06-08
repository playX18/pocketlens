use std::net::SocketAddr;

use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;

use crate::{
    config::ReceiverConfig,
    protocol::{PROTOCOL_VERSION, SERVICE_TYPE},
};

pub const DISCOVERY_PROBE: &str = "ACAMERA_DISCOVER_V1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryResponse {
    pub receiver_name: String,
    pub protocol_version: u16,
    pub service_type: String,
    pub control_port: u16,
    pub capabilities: Vec<String>,
}

pub fn spawn_udp_responder(config: ReceiverConfig) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(error) = run_udp_responder(config).await {
            tracing::warn!(%error, "PocketLens UDP discovery responder stopped");
        }
    })
}

async fn run_udp_responder(config: ReceiverConfig) -> anyhow::Result<()> {
    let bind = SocketAddr::new(config.bind_address, config.control_port);
    let socket = UdpSocket::bind(bind).await?;
    let response = serde_json::to_vec(&DiscoveryResponse {
        receiver_name: config.receiver_name,
        protocol_version: PROTOCOL_VERSION,
        service_type: SERVICE_TYPE.to_string(),
        control_port: config.control_port,
        capabilities: vec![
            "h264".to_string(),
            "opus".to_string(),
            "rtp".to_string(),
            "secure_pairing".to_string(),
            "encrypted_rtp".to_string(),
        ],
    })?;
    let mut buf = [0_u8; 256];
    loop {
        let (len, peer) = socket.recv_from(&mut buf).await?;
        if std::str::from_utf8(&buf[..len]).map(str::trim) == Ok(DISCOVERY_PROBE) {
            let _ = socket.send_to(&response, peer).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_response_serializes_expected_fields() {
        let response = DiscoveryResponse {
            receiver_name: "Desk".to_string(),
            protocol_version: 1,
            service_type: "_pocketlens._udp.local".to_string(),
            control_port: 3769,
            capabilities: vec!["secure_pairing".to_string()],
        };
        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value["receiver_name"], "Desk");
        assert_eq!(value["control_port"], 3769);
    }
}
