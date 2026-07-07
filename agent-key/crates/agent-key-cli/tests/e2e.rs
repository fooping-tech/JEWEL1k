//! End-to-end tests for the `agent-key` CLI binary: spawn the real
//! executable against a scripted localhost API server and assert on the
//! requests it makes, its stdout and its exit codes.
//!
//! (The server side of the contract is covered by the plugin's integration
//! tests; here the CLI process itself is the unit under test.)

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
struct Recorded {
    method: String,
    path: String,
    body: Value,
    token: Option<String>,
}

type Handler = dyn Fn(&Recorded) -> (u16, Value) + Send + Sync;

/// Spawn a throwaway HTTP server on an ephemeral port. Every request is
/// recorded and answered by `handler`. The thread dies with the test process.
fn spawn_server(handler: Box<Handler>) -> (u16, Arc<Mutex<Vec<Recorded>>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let port = listener.local_addr().unwrap().port();
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let recorded_ = recorded.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            let mut parts = line.split_whitespace();
            let method = parts.next().unwrap_or("").to_string();
            let path = parts.next().unwrap_or("/").to_string();
            let mut content_length = 0usize;
            let mut token = None;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).is_err() {
                    break;
                }
                let header = header.trim();
                if header.is_empty() {
                    break;
                }
                if let Some((name, value)) = header.split_once(':') {
                    let name = name.trim().to_ascii_lowercase();
                    if name == "content-length" {
                        content_length = value.trim().parse().unwrap_or(0);
                    } else if name == "x-agent-key-token" {
                        token = Some(value.trim().to_string());
                    }
                }
            }
            let mut body_bytes = vec![0u8; content_length];
            if content_length > 0 {
                let _ = reader.read_exact(&mut body_bytes);
            }
            let req = Recorded {
                method,
                path,
                body: serde_json::from_slice(&body_bytes).unwrap_or(Value::Null),
                token,
            };
            let (status, body) = handler(&req);
            recorded_.lock().unwrap().push(req);
            let text = body.to_string();
            let response = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                text.len(),
                text
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.shutdown(std::net::Shutdown::Write);
            // Wait for the client to finish reading and close its side;
            // dropping the socket earlier can turn into a RST on Windows
            // loopback and the client loses the buffered response.
            let mut sink = [0u8; 64];
            while matches!(reader.read(&mut sink), Ok(n) if n > 0) {}
        }
    });
    (port, recorded)
}

fn cli(port: u16) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_agent-key"));
    cmd.env("AGENT_KEY_PORT", port.to_string())
        .env_remove("AGENT_KEY_TOKEN")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

fn run(cmd: &mut Command) -> (i32, String, String) {
    let out = cmd.output().expect("run agent-key CLI");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn status_command_posts_state_and_exits_zero() {
    let (port, recorded) = spawn_server(Box::new(|req| match req.path.as_str() {
        "/status" => (200, json!({ "state": "thinking", "risk": "low" })),
        _ => (404, json!({ "error": "not found" })),
    }));

    let (code, stdout, _) = run(cli(port).args(["status", "thinking", "--risk", "low"]));
    assert_eq!(code, 0, "stdout: {stdout}");
    assert!(stdout.contains("thinking"));

    let recorded = recorded.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].method, "POST");
    assert_eq!(recorded[0].path, "/status");
    assert_eq!(recorded[0].body["state"], "thinking");
    assert_eq!(recorded[0].body["risk"], "low");
}

#[test]
fn approval_exit_codes_follow_the_decision() {
    for (decision, expected_code) in [
        ("approved", 0),
        ("denied", 2),
        ("emergency_stopped", 2),
        ("timed_out", 3),
    ] {
        let decision_owned = decision.to_string();
        let (port, recorded) = spawn_server(Box::new(move |_req| {
            (200, json!({ "id": "a1", "decision": decision_owned }))
        }));
        let (code, stdout, stderr) = run(cli(port).args(["approval", "test action", "--risk", "high"]));
        assert_eq!(code, expected_code, "decision={decision} stdout={stdout} stderr={stderr}");
        assert!(stdout.contains(decision));

        let recorded = recorded.lock().unwrap();
        assert_eq!(recorded[0].path, "/approval");
        assert_eq!(recorded[0].body["title"], "test action");
        assert_eq!(recorded[0].body["risk"], "high");
    }
}

