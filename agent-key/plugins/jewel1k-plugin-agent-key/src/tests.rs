//! Integration tests: plugin + MockTransport + localhost API, no hardware
//! and no webview (tauri MockRuntime). These cover the MVP acceptance
//! criteria end-to-end.

use crate::server::{self, ServerConfig};
use crate::AgentKey;
use agent_key_core::types::{AgentState, Decision, LedPattern, RiskLevel};
use agent_key_core::{ApprovalRequest, StatusUpdate};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;
use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri::Manager;

type MockApp = tauri::App<tauri::test::MockRuntime>;

fn build_app() -> MockApp {
    mock_builder()
        .plugin(crate::init())
        .build(mock_context(noop_assets()))
        .expect("failed to build mock app")
}

fn core(app: &MockApp) -> Arc<crate::ManagerCore> {
    app.state::<AgentKey>().0.clone()
}

/// Bare-bones HTTP helper mirroring what the CLI does.
fn http(port: u16, method: &str, path: &str, body: &Value) -> (u16, Value) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    let payload = body.to_string();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
        payload.len(),
        payload
    )
    .unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    let text = String::from_utf8_lossy(&raw);
    let (head, body_text) = text.split_once("\r\n\r\n").expect("http response");
    let status: u16 = head
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    (status, serde_json::from_str(body_text.trim()).unwrap_or(Value::Null))
}

fn spawn_api(app: &MockApp) -> u16 {
    server::spawn(
        core(app),
        ServerConfig {
            port: 0, // ephemeral, avoids clashes with the default 43117
            token: None,
        },
    )
    .expect("spawn API server")
}

#[test]
fn set_status_thinking_reaches_mock_transport() {
    let app = build_app();
    let core = core(&app);

    // MVP #1/#2: setStatus({state:"thinking"}) works and the mock transport
    // logs/records the LED packet.
    core.set_status(StatusUpdate {
        state: AgentState::Thinking,
        risk: None,
        message: None,
    })
    .unwrap();

    let packet = core.last_sent_packet().expect("packet sent to mock");
    assert_eq!(packet.state, AgentState::Thinking);
    assert_eq!(packet.pattern, LedPattern::Breath);
    assert!(packet.brightness > 0);

    let state = core.get_current_state();
    assert_eq!(state.state, AgentState::Thinking);
    assert!(state.connected);
}

#[test]
fn medium_risk_approved_by_simulated_click_over_http() {
    let app = build_app();
    let port = spawn_api(&app);

    // MVP #8: request an approval through the localhost API...
    let handle = std::thread::spawn(move || {
        http(
            port,
            "POST",
            "/approval",
            &json!({ "title": "write file", "risk": "medium", "timeout_ms": 10000 }),
        )
    });

    // ...MVP #3/#4: a simulated single click approves medium risk.
    std::thread::sleep(Duration::from_millis(300));
    let (status, _) = http(port, "POST", "/simulate", &json!({ "gesture": "single" }));
    assert_eq!(status, 200);

    let (status, body) = handle.join().unwrap();
    assert_eq!(status, 200);
    assert_eq!(body["decision"], "approved");
}

#[test]
fn high_risk_needs_two_clicks_over_http() {
    let app = build_app();
    let port = spawn_api(&app);

    let handle = std::thread::spawn(move || {
        http(
            port,
            "POST",
            "/approval",
            &json!({ "title": "git push --force", "risk": "high", "timeout_ms": 10000 }),
        )
    });

    std::thread::sleep(Duration::from_millis(300));
    // MVP #5: first click is not enough...
    http(port, "POST", "/simulate", &json!({ "gesture": "single" }));
    std::thread::sleep(Duration::from_millis(200));
    let (_, state) = http(port, "GET", "/state", &Value::Null);
    assert_eq!(state["state"], "needs_approval", "still pending after 1 click");
    // ...the second click approves.
    http(port, "POST", "/simulate", &json!({ "gesture": "single" }));

    let (status, body) = handle.join().unwrap();
    assert_eq!(status, 200);
    assert_eq!(body["decision"], "approved");
}

#[test]
fn critical_risk_is_denied_without_button() {
    let app = build_app();
    let port = spawn_api(&app);

    // MVP #6: critical risk resolves immediately as denied.
    let (status, body) = http(
        port,
        "POST",
        "/approval",
        &json!({ "title": "rm -rf /", "risk": "critical" }),
    );
    assert_eq!(status, 200);
    assert_eq!(body["decision"], "denied");
}

#[test]
fn long_press_denies_and_led_returns_to_previous_state() {
    let app = build_app();
    let core_ = core(&app);
    let port = spawn_api(&app);

    core_
        .set_status(StatusUpdate {
            state: AgentState::ToolRunning,
            risk: None,
            message: None,
        })
        .unwrap();

    let handle = std::thread::spawn(move || {
        http(
            port,
            "POST",
            "/approval",
            &json!({ "title": "edit prod config", "risk": "medium", "timeout_ms": 10000 }),
        )
    });
    std::thread::sleep(Duration::from_millis(300));

    // While pending, the LED must show needs_approval.
    let packet = core_.last_sent_packet().unwrap();
    assert_eq!(packet.state, AgentState::NeedsApproval);

    http(port, "POST", "/simulate", &json!({ "gesture": "long" }));
    let (_, body) = handle.join().unwrap();
    assert_eq!(body["decision"], "denied");

    // Queue drained -> the LED resumes the agent's own status.
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(
        core_.last_sent_packet().unwrap().state,
        AgentState::ToolRunning
    );
}

