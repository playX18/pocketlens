use std::net::IpAddr;

use crate::{
    crypto,
    protocol::{
        AudioSettings, DEFAULT_AUDIO_PAYLOAD_TYPE, DEFAULT_AUDIO_RTP_PORT, DEFAULT_AUDIO_SSRC,
        DEFAULT_RECEIVER_HOST, DEFAULT_VIDEO_PAYLOAD_TYPE, DEFAULT_VIDEO_RTP_PORT,
        DEFAULT_VIDEO_SSRC, MediaEncryption, MediaEncryptionMode, QualityPreset,
        SessionStartResponse, SessionStateName, VideoSettings,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionConfig {
    pub receiver_host: String,
    pub video_port: u16,
    pub audio_port: u16,
    pub video_payload_type: u8,
    pub audio_payload_type: u8,
    pub ssrc_video: u32,
    pub ssrc_audio: u32,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            receiver_host: DEFAULT_RECEIVER_HOST.to_string(),
            video_port: DEFAULT_VIDEO_RTP_PORT,
            audio_port: DEFAULT_AUDIO_RTP_PORT,
            video_payload_type: DEFAULT_VIDEO_PAYLOAD_TYPE,
            audio_payload_type: DEFAULT_AUDIO_PAYLOAD_TYPE,
            ssrc_video: DEFAULT_VIDEO_SSRC,
            ssrc_audio: DEFAULT_AUDIO_SSRC,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveSession {
    pub session_id: String,
    pub sender_ip: Option<IpAddr>,
    pub preset: QualityPreset,
    pub video: VideoSettings,
    pub audio: AudioSettings,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SessionError {
    #[error("invalid session transition from {0:?}")]
    InvalidTransition(SessionStateName),
}

#[derive(Debug, Clone)]
pub struct SessionManager {
    state: SessionStateName,
    active: Option<ActiveSession>,
    config: SessionConfig,
}

impl SessionManager {
    pub fn new(config: SessionConfig) -> Self {
        Self {
            state: SessionStateName::Idle,
            active: None,
            config,
        }
    }

    pub fn state(&self) -> SessionStateName {
        self.state
    }

    pub fn is_active(&self) -> bool {
        self.state == SessionStateName::Active
    }

    pub fn mark_paired(&mut self) {
        if self.state == SessionStateName::Idle {
            self.state = SessionStateName::Paired;
        }
    }

    pub fn start(
        &mut self,
        sender_ip: Option<IpAddr>,
        preset: QualityPreset,
        video: VideoSettings,
        audio: AudioSettings,
    ) -> Result<SessionStartResponse, SessionError> {
        match self.state {
            SessionStateName::Paired | SessionStateName::Failed => {
                self.state = SessionStateName::Starting;
                let session_id = "sess_0123456789abcdef".to_string();
                self.active = Some(ActiveSession {
                    session_id: session_id.clone(),
                    sender_ip,
                    preset,
                    video: video.clone(),
                    audio: audio.clone(),
                });
                let video_key = crypto::hex_encode(&crypto::derive_key(&[
                    b"pocketlens-media-video-v1",
                    session_id.as_bytes(),
                ]));
                let audio_key = crypto::hex_encode(&crypto::derive_key(&[
                    b"pocketlens-media-audio-v1",
                    session_id.as_bytes(),
                ]));
                self.state = SessionStateName::Active;
                Ok(SessionStartResponse {
                    session_id,
                    receiver_host: self.config.receiver_host.clone(),
                    video_rtp_port: self.config.video_port,
                    audio_rtp_port: self.config.audio_port,
                    video_payload_type: self.config.video_payload_type,
                    audio_payload_type: self.config.audio_payload_type,
                    ssrc_video: self.config.ssrc_video,
                    ssrc_audio: self.config.ssrc_audio,
                    quality_preset: preset,
                    video,
                    audio,
                    media_encryption: Some(MediaEncryption {
                        mode: MediaEncryptionMode::Aes256GcmV1,
                        video_key,
                        audio_key,
                    }),
                })
            }
            other => Err(SessionError::InvalidTransition(other)),
        }
    }

    pub fn stop(&mut self) -> Result<(), SessionError> {
        match self.state {
            SessionStateName::Active => {
                self.state = SessionStateName::Stopping;
                self.active = None;
                self.state = SessionStateName::Paired;
                Ok(())
            }
            other => Err(SessionError::InvalidTransition(other)),
        }
    }

    pub fn fail(&mut self) {
        self.active = None;
        self.state = SessionStateName::Failed;
    }

    pub fn active_session_id(&self) -> Option<&str> {
        self.active
            .as_ref()
            .map(|session| session.session_id.as_str())
    }

    pub fn active_session(&self) -> Option<&ActiveSession> {
        self.active.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_transitions_from_idle_to_paired_to_active_to_paired() {
        let mut manager = SessionManager::new(SessionConfig::default());
        assert_eq!(manager.state(), SessionStateName::Idle);
        manager.mark_paired();
        assert_eq!(manager.state(), SessionStateName::Paired);
        let negotiated = manager
            .start(
                None,
                QualityPreset::Balanced,
                VideoSettings::default(),
                AudioSettings::default(),
            )
            .unwrap();
        assert_eq!(manager.state(), SessionStateName::Active);
        assert_eq!(negotiated.video_rtp_port, 5004);
        assert_eq!(negotiated.audio_rtp_port, 5006);
        assert_eq!(negotiated.ssrc_video, 0x1234_5678);
        assert_eq!(negotiated.video, VideoSettings::default());
        assert_eq!(negotiated.audio, AudioSettings::default());
        manager.stop().unwrap();
        assert_eq!(manager.state(), SessionStateName::Paired);
    }

    #[test]
    fn only_one_active_sender_is_allowed() {
        let mut manager = SessionManager::new(SessionConfig::default());
        manager.mark_paired();
        manager
            .start(
                None,
                QualityPreset::Low,
                VideoSettings::default(),
                AudioSettings::default(),
            )
            .unwrap();
        assert_eq!(
            manager.start(
                None,
                QualityPreset::High,
                VideoSettings::default(),
                AudioSettings::default(),
            ),
            Err(SessionError::InvalidTransition(SessionStateName::Active))
        );
    }

    #[test]
    fn stop_without_active_session_fails() {
        let mut manager = SessionManager::new(SessionConfig::default());
        manager.mark_paired();
        assert_eq!(
            manager.stop(),
            Err(SessionError::InvalidTransition(SessionStateName::Paired))
        );
    }
}
