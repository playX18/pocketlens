use std::net::{IpAddr, Ipv4Addr};

use clap::Parser;

use crate::protocol::{
    DEFAULT_AUDIO_RTP_PORT, DEFAULT_RECEIVER_HOST, DEFAULT_VIDEO_RTP_PORT, DEVICE_CAMERA_NAME,
    DEVICE_MICROPHONE_NAME, PROTOCOL_VERSION, QualityPreset, RECEIVER_NAME,
};
use crate::virtual_mic;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiverConfig {
    pub bind_address: IpAddr,
    pub control_port: u16,
    pub receiver_name: String,
    pub default_preset: QualityPreset,
    pub camera_device_name: String,
    pub camera_device_path: String,
    pub microphone_device_name: String,
    pub microphone_sink_name: String,
    pub protocol_version: u16,
    pub video_port: u16,
    pub audio_port: u16,
    pub receiver_host: String,
}

impl Default for ReceiverConfig {
    fn default() -> Self {
        Self {
            bind_address: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            control_port: 47650,
            receiver_name: RECEIVER_NAME.to_string(),
            default_preset: QualityPreset::Balanced,
            camera_device_name: DEVICE_CAMERA_NAME.to_string(),
            camera_device_path: "/dev/video10".to_string(),
            microphone_device_name: DEVICE_MICROPHONE_NAME.to_string(),
            microphone_sink_name: virtual_mic::DEFAULT_SINK_NAME.to_string(),
            protocol_version: PROTOCOL_VERSION,
            video_port: DEFAULT_VIDEO_RTP_PORT,
            audio_port: DEFAULT_AUDIO_RTP_PORT,
            receiver_host: DEFAULT_RECEIVER_HOST.to_string(),
        }
    }
}

#[derive(Debug, Parser)]
#[command(name = "acamera-receiver")]
#[command(about = "Linux receiver for Android camera and microphone streams")]
pub struct Cli {
    #[arg(long, default_value_t = ReceiverConfig::default().bind_address)]
    pub bind_address: IpAddr,
    #[arg(long, default_value_t = ReceiverConfig::default().control_port)]
    pub control_port: u16,
    #[arg(long, default_value = RECEIVER_NAME)]
    pub receiver_name: String,
    #[arg(long, value_enum, default_value_t = QualityPreset::Balanced)]
    pub preset: QualityPreset,
    #[arg(long, default_value_t = ReceiverConfig::default().video_port)]
    pub video_port: u16,
    #[arg(long, default_value_t = ReceiverConfig::default().audio_port)]
    pub audio_port: u16,
    #[arg(long, default_value = DEFAULT_RECEIVER_HOST)]
    pub receiver_host: String,
    #[arg(long, default_value = "/dev/video10")]
    pub camera_device: String,
    #[arg(long, default_value = virtual_mic::DEFAULT_SINK_NAME)]
    pub microphone_sink: String,
    #[arg(long)]
    pub setup_virtual_mic: bool,
    #[arg(long)]
    pub remove_virtual_mic: bool,
    #[arg(long)]
    pub diagnose: bool,
    #[arg(long)]
    pub check_deps: bool,
    #[arg(long)]
    pub cleanup: bool,
    #[arg(long)]
    pub setup_camera: bool,
    #[arg(long)]
    pub remove_camera: bool,
    #[arg(long)]
    pub install: bool,
    #[arg(long, default_value = "~/.local")]
    pub prefix: String,
    #[arg(long)]
    pub install_apk: bool,
}

impl Cli {
    pub fn to_config(&self) -> anyhow::Result<ReceiverConfig> {
        if self.control_port == self.video_port
            || self.control_port == self.audio_port
            || self.video_port == self.audio_port
        {
            anyhow::bail!("control, video, and audio ports must be distinct");
        }

        Ok(ReceiverConfig {
            bind_address: self.bind_address,
            control_port: self.control_port,
            receiver_name: self.receiver_name.clone(),
            default_preset: self.preset,
            camera_device_name: DEVICE_CAMERA_NAME.to_string(),
            camera_device_path: self.camera_device.clone(),
            microphone_device_name: DEVICE_MICROPHONE_NAME.to_string(),
            microphone_sink_name: self.microphone_sink.clone(),
            protocol_version: PROTOCOL_VERSION,
            video_port: self.video_port,
            audio_port: self.audio_port,
            receiver_host: self.receiver_host.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[test]
    fn defaults_match_linux_plan() {
        let config = ReceiverConfig::default();
        assert_eq!(config.control_port, 47650);
        assert_eq!(config.receiver_name, "ACamera Linux");
        assert_eq!(config.default_preset, QualityPreset::Balanced);
        assert_eq!(config.camera_device_name, "ACamera");
        assert_eq!(config.camera_device_path, "/dev/video10");
        assert_eq!(config.microphone_device_name, "ACamera Microphone");
        assert_eq!(config.microphone_sink_name, "acamera_sink");
        assert_eq!(config.protocol_version, 1);
        assert_eq!(config.video_port, 5004);
        assert_eq!(config.audio_port, 5006);
        assert_eq!(config.receiver_host, "192.168.1.25");
    }

    #[test]
    fn cli_overrides_receiver_name_ports_and_diagnostic_mode() {
        let cli = Cli::parse_from([
            "acamera-receiver",
            "--receiver-name",
            "Desk",
            "--control-port",
            "5000",
            "--video-port",
            "5002",
            "--audio-port",
            "5004",
            "--camera-device",
            "/dev/video42",
            "--microphone-sink",
            "custom_sink",
            "--diagnose",
        ]);
        let config = cli.to_config().unwrap();
        assert!(cli.diagnose);
        assert_eq!(config.receiver_name, "Desk");
        assert_eq!(config.control_port, 5000);
        assert_eq!(config.video_port, 5002);
        assert_eq!(config.audio_port, 5004);
        assert_eq!(config.camera_device_path, "/dev/video42");
        assert_eq!(config.microphone_sink_name, "custom_sink");
    }

    #[test]
    fn cli_rejects_duplicate_ports() {
        let cli = Cli::parse_from([
            "acamera-receiver",
            "--control-port",
            "5000",
            "--video-port",
            "5000",
        ]);
        assert!(cli.to_config().is_err());
    }
}