#[test]
fn approval_unresolved_maps_to_exit_3() {
    let (port, _) = spawn_server(Box::new(|_req| {
        (504, json!({ "status": "unresolved", "id": "a1" }))
    }));
    let (code, _, _) = run(cli(port).args(["approval", "x"]));
    assert_eq!(code, 3);
}

#[test]
fn token_flag_is_forwarded_as_header() {
    let (port, recorded) = spawn_server(Box::new(|_req| (200, json!({ "connected": true }))));
    let (code, _, _) = run(cli(port).args(["--token", "s3cret", "health"]));
    assert_eq!(code, 0);
    assert_eq!(recorded.lock().unwrap()[0].token.as_deref(), Some("s3cret"));
}

#[test]
fn unreachable_api_exits_one_with_hint() {
    // Port 1 (tcpmux) is privileged/never served on localhost. Deliberately
    // NOT a bind-then-drop ephemeral port: the OS may hand that port to a
    // concurrently running test's server, which would then receive this
    // test's request.
    let (code, _, stderr) = run(cli(1).arg("health"));
    assert_eq!(code, 1);
    assert!(stderr.contains("cannot reach agent-key API"), "stderr: {stderr}");
}

#[test]
fn hook_pre_tool_blocks_denied_tool_with_exit_2() {
    let (port, recorded) = spawn_server(Box::new(|req| match req.path.as_str() {
        "/status" => (200, json!({ "state": "needs_approval" })),
        "/approval" => (200, json!({ "id": "a1", "decision": "denied" })),
        _ => (404, json!({ "error": "not found" })),
    }));

    let mut cmd = cli(port);
    cmd.args(["hook", "pre-tool", "--risk", "high"]).stdin(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"tool_name":"Bash","tool_input":{"command":"rm -rf /"}}"#)
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("Blocked by physical key"));

    let recorded = recorded.lock().unwrap();
    assert_eq!(recorded[0].path, "/status");
    assert_eq!(recorded[0].body["state"], "needs_approval");
    assert_eq!(recorded[1].path, "/approval");
    assert_eq!(recorded[1].body["title"], "Bash");
    assert_eq!(recorded[1].body["source"], "claude-code");
}

