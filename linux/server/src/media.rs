use std::{
    net::UdpSocket,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use crate::{
    crypto,
    protocol::{
        AudioSettings, Codec, MediaEncryption, QualityPreset, SessionStartResponse, VideoSettings,
    },
    rtp, virtual_mic,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtpEndpoint {
    pub port: u16,
    pub payload_type: u8,
    pub ssrc: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaSessionSpec {
    pub session_id: String,
    pub quality_preset: QualityPreset,
    pub encryption: Option<MediaEncryption>,
    pub video: VideoStreamSpec,
    pub audio: AudioStreamSpec,
}

impl MediaSessionSpec {
    pub fn from_negotiated(
        negotiated: &SessionStartResponse,
        camera_device: impl Into<String>,
        microphone_device: impl Into<String>,
    ) -> Self {
        Self {
            session_id: negotiated.session_id.clone(),
            quality_preset: negotiated.quality_preset,
            encryption: negotiated.media_encryption.clone(),
            video: VideoStreamSpec {
                rtp: RtpEndpoint {
                    port: negotiated.video_rtp_port,
                    payload_type: negotiated.video_payload_type,
                    ssrc: negotiated.ssrc_video,
                },
                settings: negotiated.video.clone(),
                v4l2_device: camera_device.into(),
            },
            audio: AudioStreamSpec {
                rtp: RtpEndpoint {
                    port: negotiated.audio_rtp_port,
                    payload_type: negotiated.audio_payload_type,
                    ssrc: negotiated.ssrc_audio,
                },
                settings: negotiated.audio.clone(),
                pulse_sink_name: microphone_device.into(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoStreamSpec {
    pub rtp: RtpEndpoint,
    pub settings: VideoSettings,
    pub v4l2_device: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioStreamSpec {
    pub rtp: RtpEndpoint,
    pub settings: AudioSettings,
    pub pulse_sink_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GStreamerLaunch {
    pub program: String,
    pub args: Vec<String>,
}

impl GStreamerLaunch {
    pub fn command_line(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaPipelineSpec {
    pub session_id: String,
    pub video: GStreamerLaunch,
    pub audio: GStreamerLaunch,
}

pub fn build_gstreamer_pipeline(spec: &MediaSessionSpec) -> Result<MediaPipelineSpec, MediaError> {
    if spec.video.settings.codec != Codec::H264 {
        return Err(MediaError::UnsupportedCodec(
            "video must be h264".to_string(),
        ));
    }
    if spec.audio.settings.codec != Codec::Opus {
        return Err(MediaError::UnsupportedCodec(
            "audio must be opus".to_string(),
        ));
    }

    Ok(MediaPipelineSpec {
        session_id: spec.session_id.clone(),
        video: video_launch(&spec.video, spec.encryption.is_some()),
        audio: audio_launch(&spec.audio, spec.encryption.is_some()),
    })
}

fn video_launch(spec: &VideoStreamSpec, encrypted: bool) -> GStreamerLaunch {
    GStreamerLaunch {
        program: "gst-launch-1.0".to_string(),
        args: vec![
            "-q".to_string(),
            "udpsrc".to_string(),
            format!("port={}", media_input_port(spec.rtp.port, encrypted)),
            format!(
                "caps=application/x-rtp,media=video,encoding-name=H264,payload={},clock-rate=90000,ssrc={}",
                spec.rtp.payload_type, spec.rtp.ssrc
            ),
            "!".to_string(),
            "rtph264depay".to_string(),
            "!".to_string(),
            "h264parse".to_string(),
            "!".to_string(),
            "avdec_h264".to_string(),
            "!".to_string(),
            "videoconvert".to_string(),
            "!".to_string(),
            "v4l2sink".to_string(),
            format!("device={}", spec.v4l2_device),
            "sync=false".to_string(),
        ],
    }
}

fn audio_launch(spec: &AudioStreamSpec, encrypted: bool) -> GStreamerLaunch {
    GStreamerLaunch {
        program: "gst-launch-1.0".to_string(),
        args: vec![
            "-q".to_string(),
            "udpsrc".to_string(),
            format!("port={}", media_input_port(spec.rtp.port, encrypted)),
            format!(
                "caps=application/x-rtp,media=audio,encoding-name=OPUS,payload={},clock-rate={},ssrc={}",
                spec.rtp.payload_type, spec.settings.sample_rate_hz, spec.rtp.ssrc
            ),
            "!".to_string(),
            "rtpopusdepay".to_string(),
            "!".to_string(),
            "opusdec".to_string(),
            "!".to_string(),
            "audioconvert".to_string(),
            "!".to_string(),
            "audioresample".to_string(),
            "!".to_string(),
            "pulsesink".to_string(),
            format!("device={}", spec.pulse_sink_name),
            format!(
                "client-name={}",
                shellish_value(virtual_mic::DEFAULT_SOURCE_DESCRIPTION)
            ),
            "sync=false".to_string(),
        ],
    }
}

fn media_input_port(port: u16, encrypted: bool) -> u16 {
    if encrypted {
        port.saturating_add(1_000)
    } else {
        port
    }
}

fn shellish_value(value: &str) -> String {
    if value.contains(' ') {
        format!("\"{value}\"")
    } else {
        value.to_string()
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MediaError {
    #[error("unsupported media codec: {0}")]
    UnsupportedCodec(String),
    #[error("GStreamer pipeline failed: {0}")]
    GStreamer(String),
    #[error("media session is already active")]
    AlreadyActive,
    #[error("media session is not active")]
    NotActive,
    #[error("media encryption is invalid: {0}")]
    InvalidEncryption(String),
}

pub trait MediaRuntime: Send {
    fn start(&mut self, spec: MediaSessionSpec) -> Result<(), MediaError>;
    fn stop(&mut self) -> Result<(), MediaError>;
    fn active_session_id(&self) -> Option<String>;
    fn stats(&self) -> MediaStatsSnapshot {
        MediaStatsSnapshot::default()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MediaStatsSnapshot {
    pub video_packets: u64,
    pub audio_packets: u64,
    pub video_bytes: u64,
    pub audio_bytes: u64,
    pub video_malformed: u64,
    pub audio_malformed: u64,
}

#[derive(Default)]
pub struct NoopMediaRuntime {
    active_session_id: Option<String>,
}

impl MediaRuntime for NoopMediaRuntime {
    fn start(&mut self, spec: MediaSessionSpec) -> Result<(), MediaError> {
        if self.active_session_id.is_some() {
            return Err(MediaError::AlreadyActive);
        }
        self.active_session_id = Some(spec.session_id);
        Ok(())
    }

    fn stop(&mut self) -> Result<(), MediaError> {
        self.active_session_id.take().ok_or(MediaError::NotActive)?;
        Ok(())
    }

    fn active_session_id(&self) -> Option<String> {
        self.active_session_id.clone()
    }
}

pub struct GStreamerMediaRuntime<R: ProcessRunner = SystemProcessRunner> {
    runner: R,
    active: Option<RunningMediaPipelines>,
}

impl GStreamerMediaRuntime<SystemProcessRunner> {
    pub fn system() -> Self {
        Self::new(SystemProcessRunner)
    }
}

impl<R: ProcessRunner> GStreamerMediaRuntime<R> {
    pub fn new(runner: R) -> Self {
        Self {
            runner,
            active: None,
        }
    }
}

impl<R: ProcessRunner + Send> MediaRuntime for GStreamerMediaRuntime<R> {
    fn start(&mut self, spec: MediaSessionSpec) -> Result<(), MediaError> {
        if self.active.is_some() {
            return Err(MediaError::AlreadyActive);
        }
        let stats = Arc::new(MediaStatsCounters::default());
        let decrypt = DecryptWorkers::start(&spec, Arc::clone(&stats))?;
        let pipeline = build_gstreamer_pipeline(&spec)?;
        let mut video = self.runner.spawn(&pipeline.video)?;
        let audio = match self.runner.spawn(&pipeline.audio) {
            Ok(audio) => audio,
            Err(error) => {
                let _ = video.stop();
                if let Some(decrypt) = decrypt {
                    decrypt.stop();
                }
                return Err(error);
            }
        };
        self.active = Some(RunningMediaPipelines {
            session_id: pipeline.session_id,
            video,
            audio,
            decrypt,
            stats,
        });
        Ok(())
    }

    fn stop(&mut self) -> Result<(), MediaError> {
        let mut active = self.active.take().ok_or(MediaError::NotActive)?;
        let video_result = active.video.stop();
        let audio_result = active.audio.stop();
        if let Some(decrypt) = active.decrypt {
            decrypt.stop();
        }
        video_result.and(audio_result)
    }

    fn active_session_id(&self) -> Option<String> {
        self.active.as_ref().map(|active| active.session_id.clone())
    }

    fn stats(&self) -> MediaStatsSnapshot {
        self.active
            .as_ref()
            .map(|active| active.stats.snapshot())
            .unwrap_or_default()
    }
}

impl<R: ProcessRunner> Drop for GStreamerMediaRuntime<R> {
    fn drop(&mut self) {
        if let Some(mut active) = self.active.take() {
            let _ = active.video.stop();
            let _ = active.audio.stop();
            if let Some(decrypt) = active.decrypt {
                decrypt.stop();
            }
        }
    }
}

struct RunningMediaPipelines {
    session_id: String,
    video: Box<dyn ProcessHandle>,
    audio: Box<dyn ProcessHandle>,
    decrypt: Option<DecryptWorkers>,
    stats: Arc<MediaStatsCounters>,
}

#[derive(Default)]
struct MediaStatsCounters {
    video_packets: AtomicU64,
    audio_packets: AtomicU64,
    video_bytes: AtomicU64,
    audio_bytes: AtomicU64,
    video_malformed: AtomicU64,
    audio_malformed: AtomicU64,
}

impl MediaStatsCounters {
    fn snapshot(&self) -> MediaStatsSnapshot {
        MediaStatsSnapshot {
            video_packets: self.video_packets.load(Ordering::Relaxed),
            audio_packets: self.audio_packets.load(Ordering::Relaxed),
            video_bytes: self.video_bytes.load(Ordering::Relaxed),
            audio_bytes: self.audio_bytes.load(Ordering::Relaxed),
            video_malformed: self.video_malformed.load(Ordering::Relaxed),
            audio_malformed: self.audio_malformed.load(Ordering::Relaxed),
        }
    }

    fn observe(&self, stream: MediaStreamKind, bytes: usize) {
        match stream {
            MediaStreamKind::Video => {
                self.video_packets.fetch_add(1, Ordering::Relaxed);
                self.video_bytes.fetch_add(bytes as u64, Ordering::Relaxed);
            }
            MediaStreamKind::Audio => {
                self.audio_packets.fetch_add(1, Ordering::Relaxed);
                self.audio_bytes.fetch_add(bytes as u64, Ordering::Relaxed);
            }
        }
    }

    fn observe_malformed(&self, stream: MediaStreamKind) {
        match stream {
            MediaStreamKind::Video => {
                self.video_malformed.fetch_add(1, Ordering::Relaxed);
            }
            MediaStreamKind::Audio => {
                self.audio_malformed.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

#[derive(Clone, Copy)]
enum MediaStreamKind {
    Video,
    Audio,
}

struct DecryptWorkers {
    stop: Arc<AtomicBool>,
    handles: Vec<thread::JoinHandle<()>>,
}

impl DecryptWorkers {
    fn start(
        spec: &MediaSessionSpec,
        stats: Arc<MediaStatsCounters>,
    ) -> Result<Option<Self>, MediaError> {
        let Some(encryption) = &spec.encryption else {
            return Ok(None);
        };
        let video_key = crypto::hex_decode(&encryption.video_key)
            .map_err(|error| MediaError::InvalidEncryption(error.to_string()))?;
        let audio_key = crypto::hex_decode(&encryption.audio_key)
            .map_err(|error| MediaError::InvalidEncryption(error.to_string()))?;
        let video_key: [u8; 32] = video_key
            .try_into()
            .map_err(|_| MediaError::InvalidEncryption("video key must be 32 bytes".to_string()))?;
        let audio_key: [u8; 32] = audio_key
            .try_into()
            .map_err(|_| MediaError::InvalidEncryption("audio key must be 32 bytes".to_string()))?;
        let stop = Arc::new(AtomicBool::new(false));
        let mut handles = Vec::new();
        match spawn_decrypt_worker(
            "video",
            spec.video.rtp.port,
            media_input_port(spec.video.rtp.port, true),
            video_key,
            Arc::clone(&stop),
            Arc::clone(&stats),
            MediaStreamKind::Video,
        ) {
            Ok(handle) => handles.push(handle),
            Err(error) => return Err(error),
        }
        match spawn_decrypt_worker(
            "audio",
            spec.audio.rtp.port,
            media_input_port(spec.audio.rtp.port, true),
            audio_key,
            Arc::clone(&stop),
            Arc::clone(&stats),
            MediaStreamKind::Audio,
        ) {
            Ok(handle) => handles.push(handle),
            Err(error) => {
                stop.store(true, Ordering::Relaxed);
                for handle in handles {
                    let _ = handle.join();
                }
                return Err(error);
            }
        }
        Ok(Some(Self { stop, handles }))
    }

    fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
        for handle in self.handles {
            let _ = handle.join();
        }
    }
}

fn spawn_decrypt_worker(
    label: &'static str,
    encrypted_port: u16,
    plaintext_port: u16,
    key: [u8; 32],
    stop: Arc<AtomicBool>,
    stats: Arc<MediaStatsCounters>,
    stream: MediaStreamKind,
) -> Result<thread::JoinHandle<()>, MediaError> {
    let socket = UdpSocket::bind(("0.0.0.0", encrypted_port))
        .map_err(|error| MediaError::InvalidEncryption(format!("bind {label}: {error}")))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(100)))
        .map_err(|error| MediaError::InvalidEncryption(format!("timeout {label}: {error}")))?;
    let forward = UdpSocket::bind(("127.0.0.1", 0))
        .map_err(|error| MediaError::InvalidEncryption(format!("forward {label}: {error}")))?;
    Ok(thread::spawn(move || {
        let mut buf = [0_u8; 2048];
        while !stop.load(Ordering::Relaxed) {
            let Ok((len, _addr)) = socket.recv_from(&mut buf) else {
                continue;
            };
            match crypto::decrypt_packet(&key, label.as_bytes(), &buf[..len]) {
                Ok(plaintext) => {
                    if rtp::parse_packet(&plaintext).is_ok() {
                        stats.observe(stream, plaintext.len());
                    } else {
                        stats.observe_malformed(stream);
                    }
                    let _ = forward.send_to(&plaintext, ("127.0.0.1", plaintext_port));
                }
                Err(error) => {
                    tracing::warn!(%error, stream = label, "dropped encrypted RTP packet");
                }
            }
        }
    }))
}

pub trait ProcessRunner {
    fn spawn(&self, launch: &GStreamerLaunch) -> Result<Box<dyn ProcessHandle>, MediaError>;
}

pub trait ProcessHandle: Send {
    fn stop(&mut self) -> Result<(), MediaError>;
}

pub struct SystemProcessRunner;

impl ProcessRunner for SystemProcessRunner {
    fn spawn(&self, launch: &GStreamerLaunch) -> Result<Box<dyn ProcessHandle>, MediaError> {
        let child = Command::new(&launch.program)
            .args(&launch.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                MediaError::GStreamer(format!("failed to spawn {}: {error}", launch.program))
            })?;
        Ok(Box::new(ChildProcessHandle { child }))
    }
}

struct ChildProcessHandle {
    child: Child,
}

impl ProcessHandle for ChildProcessHandle {
    fn stop(&mut self) -> Result<(), MediaError> {
        match self.child.try_wait() {
            Ok(Some(_status)) => Ok(()),
            Ok(None) => {
                self.child
                    .kill()
                    .map_err(|error| MediaError::GStreamer(format!("failed to stop: {error}")))?;
                self.child
                    .wait()
                    .map_err(|error| MediaError::GStreamer(format!("failed to reap: {error}")))?;
                Ok(())
            }
            Err(error) => Err(MediaError::GStreamer(format!(
                "failed to inspect process: {error}"
            ))),
        }
    }
}

#[derive(Default, Clone)]
pub struct RecordingProcessRunner {
    pub launches: Arc<Mutex<Vec<GStreamerLaunch>>>,
    pub stopped: Arc<Mutex<usize>>,
    fail_on_spawn: Option<usize>,
}

impl RecordingProcessRunner {
    pub fn fail_on_spawn(spawn_number: usize) -> Self {
        Self {
            launches: Arc::default(),
            stopped: Arc::default(),
            fail_on_spawn: Some(spawn_number),
        }
    }
}

impl ProcessRunner for RecordingProcessRunner {
    fn spawn(&self, launch: &GStreamerLaunch) -> Result<Box<dyn ProcessHandle>, MediaError> {
        let mut launches = self.launches.lock().unwrap();
        let spawn_number = launches.len() + 1;
        launches.push(launch.clone());
        if self.fail_on_spawn == Some(spawn_number) {
            return Err(MediaError::GStreamer(format!(
                "fake spawn {spawn_number} failed"
            )));
        }
        Ok(Box::new(RecordingProcessHandle {
            stopped: Arc::clone(&self.stopped),
        }))
    }
}

struct RecordingProcessHandle {
    stopped: Arc<Mutex<usize>>,
}

impl ProcessHandle for RecordingProcessHandle {
    fn stop(&mut self) -> Result<(), MediaError> {
        *self.stopped.lock().unwrap() += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        protocol::{
            AudioSettings, DEFAULT_AUDIO_PAYLOAD_TYPE, DEFAULT_AUDIO_RTP_PORT, DEFAULT_AUDIO_SSRC,
            DEFAULT_RECEIVER_HOST, DEFAULT_VIDEO_PAYLOAD_TYPE, DEFAULT_VIDEO_RTP_PORT,
            DEFAULT_VIDEO_SSRC, DEVICE_CAMERA_NAME, DEVICE_MICROPHONE_NAME, QualityPreset,
            SessionStartResponse, VideoSettings,
        },
        virtual_mic,
    };

    fn negotiated() -> SessionStartResponse {
        SessionStartResponse {
            session_id: "sess_test".to_string(),
            receiver_host: DEFAULT_RECEIVER_HOST.to_string(),
            video_rtp_port: DEFAULT_VIDEO_RTP_PORT,
            audio_rtp_port: DEFAULT_AUDIO_RTP_PORT,
            video_payload_type: DEFAULT_VIDEO_PAYLOAD_TYPE,
            audio_payload_type: DEFAULT_AUDIO_PAYLOAD_TYPE,
            ssrc_video: DEFAULT_VIDEO_SSRC,
            ssrc_audio: DEFAULT_AUDIO_SSRC,
            quality_preset: QualityPreset::Balanced,
            video: VideoSettings::default(),
            audio: AudioSettings::default(),
            media_encryption: None,
        }
    }

    #[test]
    fn builds_video_gstreamer_args_from_negotiated_ports_payloads_and_device() {
        let media = MediaSessionSpec::from_negotiated(
            &negotiated(),
            "/dev/video10",
            DEVICE_MICROPHONE_NAME,
        );
        let pipeline = build_gstreamer_pipeline(&media).unwrap();
        let line = pipeline.video.command_line();

        assert!(line.contains("udpsrc port=5004"));
        assert!(line.contains("encoding-name=H264,payload=96,clock-rate=90000,ssrc=305419896"));
        assert!(line.contains("rtph264depay ! h264parse ! avdec_h264 ! videoconvert"));
        assert!(!line.contains("video/x-raw,width=1280,height=720,framerate=30/1"));
        assert!(line.contains("v4l2sink device=/dev/video10"));
    }

    #[test]
    fn builds_audio_gstreamer_args_from_negotiated_ports_payloads_and_device_name() {
        let media = MediaSessionSpec::from_negotiated(
            &negotiated(),
            DEVICE_CAMERA_NAME,
            virtual_mic::DEFAULT_SINK_NAME,
        );
        let pipeline = build_gstreamer_pipeline(&media).unwrap();
        let line = pipeline.audio.command_line();

        assert!(line.contains("udpsrc port=5006"));
        assert!(line.contains("encoding-name=OPUS,payload=97,clock-rate=48000,ssrc=2596069104"));
        assert!(line.contains("rtpopusdepay ! opusdec ! audioconvert ! audioresample"));
        assert!(line.contains("pulsesink device=acamera_sink client-name=\"ACamera Microphone\""));
    }

    #[test]
    fn runtime_starts_both_pipelines_and_stops_both_processes() {
        let runner = RecordingProcessRunner::default();
        let launches = Arc::clone(&runner.launches);
        let stopped = Arc::clone(&runner.stopped);
        let mut runtime = GStreamerMediaRuntime::new(runner);

        runtime
            .start(MediaSessionSpec::from_negotiated(
                &negotiated(),
                "/dev/video10",
                DEVICE_MICROPHONE_NAME,
            ))
            .unwrap();
        assert_eq!(runtime.active_session_id(), Some("sess_test".to_string()));
        assert_eq!(launches.lock().unwrap().len(), 2);

        runtime.stop().unwrap();
        assert_eq!(*stopped.lock().unwrap(), 2);
        assert_eq!(runtime.active_session_id(), None);
    }

    #[test]
    fn runtime_stops_active_pipelines_when_dropped() {
        let runner = RecordingProcessRunner::default();
        let stopped = Arc::clone(&runner.stopped);
        {
            let mut runtime = GStreamerMediaRuntime::new(runner);
            runtime
                .start(MediaSessionSpec::from_negotiated(
                    &negotiated(),
                    "/dev/video10",
                    DEVICE_MICROPHONE_NAME,
                ))
                .unwrap();
            assert_eq!(*stopped.lock().unwrap(), 0);
        }
        assert_eq!(*stopped.lock().unwrap(), 2);
    }

    #[test]
    fn runtime_rolls_back_video_process_when_audio_spawn_fails() {
        let runner = RecordingProcessRunner::fail_on_spawn(2);
        let stopped = Arc::clone(&runner.stopped);
        let mut runtime = GStreamerMediaRuntime::new(runner);

        let error = runtime
            .start(MediaSessionSpec::from_negotiated(
                &negotiated(),
                "/dev/video10",
                DEVICE_MICROPHONE_NAME,
            ))
            .unwrap_err();

        assert_eq!(
            error,
            MediaError::GStreamer("fake spawn 2 failed".to_string())
        );
        assert_eq!(*stopped.lock().unwrap(), 1);
        assert_eq!(runtime.active_session_id(), None);
    }
}
