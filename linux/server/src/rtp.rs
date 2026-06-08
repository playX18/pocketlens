#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtpPacket<'a> {
    pub payload_type: u8,
    pub marker: bool,
    pub sequence_number: u16,
    pub timestamp: u32,
    pub ssrc: u32,
    pub payload: &'a [u8],
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RtpError {
    #[error("RTP packet is shorter than the fixed header")]
    TooShort,
    #[error("unsupported RTP version {0}")]
    UnsupportedVersion(u8),
    #[error("RTP packet extension is not supported in v1 parser")]
    ExtensionUnsupported,
    #[error("RTP packet has invalid CSRC count")]
    InvalidCsrcCount,
}

pub fn parse_packet(bytes: &[u8]) -> Result<RtpPacket<'_>, RtpError> {
    if bytes.len() < 12 {
        return Err(RtpError::TooShort);
    }
    let version = bytes[0] >> 6;
    if version != 2 {
        return Err(RtpError::UnsupportedVersion(version));
    }
    let extension = bytes[0] & 0x10 != 0;
    if extension {
        return Err(RtpError::ExtensionUnsupported);
    }
    let csrc_count = (bytes[0] & 0x0f) as usize;
    let header_len = 12 + csrc_count * 4;
    if bytes.len() < header_len {
        return Err(RtpError::InvalidCsrcCount);
    }

    Ok(RtpPacket {
        marker: bytes[1] & 0x80 != 0,
        payload_type: bytes[1] & 0x7f,
        sequence_number: u16::from_be_bytes([bytes[2], bytes[3]]),
        timestamp: u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
        ssrc: u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
        payload: &bytes[header_len..],
    })
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RtpStats {
    pub packets: u64,
    pub lost: u64,
    pub malformed: u64,
    pub jitter: f64,
    last_sequence: Option<u16>,
    last_transit: Option<i64>,
}

impl RtpStats {
    pub fn observe_packet(&mut self, packet: &RtpPacket<'_>, arrival_timestamp: u32) {
        if let Some(last) = self.last_sequence {
            let expected = last.wrapping_add(1);
            if packet.sequence_number != expected {
                self.lost += packet.sequence_number.wrapping_sub(expected) as u64;
            }
        }

        let transit = arrival_timestamp as i64 - packet.timestamp as i64;
        if let Some(last_transit) = self.last_transit {
            let delta = (transit - last_transit).abs() as f64;
            self.jitter += (delta - self.jitter) / 16.0;
        }
        self.last_transit = Some(transit);
        self.last_sequence = Some(packet.sequence_number);
        self.packets += 1;
    }

    pub fn observe_malformed(&mut self) {
        self.malformed += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(seq: u16, ts: u32) -> Vec<u8> {
        let mut bytes = vec![0x80, 0xe0];
        bytes.extend(seq.to_be_bytes());
        bytes.extend(ts.to_be_bytes());
        bytes.extend(0x11223344_u32.to_be_bytes());
        bytes.extend([1, 2, 3]);
        bytes
    }

    #[test]
    fn parses_sequence_timestamp_ssrc_payload_type_marker_and_payload() {
        let bytes = packet(42, 9000);
        let parsed = parse_packet(&bytes).unwrap();
        assert_eq!(parsed.payload_type, 96);
        assert!(parsed.marker);
        assert_eq!(parsed.sequence_number, 42);
        assert_eq!(parsed.timestamp, 9000);
        assert_eq!(parsed.ssrc, 0x11223344);
        assert_eq!(parsed.payload, &[1, 2, 3]);
    }

    #[test]
    fn rejects_malformed_packets() {
        assert_eq!(parse_packet(&[0, 1]), Err(RtpError::TooShort));
        let mut invalid_version = packet(1, 1);
        invalid_version[0] = 0x40;
        assert_eq!(
            parse_packet(&invalid_version),
            Err(RtpError::UnsupportedVersion(1))
        );
    }

    #[test]
    fn tracks_packet_loss_and_jitter() {
        let p1_bytes = packet(10, 1000);
        let p2_bytes = packet(12, 2000);
        let p1 = parse_packet(&p1_bytes).unwrap();
        let p2 = parse_packet(&p2_bytes).unwrap();
        let mut stats = RtpStats::default();
        stats.observe_packet(&p1, 1100);
        stats.observe_packet(&p2, 2150);
        assert_eq!(stats.packets, 2);
        assert_eq!(stats.lost, 1);
        assert!(stats.jitter > 0.0);
    }
}