#[test]
fn hook_pre_tool_json_emits_permission_decision_and_exits_zero() {
    for (decision, expected_permission) in [("approved", "allow"), ("denied", "deny"), ("timed_out", "deny")] {
        let decision_owned = decision.to_string();
        let (port, _) = spawn_server(Box::new(move |req| match req.path.as_str() {
            "/status" => (200, json!({})),
            "/approval" => (200, json!({ "id": "a1", "decision": decision_owned })),
            _ => (404, json!({ "error": "not found" })),
        }));

        let mut cmd = cli(port);
        cmd.args(["hook", "pre-tool", "--json"]).stdin(Stdio::piped());
        let mut child = cmd.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(br#"{"tool_name":"Write","tool_input":{}}"#)
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert_eq!(out.status.code(), Some(0), "decision={decision}");

        let stdout = String::from_utf8_lossy(&out.stdout);
        let parsed: Value = serde_json::from_str(stdout.trim()).expect("hookOutput is JSON");
        let hso = &parsed["hookSpecificOutput"];
        assert_eq!(hso["hookEventName"], "PreToolUse");
        assert_eq!(hso["permissionDecision"], expected_permission, "decision={decision}");
        assert!(hso["permissionDecisionReason"].as_str().unwrap().contains("JEWEL1k"));
    }
}

#[test]
fn hook_pre_tool_skips_gate_in_auto_mode() {
    for mode in ["auto", "dontAsk", "bypassPermissions"] {
        let (port, recorded) = spawn_server(Box::new(|req| match req.path.as_str() {
            "/status" => (200, json!({})),
            _ => (404, json!({ "error": "not found" })),
        }));

        let mut cmd = cli(port);
        cmd.args(["hook", "pre-tool", "--risk", "high", "--json"])
            .stdin(Stdio::piped());
        let mut child = cmd.spawn().unwrap();
        let payload = format!(
            r#"{{"tool_name":"Bash","tool_input":{{}},"permission_mode":"{mode}"}}"#
        );
        child.stdin.take().unwrap().write_all(payload.as_bytes()).unwrap();
        let out = child.wait_with_output().unwrap();
        assert_eq!(out.status.code(), Some(0), "mode={mode}");

        let stdout = String::from_utf8_lossy(&out.stdout);
        let parsed: Value = serde_json::from_str(stdout.trim()).expect("hookOutput is JSON");
        assert_eq!(
            parsed["hookSpecificOutput"]["permissionDecision"], "allow",
            "mode={mode}"
        );

        // No approval request may be made; only a tool_running status.
        let recorded = recorded.lock().unwrap();
        assert!(
            recorded.iter().all(|r| r.path != "/approval"),
            "mode={mode}: /approval must not be called"
        );
        assert_eq!(recorded[0].path, "/status");
        assert_eq!(recorded[0].body["state"], "tool_running");
    }
}

#[test]
fn hook_pre_tool_always_gate_overrides_auto_mode() {
    let (port, recorded) = spawn_server(Box::new(|req| match req.path.as_str() {
        "/status" => (200, json!({})),
        "/approval" => (200, json!({ "id": "a1", "decision": "approved" })),
        _ => (404, json!({ "error": "not found" })),
    }));

    let mut cmd = cli(port);
    cmd.args(["hook", "pre-tool", "--risk", "high", "--always-gate"])
        .stdin(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"tool_name":"Bash","tool_input":{},"permission_mode":"bypassPermissions"}"#)
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0));

    let recorded = recorded.lock().unwrap();
    assert!(
        recorded.iter().any(|r| r.path == "/approval"),
        "--always-gate must still request approval"
    );
}

#[test]
fn hook_pre_tool_auto_mode_still_gates_critical() {
    let (port, recorded) = spawn_server(Box::new(|req| match req.path.as_str() {
        "/status" => (200, json!({})),
        "/approval" => (200, json!({ "id": "a1", "decision": "denied" })),
        _ => (404, json!({ "error": "not found" })),
    }));

    let mut cmd = cli(port);
    cmd.args(["hook", "pre-tool", "--risk", "critical"]).stdin(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"tool_name":"Bash","tool_input":{},"permission_mode":"auto"}"#)
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(2), "critical must stay denied in auto mode");

    let recorded = recorded.lock().unwrap();
    assert!(recorded.iter().any(|r| r.path == "/approval"));
}

#[test]
fn hook_codex_notify_maps_turn_complete_to_done() {
    let (port, recorded) = spawn_server(Box::new(|_req| (200, json!({ "state": "done" }))));
    let (code, _, _) = run(cli(port).args([
        "hook",
        "codex-notify",
        r#"{"type":"agent-turn-complete","turn-id":"t1"}"#,
    ]));
    assert_eq!(code, 0);
    let recorded = recorded.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].path, "/status");
    assert_eq!(recorded[0].body["state"], "done");
}

#[test]
fn hook_codex_notify_ignores_unknown_event_types() {
    let (port, recorded) = spawn_server(Box::new(|_req| (200, json!({}))));
    let (code, _, _) = run(cli(port).args([
        "hook",
        "codex-notify",
        r#"{"type":"something-new"}"#,
    ]));
    assert_eq!(code, 0);
    assert!(recorded.lock().unwrap().is_empty(), "no API call expected");
}
