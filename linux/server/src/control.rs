use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use axum::{
    Router,
    extract::{
        Json, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use tokio::sync::broadcast;

use crate::{
    config::ReceiverConfig,
    diagnostics::DependencyReport,
    media::{
        GStreamerMediaRuntime, MediaRuntime, MediaSessionSpec, MediaStatsSnapshot, NoopMediaRuntime,
    },
    pairing::{PairingConfig, PairingError, PairingManager},
    protocol::{
        ApiErrorBody, ApiErrorCode, ApiErrorResponse, Capabilities, DEVICE_CAMERA_NAME,
        DEVICE_MICROPHONE_NAME, EventMessage, PROTOCOL_VERSION, PairRequest, PairResponse,
        PendingPairingResponse, SERVICE_TYPE, SESSION_TOKEN_EXPIRES_IN_SECONDS,
        SecurePairApproveRequest, SecurePairApproveResponse, SecurePairRequest,
        SecurePairRequestResponse, SecurePairResultResponse, SessionStartRequest,
        SessionStopRequest, SessionStopResponse, StatusResponse, VirtualDevice,
        VirtualDeviceBackend, VirtualDevices,
    },
    session::{ActiveSession, SessionConfig, SessionError, SessionManager},
};

#[derive(Clone)]
pub struct AppState {
    inner: Arc<Mutex<InnerState>>,
    events: broadcast::Sender<EventMessage>,
}

struct InnerState {
    config: ReceiverConfig,
    pairing: PairingManager,
    session: SessionManager,
    dependencies: DependencyReport,
    media: Box<dyn MediaRuntime>,
}

impl AppState {
    pub fn new(config: ReceiverConfig) -> Self {
        let now = Instant::now();
        let (events, _) = broadcast::channel(64);
        Self {
            events,
            inner: Arc::new(Mutex::new(InnerState {
                session: SessionManager::new(SessionConfig {
                    receiver_host: config.receiver_host.clone(),
                    video_port: config.video_port,
                    audio_port: config.audio_port,
                    ..SessionConfig::default()
                }),
                pairing: PairingManager::new("000000", now, PairingConfig::default()),
                dependencies: DependencyReport::probe_system(),
                media: Box::new(GStreamerMediaRuntime::system()),
                config,
            })),
        }
    }

    pub fn for_tests(config: ReceiverConfig, pin: &str, dependencies: DependencyReport) -> Self {
        let now = Instant::now();
        let (events, _) = broadcast::channel(64);
        Self {
            events,
            inner: Arc::new(Mutex::new(InnerState {
                session: SessionManager::new(SessionConfig {
                    receiver_host: config.receiver_host.clone(),
                    video_port: config.video_port,
                    audio_port: config.audio_port,
                    ..SessionConfig::default()
                }),
                pairing: PairingManager::new(pin, now, PairingConfig::default()),
                dependencies,
                media: Box::<NoopMediaRuntime>::default(),
                config,
            })),
        }
    }

    #[cfg(test)]
    pub fn for_tests_with_media(
        config: ReceiverConfig,
        pin: &str,
        dependencies: DependencyReport,
        media: Box<dyn MediaRuntime>,
    ) -> Self {
        let now = Instant::now();
        let (events, _) = broadcast::channel(64);
        Self {
            events,
            inner: Arc::new(Mutex::new(InnerState {
                session: SessionManager::new(SessionConfig {
                    receiver_host: config.receiver_host.clone(),
                    video_port: config.video_port,
                    audio_port: config.audio_port,
                    ..SessionConfig::default()
                }),
                pairing: PairingManager::new(pin, now, PairingConfig::default()),
                dependencies,
                media,
                config,
            })),
        }
    }

    fn publish(&self, event: EventMessage) {
        let _ = self.events.send(event);
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/status", get(status))
        .route("/pair", post(pair))
        .route("/pair/request", post(pair_request))
        .route("/pair/pending", get(pair_pending))
        .route("/pair/approve", post(pair_approve))
        .route("/pair/result", get(pair_result))
        .route("/session/start", post(session_start))
        .route("/session/stop", post(session_stop))
        .route("/session/events", get(session_events))
        .with_state(state)
}

#[derive(Debug, Deserialize)]
struct PairResultQuery {
    pairing_id: String,
}

#[derive(Debug, Deserialize)]
struct SessionEventsQuery {
    session_token: String,
    session_id: Option<String>,
}

async fn status(State(state): State<AppState>) -> Json<StatusResponse> {
    let inner = state.inner.lock().unwrap();
    Json(StatusResponse {
        receiver_name: inner.config.receiver_name.clone(),
        protocol_version: inner.config.protocol_version,
        service_type: SERVICE_TYPE.to_string(),
        paired: inner.pairing.is_paired(Instant::now()),
        active_session: inner.session.is_active(),
        capabilities: Capabilities::default(),
        virtual_devices: VirtualDevices {
            camera: VirtualDevice {
                name: DEVICE_CAMERA_NAME.to_string(),
                ready: inner.dependencies.virtual_camera_ready,
                backend: VirtualDeviceBackend::V4l2loopback,
            },
            microphone: VirtualDevice {
                name: DEVICE_MICROPHONE_NAME.to_string(),
                ready: inner.dependencies.virtual_microphone_ready,
                backend: VirtualDeviceBackend::Pipewire,
            },
        },
        diagnostics: inner.dependencies.diagnostics(),
    })
}

async fn pair(State(state): State<AppState>, Json(request): Json<PairRequest>) -> Response {
    let mut inner = state.inner.lock().unwrap();
    match inner
        .pairing
        .pair(&request.pin, request.device_name, None, Instant::now())
    {
        Ok(session_token) => {
            inner.session.mark_paired();
            (
                StatusCode::OK,
                Json(PairResponse {
                    session_token,
                    receiver_name: inner.config.receiver_name.clone(),
                    protocol_version: PROTOCOL_VERSION,
                    expires_in_seconds: SESSION_TOKEN_EXPIRES_IN_SECONDS,
                }),
            )
                .into_response()
        }
        Err(error) => api_error(
            pairing_error_code(error),
            "The pairing PIN is invalid or expired.",
            StatusCode::UNAUTHORIZED,
        ),
    }
}

async fn pair_request(
    State(state): State<AppState>,
    Json(request): Json<SecurePairRequest>,
) -> Response {
    if request.pairing_id.trim().is_empty()
        || request.device_name.trim().is_empty()
        || request.phone_nonce.trim().is_empty()
        || request.phone_public_key.trim().is_empty()
    {
        return api_error(
            ApiErrorCode::BadRequest,
            "pairing request fields must not be empty",
            StatusCode::BAD_REQUEST,
        );
    }

    let mut inner = state.inner.lock().unwrap();
    let (receiver_nonce, receiver_public_key) = inner.pairing.request_secure_pairing(
        request.pairing_id.clone(),
        request.device_name,
        request.phone_nonce,
        request.phone_public_key,
        Instant::now(),
        unix_ms_now(),
    );
    (
        StatusCode::OK,
        Json(SecurePairRequestResponse {
            pairing_id: request.pairing_id,
            receiver_nonce,
            receiver_public_key,
            expires_in_seconds: PairingConfig::default().pin_ttl.as_secs(),
        }),
    )
        .into_response()
}

async fn pair_pending(State(state): State<AppState>) -> Json<PendingPairingResponse> {
    let mut inner = state.inner.lock().unwrap();
    Json(PendingPairingResponse {
        requests: inner.pairing.pending_requests(Instant::now()),
    })
}

async fn pair_approve(
    State(state): State<AppState>,
    Json(request): Json<SecurePairApproveRequest>,
) -> Response {
    let mut inner = state.inner.lock().unwrap();
    let receiver_name = inner.config.receiver_name.clone();
    match inner.pairing.approve_secure_pairing(
        &request.pairing_id,
        &request.pin,
        Instant::now(),
        &receiver_name,
        PROTOCOL_VERSION,
    ) {
        Ok(()) => {
            inner.session.mark_paired();
            (
                StatusCode::OK,
                Json(SecurePairApproveResponse {
                    pairing_id: request.pairing_id,
                    status: crate::protocol::SecurePairingStatus::Approved,
                }),
            )
                .into_response()
        }
        Err(error) => api_error(
            pairing_error_code(error),
            "The pairing PIN is invalid or expired.",
            StatusCode::UNAUTHORIZED,
        ),
    }
}

async fn pair_result(
    State(state): State<AppState>,
    Query(query): Query<PairResultQuery>,
) -> Json<SecurePairResultResponse> {
    let mut inner = state.inner.lock().unwrap();
    let (status, encrypted_result) = inner
        .pairing
        .secure_pairing_result(&query.pairing_id, Instant::now());
    Json(SecurePairResultResponse {
        pairing_id: query.pairing_id,
        status,
        encrypted_result,
    })
}

async fn session_start(
    State(state): State<AppState>,
    Json(request): Json<SessionStartRequest>,
) -> Response {
    let negotiated = {
        let mut inner = state.inner.lock().unwrap();
        if let Err(error) = inner
            .pairing
            .validate_token(Some(&request.session_token), Instant::now())
        {
            return api_error(
                pairing_error_code(error),
                "invalid session token",
                StatusCode::UNAUTHORIZED,
            );
        }
        if !inner.dependencies.virtual_camera_ready || !inner.dependencies.virtual_microphone_ready
        {
            let event = EventMessage::Error {
                session_id: inner
                    .session
                    .active_session_id()
                    .unwrap_or("pending")
                    .to_string(),
                code: ApiErrorCode::MissingDependencies,
                message: "virtual camera or microphone dependencies are missing".to_string(),
            };
            drop(inner);
            state.publish(event);
            return api_error(
                ApiErrorCode::MissingDependencies,
                "virtual camera or microphone dependencies are missing",
                StatusCode::SERVICE_UNAVAILABLE,
            );
        }
        if inner.session.is_active() {
            if let Err(error) = inner.media.stop()
                && error != crate::media::MediaError::NotActive
            {
                let event = EventMessage::Warning {
                    session_id: inner
                        .session
                        .active_session_id()
                        .unwrap_or("stale")
                        .to_string(),
                    code: ApiErrorCode::NetworkDegraded,
                    message: format!("stale media cleanup failed before restart: {error}"),
                };
                state.publish(event);
            }
            let _ = inner.session.stop();
        }
        let negotiated =
            match inner
                .session
                .start(None, request.quality_preset, request.video, request.audio)
            {
                Ok(response) => response,
                Err(error) => return session_error_response(error),
            };
        let media_spec = MediaSessionSpec::from_negotiated(
            &negotiated,
            inner.config.camera_device_path.clone(),
            inner.config.microphone_sink_name.clone(),
        );
        match inner.media.start(media_spec) {
            Ok(()) => negotiated,
            Err(error) => {
                let event = EventMessage::Error {
                    session_id: negotiated.session_id.clone(),
                    code: ApiErrorCode::MediaPipelineFailed,
                    message: format!("failed to start media pipeline: {error}"),
                };
                inner.session.fail();
                drop(inner);
                state.publish(event);
                return api_error(
                    ApiErrorCode::MediaPipelineFailed,
                    &format!("failed to start media pipeline: {error}"),
                    StatusCode::SERVICE_UNAVAILABLE,
                );
            }
        }
    };
    state.publish(EventMessage::Stats {
        session_id: negotiated.session_id.clone(),
        video_packets: 0,
        audio_packets: 0,
        video_packets_lost: 0,
        audio_packets_lost: 0,
        estimated_bitrate_kbps: 0,
        quality_preset: negotiated.quality_preset,
    });
    spawn_stats_publisher(
        state.clone(),
        negotiated.session_id.clone(),
        negotiated.quality_preset,
    );
    (StatusCode::OK, Json(negotiated)).into_response()
}

async fn session_stop(
    State(state): State<AppState>,
    Json(request): Json<SessionStopRequest>,
) -> Response {
    let active_session_id = {
        let mut inner = state.inner.lock().unwrap();
        if let Err(error) = inner
            .pairing
            .validate_token(Some(&request.session_token), Instant::now())
        {
            return api_error(
                pairing_error_code(error),
                "invalid session token",
                StatusCode::UNAUTHORIZED,
            );
        }
        let active_session_id = inner
            .session
            .active_session_id()
            .map(str::to_string)
            .unwrap_or_else(|| request.session_id.clone());
        if !inner.session.is_active() {
            return session_error_response(SessionError::InvalidTransition(inner.session.state()));
        }
        if let Err(error) = inner.media.stop() {
            let event = EventMessage::Error {
                session_id: active_session_id.clone(),
                code: ApiErrorCode::MediaPipelineFailed,
                message: format!("failed to stop media pipeline: {error}"),
            };
            drop(inner);
            state.publish(event);
            return api_error(
                ApiErrorCode::MediaPipelineFailed,
                &format!("failed to stop media pipeline: {error}"),
                StatusCode::SERVICE_UNAVAILABLE,
            );
        }
        match inner.session.stop() {
            Ok(()) => active_session_id,
            Err(error) => return session_error_response(error),
        }
    };
    state.publish(EventMessage::Warning {
        session_id: active_session_id.clone(),
        code: ApiErrorCode::BadRequest,
        message: "Media session stopped.".to_string(),
    });
    (
        StatusCode::OK,
        Json(SessionStopResponse {
            session_id: active_session_id,
            stopped: true,
        }),
    )
        .into_response()
}

async fn session_events(
    State(state): State<AppState>,
    Query(query): Query<SessionEventsQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let initial = {
        let inner = state.inner.lock().unwrap();
        if let Err(error) = inner
            .pairing
            .validate_token(Some(&query.session_token), Instant::now())
        {
            return api_error(
                pairing_error_code(error),
                "invalid session token",
                StatusCode::UNAUTHORIZED,
            );
        }
        let active = inner.session.active_session().cloned();
        if let (Some(requested), Some(active)) = (&query.session_id, active.as_ref())
            && requested != &active.session_id
        {
            return session_error_response(SessionError::InvalidTransition(inner.session.state()));
        }
        active.map(|session| stats_event(&session))
    };
    let receiver = state.events.subscribe();
    ws.on_upgrade(move |socket| stream_events(socket, receiver, query.session_id, initial))
}

async fn stream_events(
    mut socket: WebSocket,
    mut receiver: broadcast::Receiver<EventMessage>,
    session_id: Option<String>,
    initial: Option<EventMessage>,
) {
    if let Some(event) = initial
        && send_event(&mut socket, &event).await.is_err()
    {
        return;
    }

    loop {
        match receiver.recv().await {
            Ok(event) if event_matches_session(&event, session_id.as_deref()) => {
                if send_event(&mut socket, &event).await.is_err() {
                    break;
                }
            }
            Ok(_) => {}
            Err(broadcast::error::RecvError::Lagged(_)) => {
                let event = EventMessage::Warning {
                    session_id: session_id.clone().unwrap_or_else(|| "unknown".to_string()),
                    code: ApiErrorCode::NetworkDegraded,
                    message: "Receiver event stream lagged behind.".to_string(),
                };
                if send_event(&mut socket, &event).await.is_err() {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

async fn send_event(socket: &mut WebSocket, event: &EventMessage) -> Result<(), axum::Error> {
    socket
        .send(Message::Text(
            serde_json::to_string(event)
                .expect("receiver event serialization should not fail")
                .into(),
        ))
        .await
}

fn stats_event(session: &ActiveSession) -> EventMessage {
    EventMessage::Stats {
        session_id: session.session_id.clone(),
        video_packets: 0,
        audio_packets: 0,
        video_packets_lost: 0,
        audio_packets_lost: 0,
        estimated_bitrate_kbps: 0,
        quality_preset: session.preset,
    }
}

fn spawn_stats_publisher(
    state: AppState,
    session_id: String,
    quality_preset: crate::protocol::QualityPreset,
) {
    tokio::spawn(async move {
        let mut last = MediaStatsSnapshot::default();
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let snapshot = {
                let inner = state.inner.lock().unwrap();
                if inner.session.active_session_id() != Some(session_id.as_str()) {
                    break;
                }
                inner.media.stats()
            };
            let byte_delta = snapshot
                .video_bytes
                .saturating_sub(last.video_bytes)
                .saturating_add(snapshot.audio_bytes.saturating_sub(last.audio_bytes));
            let estimated_bitrate_kbps = ((byte_delta * 8) / 1_000) as u32;
            state.publish(EventMessage::Stats {
                session_id: session_id.clone(),
                video_packets: snapshot.video_packets,
                audio_packets: snapshot.audio_packets,
                video_packets_lost: snapshot.video_malformed,
                audio_packets_lost: snapshot.audio_malformed,
                estimated_bitrate_kbps,
                quality_preset,
            });
            last = snapshot;
        }
    });
}

fn event_matches_session(event: &EventMessage, session_id: Option<&str>) -> bool {
    let Some(session_id) = session_id else {
        return true;
    };
    match event {
        EventMessage::Stats {
            session_id: event_session_id,
            ..
        }
        | EventMessage::Warning {
            session_id: event_session_id,
            ..
        }
        | EventMessage::Error {
            session_id: event_session_id,
            ..
        } => event_session_id == session_id,
    }
}

fn pairing_error_code(error: PairingError) -> ApiErrorCode {
    match error {
        PairingError::InvalidPin | PairingError::PinExpired | PairingError::TooManyAttempts => {
            ApiErrorCode::InvalidPin
        }
        PairingError::InvalidToken => ApiErrorCode::Unauthorized,
    }
}

fn session_error_response(error: SessionError) -> Response {
    api_error(
        ApiErrorCode::InvalidSessionState,
        &error.to_string(),
        StatusCode::CONFLICT,
    )
}

fn unix_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn api_error(code: ApiErrorCode, message: &str, status: StatusCode) -> Response {
    (
        status,
        Json(ApiErrorResponse {
            error: ApiErrorBody {
                code,
                message: message.to_string(),
            },
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use futures_util::StreamExt;
    use serde_json::json;
    use tokio::time::{Duration, timeout};
    use tokio_tungstenite::{connect_async, tungstenite};
    use tower::ServiceExt;

    use crate::{
        diagnostics::DependencyReport,
        media::{MediaError, MediaRuntime, MediaSessionSpec},
        protocol::{
            DependencyStatus, EventMessage, PairResponse, SessionStartResponse, StatusResponse,
        },
    };

    use super::*;

    fn ready_report() -> DependencyReport {
        DependencyReport {
            dependencies: vec![
                DependencyStatus {
                    name: "v4l2loopback".to_string(),
                    present: true,
                    detail: "ok".to_string(),
                },
                DependencyStatus {
                    name: "pipewire".to_string(),
                    present: true,
                    detail: "ok".to_string(),
                },
                DependencyStatus {
                    name: "gst-launch-1.0".to_string(),
                    present: true,
                    detail: "ok".to_string(),
                },
            ],
            virtual_camera_ready: true,
            virtual_microphone_ready: true,
        }
    }

    fn missing_gstreamer_report() -> DependencyReport {
        DependencyReport {
            dependencies: vec![
                DependencyStatus {
                    name: "v4l2loopback".to_string(),
                    present: true,
                    detail: "ok".to_string(),
                },
                DependencyStatus {
                    name: "pipewire".to_string(),
                    present: true,
                    detail: "ok".to_string(),
                },
                DependencyStatus {
                    name: "gst-launch-1.0".to_string(),
                    present: false,
                    detail: "missing".to_string(),
                },
            ],
            virtual_camera_ready: true,
            virtual_microphone_ready: false,
        }
    }

    struct FailingStartRuntime;

    impl MediaRuntime for FailingStartRuntime {
        fn start(&mut self, _spec: MediaSessionSpec) -> Result<(), MediaError> {
            Err(MediaError::GStreamer("fake launch failed".to_string()))
        }

        fn stop(&mut self) -> Result<(), MediaError> {
            Ok(())
        }

        fn active_session_id(&self) -> Option<String> {
            None
        }
    }

    async fn json_request(
        app: Router,
        method: &str,
        path: &str,
        body: serde_json::Value,
    ) -> Response {
        app.oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
    }

    async fn read_json<T: for<'de> serde::Deserialize<'de>>(response: Response) -> T {
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    async fn read_value(response: Response) -> serde_json::Value {
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    async fn next_event(receiver: &mut broadcast::Receiver<EventMessage>) -> EventMessage {
        timeout(Duration::from_secs(2), receiver.recv())
            .await
            .unwrap()
            .unwrap()
    }

    async fn spawn_server(state: AppState) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, router(state)).await.unwrap();
        });
        (addr, handle)
    }

    fn fixture_value(contents: &str) -> serde_json::Value {
        serde_json::from_str(contents).unwrap()
    }

    #[test]
    fn event_serialization_stays_fixture_compatible() {
        assert_eq!(
            serde_json::to_value(EventMessage::Stats {
                session_id: "sess_0123456789abcdef".to_string(),
                video_packets: 4200,
                audio_packets: 8400,
                video_packets_lost: 4,
                audio_packets_lost: 2,
                estimated_bitrate_kbps: 2400,
                quality_preset: crate::protocol::QualityPreset::Balanced,
            })
            .unwrap(),
            fixture_value(include_str!(
                "../../receiver/tests/fixtures/events.stats.json"
            ))
        );
    }

    #[tokio::test]
    async fn session_events_route_exists_and_rejects_plain_http_upgrade_miss() {
        let app = router(AppState::for_tests(
            ReceiverConfig::default(),
            "123456",
            ready_report(),
        ));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/session/events?session_token=wrong")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn websocket_session_events_rejects_invalid_token() {
        let state = AppState::for_tests(ReceiverConfig::default(), "123456", ready_report());
        let (addr, server) = spawn_server(state).await;
        let error = connect_async(format!("ws://{addr}/session/events?session_token=wrong"))
            .await
            .unwrap_err();
        match error {
            tungstenite::Error::Http(response) => {
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            }
            other => panic!("unexpected websocket error: {other}"),
        }
        server.abort();
    }

    #[tokio::test]
    async fn status_returns_stable_json() {
        let app = router(AppState::for_tests(
            ReceiverConfig::default(),
            "123456",
            ready_report(),
        ));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let status: StatusResponse = read_json(response).await;
        assert_eq!(status.receiver_name, "PocketLens Linux");
        assert!(status.virtual_devices.camera.ready);
    }

    #[tokio::test]
    async fn status_response_matches_ready_fixture() {
        let app = router(AppState::for_tests(
            ReceiverConfig::default(),
            "123456",
            ready_report(),
        ));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            read_value(response).await,
            fixture_value(include_str!(
                "../../receiver/tests/fixtures/receiver_status.ready.json"
            ))
        );
    }

    #[tokio::test]
    async fn pair_success_and_invalid_pin_match_fixtures() {
        let app = router(AppState::for_tests(
            ReceiverConfig::default(),
            "123456",
            ready_report(),
        ));
        let response = json_request(
            app.clone(),
            "POST",
            "/pair",
            fixture_value(include_str!(
                "../../receiver/tests/fixtures/pair.request.json"
            )),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            read_value(response).await,
            fixture_value(include_str!(
                "../../receiver/tests/fixtures/pair.success.json"
            ))
        );

        let app = router(AppState::for_tests(
            ReceiverConfig::default(),
            "123456",
            ready_report(),
        ));
        let response = json_request(
            app,
            "POST",
            "/pair",
            json!({"pin":"000000","device_name":"Pixel 9"}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            read_value(response).await,
            fixture_value(include_str!(
                "../../receiver/tests/fixtures/pair.invalid_pin.json"
            ))
        );
    }

    #[tokio::test]
    async fn session_start_and_stop_match_fixtures() {
        let app = router(AppState::for_tests(
            ReceiverConfig::default(),
            "123456",
            ready_report(),
        ));
        let response = json_request(
            app.clone(),
            "POST",
            "/pair",
            fixture_value(include_str!(
                "../../receiver/tests/fixtures/pair.request.json"
            )),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);

        let response = json_request(
            app.clone(),
            "POST",
            "/session/start",
            fixture_value(include_str!(
                "../../receiver/tests/fixtures/session_start.request.json"
            )),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            read_value(response).await,
            fixture_value(include_str!(
                "../../receiver/tests/fixtures/session_start.success.json"
            ))
        );

        let response = json_request(
            app,
            "POST",
            "/session/stop",
            fixture_value(include_str!(
                "../../receiver/tests/fixtures/session_stop.request.json"
            )),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            read_value(response).await,
            fixture_value(include_str!(
                "../../receiver/tests/fixtures/session_stop.success.json"
            ))
        );
    }

    #[tokio::test]
    async fn pair_start_and_stop_session() {
        let app = router(AppState::for_tests(
            ReceiverConfig::default(),
            "123456",
            ready_report(),
        ));
        let response = json_request(
            app.clone(),
            "POST",
            "/pair",
            json!({"pin":"123456","device_name":"Pixel"}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let pair: PairResponse = read_json(response).await;

        let response = json_request(
            app.clone(),
            "POST",
            "/session/start",
            json!({
                "session_token": pair.session_token,
                "quality_preset": "balanced",
                "video": {"codec":"h264","width":1280,"height":720,"fps":30},
                "audio": {"codec":"opus","sample_rate_hz":48000,"channels":1}
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let start: SessionStartResponse = read_json(response).await;
        assert_eq!(start.video_rtp_port, 5004);
        assert_eq!(start.audio_rtp_port, 5006);

        let response = json_request(
            app,
            "POST",
            "/session/stop",
            json!({"session_token":pair.session_token,"session_id":start.session_id}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn session_start_rejects_missing_media_dependencies() {
        let app = router(AppState::for_tests(
            ReceiverConfig::default(),
            "123456",
            missing_gstreamer_report(),
        ));
        let response = json_request(
            app.clone(),
            "POST",
            "/pair",
            json!({"pin":"123456","device_name":"Pixel"}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let pair: PairResponse = read_json(response).await;

        let response = json_request(
            app,
            "POST",
            "/session/start",
            json!({
                "session_token": pair.session_token,
                "quality_preset": "balanced",
                "video": {"codec":"h264","width":1280,"height":720,"fps":30},
                "audio": {"codec":"opus","sample_rate_hz":48000,"channels":1}
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            read_value(response).await,
            json!({"error":{"code":"missing_dependencies","message":"virtual camera or microphone dependencies are missing"}})
        );
    }

    #[tokio::test]
    async fn session_status_reflects_media_lifecycle() {
        let app = router(AppState::for_tests(
            ReceiverConfig::default(),
            "123456",
            ready_report(),
        ));
        let response = json_request(
            app.clone(),
            "POST",
            "/pair",
            json!({"pin":"123456","device_name":"Pixel"}),
        )
        .await;
        let pair: PairResponse = read_json(response).await;

        let response = json_request(
            app.clone(),
            "POST",
            "/session/start",
            json!({
                "session_token": pair.session_token,
                "quality_preset": "balanced",
                "video": {"codec":"h264","width":1280,"height":720,"fps":30},
                "audio": {"codec":"opus","sample_rate_hz":48000,"channels":1}
            }),
        )
        .await;
        let start: SessionStartResponse = read_json(response).await;

        let status_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status: StatusResponse = read_json(status_response).await;
        assert!(status.active_session);

        let response = json_request(
            app.clone(),
            "POST",
            "/session/stop",
            json!({"session_token":pair.session_token,"session_id":start.session_id}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);

        let status_response = app
            .oneshot(
                Request::builder()
                    .uri("/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status: StatusResponse = read_json(status_response).await;
        assert!(!status.active_session);
    }

    #[tokio::test]
    async fn session_lifecycle_publishes_typed_events() {
        let state = AppState::for_tests(ReceiverConfig::default(), "123456", ready_report());
        let mut events = state.events.subscribe();
        let app = router(state);
        let response = json_request(
            app.clone(),
            "POST",
            "/pair",
            json!({"pin":"123456","device_name":"Pixel"}),
        )
        .await;
        let pair: PairResponse = read_json(response).await;

        let response = json_request(
            app.clone(),
            "POST",
            "/session/start",
            json!({
                "session_token": pair.session_token,
                "quality_preset": "balanced",
                "video": {"codec":"h264","width":1280,"height":720,"fps":30},
                "audio": {"codec":"opus","sample_rate_hz":48000,"channels":1}
            }),
        )
        .await;
        let start: SessionStartResponse = read_json(response).await;
        assert_eq!(
            next_event(&mut events).await,
            EventMessage::Stats {
                session_id: start.session_id.clone(),
                video_packets: 0,
                audio_packets: 0,
                video_packets_lost: 0,
                audio_packets_lost: 0,
                estimated_bitrate_kbps: 0,
                quality_preset: crate::protocol::QualityPreset::Balanced,
            }
        );

        let response = json_request(
            app,
            "POST",
            "/session/stop",
            json!({"session_token":pair.session_token,"session_id":start.session_id}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            next_event(&mut events).await,
            EventMessage::Warning {
                session_id: "sess_0123456789abcdef".to_string(),
                code: ApiErrorCode::BadRequest,
                message: "Media session stopped.".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn websocket_session_events_streams_initial_stats() {
        let state = AppState::for_tests(ReceiverConfig::default(), "123456", ready_report());
        let app = router(state.clone());
        let response = json_request(
            app.clone(),
            "POST",
            "/pair",
            json!({"pin":"123456","device_name":"Pixel"}),
        )
        .await;
        let pair: PairResponse = read_json(response).await;

        let response = json_request(
            app,
            "POST",
            "/session/start",
            json!({
                "session_token": pair.session_token,
                "quality_preset": "balanced",
                "video": {"codec":"h264","width":1280,"height":720,"fps":30},
                "audio": {"codec":"opus","sample_rate_hz":48000,"channels":1}
            }),
        )
        .await;
        let start: SessionStartResponse = read_json(response).await;

        let (addr, server) = spawn_server(state).await;
        let url = format!(
            "ws://{addr}/session/events?session_token={}&session_id={}",
            pair.session_token, start.session_id
        );
        let (mut stream, _) = connect_async(url).await.unwrap();
        let message = timeout(Duration::from_secs(2), stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let text = message.into_text().unwrap();
        let event: EventMessage = serde_json::from_str(&text).unwrap();
        assert_eq!(
            event,
            EventMessage::Stats {
                session_id: start.session_id,
                video_packets: 0,
                audio_packets: 0,
                video_packets_lost: 0,
                audio_packets_lost: 0,
                estimated_bitrate_kbps: 0,
                quality_preset: crate::protocol::QualityPreset::Balanced,
            }
        );
        server.abort();
    }

    #[tokio::test]
    async fn session_start_maps_media_launch_failure_to_api_error() {
        let app = router(AppState::for_tests_with_media(
            ReceiverConfig::default(),
            "123456",
            ready_report(),
            Box::new(FailingStartRuntime),
        ));
        let response = json_request(
            app.clone(),
            "POST",
            "/pair",
            json!({"pin":"123456","device_name":"Pixel"}),
        )
        .await;
        let pair: PairResponse = read_json(response).await;

        let response = json_request(
            app,
            "POST",
            "/session/start",
            json!({
                "session_token": pair.session_token,
                "quality_preset": "balanced",
                "video": {"codec":"h264","width":1280,"height":720,"fps":30},
                "audio": {"codec":"opus","sample_rate_hz":48000,"channels":1}
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            read_value(response).await,
            json!({"error":{"code":"media_pipeline_failed","message":"failed to start media pipeline: GStreamer pipeline failed: fake launch failed"}})
        );
    }

    #[tokio::test]
    async fn session_apis_reject_invalid_tokens() {
        let app = router(AppState::for_tests(
            ReceiverConfig::default(),
            "123456",
            ready_report(),
        ));
        let response = json_request(
            app,
            "POST",
            "/session/start",
            json!({
                "session_token":"wrong",
                "quality_preset":"balanced",
                "video": {"codec":"h264","width":1280,"height":720,"fps":30},
                "audio": {"codec":"opus","sample_rate_hz":48000,"channels":1}
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
