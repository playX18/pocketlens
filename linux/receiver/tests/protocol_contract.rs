use acamera_receiver::protocol::{
    ApiErrorResponse, EventMessage, PairRequest, PairResponse, SessionStartRequest,
    SessionStartResponse, SessionStopRequest, SessionStopResponse, StatusResponse,
};

#[test]
fn linux_fixture_copies_match_root_contract_fixtures() {
    let fixtures = [
        (
            include_str!("../../../contracts/fixtures/events.error.json"),
            include_str!("fixtures/events.error.json"),
        ),
        (
            include_str!("../../../contracts/fixtures/events.stats.json"),
            include_str!("fixtures/events.stats.json"),
        ),
        (
            include_str!("../../../contracts/fixtures/events.warning.json"),
            include_str!("fixtures/events.warning.json"),
        ),
        (
            include_str!("../../../contracts/fixtures/pair.invalid_pin.json"),
            include_str!("fixtures/pair.invalid_pin.json"),
        ),
        (
            include_str!("../../../contracts/fixtures/pair.request.json"),
            include_str!("fixtures/pair.request.json"),
        ),
        (
            include_str!("../../../contracts/fixtures/pair.success.json"),
            include_str!("fixtures/pair.success.json"),
        ),
        (
            include_str!("../../../contracts/fixtures/receiver_status.missing_dependencies.json"),
            include_str!("fixtures/receiver_status.missing_dependencies.json"),
        ),
        (
            include_str!("../../../contracts/fixtures/receiver_status.ready.json"),
            include_str!("fixtures/receiver_status.ready.json"),
        ),
        (
            include_str!("../../../contracts/fixtures/session_start.request.json"),
            include_str!("fixtures/session_start.request.json"),
        ),
        (
            include_str!("../../../contracts/fixtures/session_start.success.json"),
            include_str!("fixtures/session_start.success.json"),
        ),
        (
            include_str!("../../../contracts/fixtures/session_stop.request.json"),
            include_str!("fixtures/session_stop.request.json"),
        ),
        (
            include_str!("../../../contracts/fixtures/session_stop.success.json"),
            include_str!("fixtures/session_stop.success.json"),
        ),
    ];

    for (root, local) in fixtures {
        let root: serde_json::Value = serde_json::from_str(root).unwrap();
        let local: serde_json::Value = serde_json::from_str(local).unwrap();
        assert_eq!(local, root);
    }
}

#[test]
fn required_fixture_files_are_strict_parseable_contract_examples() {
    assert_round_trips::<StatusResponse>(include_str!("fixtures/receiver_status.ready.json"));
    assert_round_trips::<StatusResponse>(include_str!(
        "fixtures/receiver_status.missing_dependencies.json"
    ));
    assert_round_trips::<PairRequest>(include_str!("fixtures/pair.request.json"));
    assert_round_trips::<PairResponse>(include_str!("fixtures/pair.success.json"));
    assert_round_trips::<ApiErrorResponse>(include_str!("fixtures/pair.invalid_pin.json"));
    assert_round_trips::<SessionStartRequest>(include_str!("fixtures/session_start.request.json"));
    assert_round_trips::<SessionStartResponse>(include_str!("fixtures/session_start.success.json"));
    assert_round_trips::<SessionStopRequest>(include_str!("fixtures/session_stop.request.json"));
    assert_round_trips::<SessionStopResponse>(include_str!("fixtures/session_stop.success.json"));
    assert_round_trips::<EventMessage>(include_str!("fixtures/events.stats.json"));
    assert_round_trips::<EventMessage>(include_str!("fixtures/events.warning.json"));
    assert_round_trips::<EventMessage>(include_str!("fixtures/events.error.json"));
}

fn assert_round_trips<T>(json: &str)
where
    T: for<'de> serde::Deserialize<'de> + serde::Serialize,
{
    let original: serde_json::Value = serde_json::from_str(json).unwrap();
    let parsed: T = serde_json::from_str(json).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), original);
}
