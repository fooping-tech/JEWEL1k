//! agent-key: CLI for the JEWEL1k agent-key localhost API.
//!
//! Exit codes (approval-related commands):
//!   0 approved / success
//!   1 usage or connection error
//!   2 denied or emergency-stopped
//!   3 timed out, cancelled or unresolved
//!
//! Designed to sit directly in agent hooks:
//!   agent-key approval "git push --force" --risk high || exit 1

mod http;

use serde_json::{json, Value};
use std::io::Read;
use std::time::Duration;

const USAGE: &str = r#"agent-key — JEWEL1k status LED & physical approval button

USAGE:
  agent-key [--port N] [--token T] <command> [args]

COMMANDS:
  devices                       list connectable devices
  health                        show device health
  state                         show current state
  connect [mock|serial <PORT>]  connect a transport (default: mock)
  disconnect                    disconnect the current transport
  status <state> [--risk R] [--message M]
                                push agent status: idle|thinking|tool_running|
                                done|needs_approval|error|off
  brightness <0-255>            set master LED brightness
  approval <title> [--risk R] [--desc D] [--timeout MS] [--source S] [--no-wait]
                                request physical approval; blocks until the
                                button decides (exit 0=approved 2=denied 3=timeout)
  cancel <id>                   cancel a pending approval
  simulate <single|double|long|very_long>
                                inject a fake button gesture (mock only)
  hook pre-tool [--risk R]      Claude Code PreToolUse hook: reads the hook
                                JSON on stdin, asks for approval (exit 2 blocks)
  hook stop                     Claude Code Stop hook: sets status to done

ENVIRONMENT:
  AGENT_KEY_PORT   API port (default 43117)
  AGENT_KEY_TOKEN  shared token, if the plugin requires one

RISK LEVELS:
  none|low|medium: 1 click approves   high: 2 clicks within 5 s
  critical: always denied by policy   long press: deny   very long: emergency stop
"#;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(run(args));
}