#[test]
fn status_and_health_roundtrip_over_http() {
    let app = build_app();
    let port = spawn_api(&app);

    let (status, body) = http(
        port,
        "POST",
        "/status",
        &json!({ "state": "thinking", "risk": "low" }),
    );
    assert_eq!(status, 200);
    assert_eq!(body["state"], "thinking");
    assert_eq!(body["risk"], "low");

    let (status, health) = http(port, "GET", "/health", &Value::Null);
    assert_eq!(status, 200);
    assert_eq!(health["connected"], true);
    assert_eq!(health["device"]["transport"], "mock");

    let (status, brightness) = http(port, "POST", "/brightness", &json!({ "value": 42 }));
    assert_eq!(status, 200);
    assert_eq!(brightness["brightness"], 42);
}

#[test]
fn multiple_devices_share_led_broadcast_and_approval_queue() {
    let app = build_app();
    let core_ = core(&app);
    let port = spawn_api(&app);

    // The plugin auto-connected "mock"; attach a second mock device.
    core_
        .connect(crate::ConnectOptions {
            transport: Some("mock".into()),
            port: Some("mock2".into()),
        })
        .unwrap();
    let health = core_.get_health();
    assert_eq!(health.devices.len(), 2);
    assert!(health.connected);

    // LED updates are broadcast to every connected device.
    core_
        .set_status(StatusUpdate {
            state: AgentState::Thinking,
            risk: None,
            message: None,
        })
        .unwrap();
    for id in ["mock", "mock2"] {
        let packet = core_.last_sent_packet_for(id).expect("packet on each device");
        assert_eq!(packet.state, AgentState::Thinking);
    }

    // A button event from any device resolves the shared queue.
    let handle = std::thread::spawn(move || {
        http(
            port,
            "POST",
            "/approval",
            &json!({ "title": "multi-device", "risk": "medium", "timeout_ms": 10000 }),
        )
    });
    std::thread::sleep(Duration::from_millis(300));
    http(port, "POST", "/simulate", &json!({ "gesture": "single" }));
    let (status, body) = handle.join().unwrap();
    assert_eq!(status, 200);
    assert_eq!(body["decision"], "approved");

    // Disconnecting one device by id keeps the other link alive.
    let (status, _) = http(port, "POST", "/disconnect", &json!({ "id": "mock2" }));
    assert_eq!(status, 200);
    let health = core_.get_health();
    assert_eq!(health.devices.len(), 1);
    assert_eq!(health.devices[0].id, "mock");
    assert!(health.connected);
}

#[test]
fn agent_status_supersedes_orphaned_approval() {
    let app = build_app();
    let core = core(&app);

    // Auto-accept mode: a gate is requested but nobody presses the button.
    let outcome = core
        .request_approval(ApprovalRequest {
            id: String::new(),
            title: "git push".into(),
            description: None,
            risk: RiskLevel::High,
            timeout_ms: Some(120_000),
            source: Some("claude-code".into()),
        })
        .unwrap();
    let id = match outcome {
        agent_key_core::SubmitOutcome::Pending { id } => id,
        other => panic!("expected pending, got {other:?}"),
    };
    // While pending the LED shows needs_approval (red).
    assert_eq!(
        core.last_sent_packet().unwrap().state,
        AgentState::NeedsApproval
    );

    // The agent moves on and reports its own progress. The orphaned approval
    // must be superseded so the LED does not stay red forever.
    core.set_status(StatusUpdate {
        state: AgentState::Thinking,
        risk: None,
        message: None,
    })
    .unwrap();

    assert_eq!(core.last_sent_packet().unwrap().state, AgentState::Thinking);
    let state = core.get_current_state();
    assert_eq!(state.state, AgentState::Thinking);
    assert!(state.pending_approval.is_none(), "queue must be drained");
    // Superseded == Cancelled, never Approved (safety invariant).
    assert_eq!(
        core.get_resolution(&id).map(|r| r.decision),
        Some(Decision::Cancelled)
    );
}

#[test]
fn needs_approval_status_does_not_pin_the_resume_state() {
    let app = build_app();
    let core = core(&app);

    // Hook order: the agent is thinking, then a hook flips the LED red via a
    // needs_approval status just before submitting a critical request.
    core.set_status(StatusUpdate {
        state: AgentState::Thinking,
        risk: None,
        message: None,
    })
    .unwrap();
    core.set_status(StatusUpdate {
        state: AgentState::NeedsApproval,
        risk: Some(RiskLevel::Critical),
        message: None,
    })
    .unwrap();
    assert_eq!(
        core.last_sent_packet().unwrap().state,
        AgentState::NeedsApproval
    );

    // Critical resolves immediately as denied; the LED must return to the
    // agent's own state (thinking), not stay stuck on needs_approval.
    let outcome = core
        .request_approval(ApprovalRequest {
            id: String::new(),
            title: "rm -rf /".into(),
            description: None,
            risk: RiskLevel::Critical,
            timeout_ms: None,
            source: None,
        })
        .unwrap();
    assert!(matches!(
        outcome,
        agent_key_core::SubmitOutcome::Resolved(_)
    ));
    assert_eq!(core.last_sent_packet().unwrap().state, AgentState::Thinking);
}

#[test]
fn brightness_command_scales_led_output() {
    let app = build_app();
    let core = core(&app);

    core.set_status(StatusUpdate {
        state: AgentState::ToolRunning,
        risk: None,
        message: None,
    })
    .unwrap();
    let full = core.last_sent_packet().unwrap().brightness;
    core.set_brightness(64).unwrap();
    let dimmed = core.last_sent_packet().unwrap().brightness;
    assert!(dimmed < full, "expected {dimmed} < {full}");
    assert_eq!(RiskLevel::None as u8, 0); // wire byte stability guard
}