fn run(mut args: Vec<String>) -> i32 {
    let mut port: u16 = std::env::var("AGENT_KEY_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(43117);
    let mut token = std::env::var("AGENT_KEY_TOKEN").ok();

    // global options
    while args.first().map(|a| a.starts_with("--")).unwrap_or(false) {
        match take_flag(&mut args) {
            Some(("--port", v)) => match v.parse() {
                Ok(p) => port = p,
                Err(_) => return usage_error("--port needs a number"),
            },
            Some(("--token", v)) => token = Some(v),
            Some(("--help", _)) => {
                println!("{USAGE}");
                return 0;
            }
            Some((other, _)) => return usage_error(&format!("unknown option {other}")),
            None => break,
        }
    }

    let Some(command) = args.first().cloned() else {
        println!("{USAGE}");
        return 1;
    };
    let mut rest: Vec<String> = args.drain(1..).collect();

    let client = |timeout: Duration| http::Client {
        port,
        token: token.clone(),
        timeout,
    };
    let quick = client(Duration::from_secs(10));

    match command.as_str() {
        "devices" => print_result(quick.request("GET", "/devices", None)),
        "health" => print_result(quick.request("GET", "/health", None)),
        "state" => print_result(quick.request("GET", "/state", None)),
        "disconnect" => print_result(quick.request("POST", "/disconnect", Some(&json!({})))),
        "connect" => {
            let transport = rest.first().cloned().unwrap_or_else(|| "mock".into());
            let body = if transport == "serial" {
                let Some(p) = rest.get(1) else {
                    return usage_error("connect serial <PORT>");
                };
                json!({ "transport": "serial", "port": p })
            } else {
                json!({ "transport": transport })
            };
            print_result(quick.request("POST", "/connect", Some(&body)))
        }
        "status" => {
            let Some(state) = rest.first().cloned() else {
                return usage_error("status <state>");
            };
            rest.remove(0);
            let mut body = json!({ "state": state });
            if let Some(r) = opt_value(&mut rest, "--risk") {
                body["risk"] = json!(r);
            }
            if let Some(m) = opt_value(&mut rest, "--message") {
                body["message"] = json!(m);
            }
            print_result(quick.request("POST", "/status", Some(&body)))
        }
        "brightness" => {
            let Some(v) = rest.first().and_then(|v| v.parse::<u8>().ok()) else {
                return usage_error("brightness <0-255>");
            };
            print_result(quick.request("POST", "/brightness", Some(&json!({ "value": v }))))
        }
        "simulate" => {
            let Some(g) = rest.first().cloned() else {
                return usage_error("simulate <single|double|long|very_long>");
            };
            print_result(quick.request("POST", "/simulate", Some(&json!({ "gesture": g }))))
        }
        "cancel" => {
            let Some(id) = rest.first().cloned() else {
                return usage_error("cancel <id>");
            };
            print_result(quick.request("POST", &format!("/approval/{id}/cancel"), Some(&json!({}))))
        }
        "approval" => {
            let Some(title) = rest.first().cloned() else {
                return usage_error("approval <title> [--risk R]");
            };
            rest.remove(0);
            let risk = opt_value(&mut rest, "--risk").unwrap_or_else(|| "medium".into());
            let timeout_ms: u64 = opt_value(&mut rest, "--timeout")
                .and_then(|t| t.parse().ok())
                .unwrap_or(60_000);
            let desc = opt_value(&mut rest, "--desc");
            let source = opt_value(&mut rest, "--source").unwrap_or_else(|| "cli".into());
            let no_wait = take_bool(&mut rest, "--no-wait");
            request_approval(
                &client(Duration::from_millis(timeout_ms + 10_000)),
                &title,
                &risk,
                desc.as_deref(),
                timeout_ms,
                &source,
                !no_wait,
            )
        }
        "hook" => hook(&rest, &client(Duration::from_secs(600)), &quick),
        _ => usage_error(&format!("unknown command `{command}`")),
    }
}

fn request_approval(
    client: &http::Client,
    title: &str,
    risk: &str,
    desc: Option<&str>,
    timeout_ms: u64,
    source: &str,
    wait: bool,
) -> i32 {
    let mut body = json!({
        "title": title,
        "risk": risk,
        "timeout_ms": timeout_ms,
        "source": source,
    });
    if let Some(d) = desc {
        body["description"] = json!(d);
    }
    let path = if wait { "/approval" } else { "/approval?wait=false" };
    match client.request("POST", path, Some(&body)) {
        Ok(res) => {
            println!("{}", res.body);
            if res.status == 202 {
                return 0; // queued, not waiting
            }
            decision_exit_code(&res.body)
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

fn decision_exit_code(body: &Value) -> i32 {
    match body.get("decision").and_then(Value::as_str) {
        Some("approved") => 0,
        Some("denied") | Some("emergency_stopped") => 2,
        Some("timed_out") | Some("cancelled") => 3,
        _ => 3, // unresolved / unknown
    }
}

/// `agent-key hook ...`: adapters for coding-agent hook systems.
fn hook(rest: &[String], approval_client: &http::Client, quick: &http::Client) -> i32 {
    match rest.first().map(String::as_str) {
        Some("pre-tool") => {
            let mut rest = rest[1..].to_vec();
            let risk = opt_value(&mut rest, "--risk").unwrap_or_else(|| "medium".into());
            let timeout_ms: u64 = opt_value(&mut rest, "--timeout")
                .and_then(|t| t.parse().ok())
                .unwrap_or(120_000);

            // Claude Code pipes the hook payload on stdin.
            let mut input = String::new();
            let _ = std::io::stdin().read_to_string(&mut input);
            let payload: Value = serde_json::from_str(&input).unwrap_or(Value::Null);
            let tool = payload
                .get("tool_name")
                .and_then(Value::as_str)
                .unwrap_or("tool");
            let detail = payload
                .get("tool_input")
                .map(|v| {
                    let s = v.to_string();
                    s.chars().take(200).collect::<String>()
                })
                .unwrap_or_default();

            let _ = quick.request(
                "POST",
                "/status",
                Some(&json!({ "state": "needs_approval", "risk": risk })),
            );
            let code = request_approval(
                approval_client,
                &format!("{tool}"),
                &risk,
                Some(&detail),
                timeout_ms,
                "claude-code",
                true,
            );
            if code == 2 {
                // exit 2 blocks the tool call in Claude Code; stderr is fed
                // back to the model.
                eprintln!("Blocked by physical key (JEWEL1k): the user denied this action.");
            }
            if code == 3 {
                eprintln!("No decision on the physical key before timeout; blocking.");
                return 2;
            }
            code
        }
        Some("stop") => {
            let res = quick.request("POST", "/status", Some(&json!({ "state": "done" })));
            match res {
                Ok(_) => 0,
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
        _ => usage_error("hook <pre-tool|stop>"),
    }
}

// ---- tiny arg helpers -------------------------------------------------

fn take_flag(args: &mut Vec<String>) -> Option<(&'static str, String)> {
    let flag = args.first()?.clone();
    for known in ["--port", "--token"] {
        if flag == known {
            args.remove(0);
            if args.is_empty() {
                return Some((known, String::new()));
            }
            return Some((known, args.remove(0)));
        }
    }
    if flag == "--help" || flag == "-h" {
        args.remove(0);
        return Some(("--help", String::new()));
    }
    Some(("--unknown", flag))
}

fn opt_value(args: &mut Vec<String>, name: &str) -> Option<String> {
    let idx = args.iter().position(|a| a == name)?;
    args.remove(idx);
    if idx < args.len() {
        Some(args.remove(idx))
    } else {
        None
    }
}

fn take_bool(args: &mut Vec<String>, name: &str) -> bool {
    if let Some(idx) = args.iter().position(|a| a == name) {
        args.remove(idx);
        true
    } else {
        false
    }
}

fn usage_error(msg: &str) -> i32 {
    eprintln!("error: {msg}\n\n{USAGE}");
    1
}

fn print_result(res: Result<http::Response, String>) -> i32 {
    match res {
        Ok(r) => {
            println!("{}", r.body);
            if (200..300).contains(&r.status) {
                0
            } else {
                1
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_maps_to_exit_codes() {
        assert_eq!(decision_exit_code(&json!({ "decision": "approved" })), 0);
        assert_eq!(decision_exit_code(&json!({ "decision": "denied" })), 2);
        assert_eq!(
            decision_exit_code(&json!({ "decision": "emergency_stopped" })),
            2
        );
        assert_eq!(decision_exit_code(&json!({ "decision": "timed_out" })), 3);
        assert_eq!(decision_exit_code(&json!({ "status": "unresolved" })), 3);
    }

    #[test]
    fn opt_value_extracts_flags() {
        let mut args = vec![
            "--risk".to_string(),
            "high".to_string(),
            "--timeout".to_string(),
            "5000".to_string(),
        ];
        assert_eq!(opt_value(&mut args, "--risk"), Some("high".into()));
        assert_eq!(opt_value(&mut args, "--timeout"), Some("5000".into()));
        assert!(args.is_empty());
    }
}
