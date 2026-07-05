//! Minimal localhost HTTP API (127.0.0.1 only, optional shared token).
//!
//! This is how CLIs and agent hooks reach the plugin without going through
//! a webview. Endpoints (all JSON):
//!
//! - `GET  /health`                    -> Health
//! - `GET  /state`                     -> CurrentState
//! - `GET  /devices`                   -> DeviceInfo[]
//! - `POST /connect {transport,port}`  -> DeviceInfo
//! - `POST /disconnect`                -> {ok}
//! - `POST /status {state,risk?}`      -> CurrentState
//! - `POST /brightness {value}`        -> CurrentState
//! - `POST /approval {title,risk,...}` -> blocks, returns ApprovalResolution
//! - `POST /approval?wait=false ...`   -> {status:"pending",id} | resolution
//! - `POST /approval/<id>/cancel`      -> ApprovalResolution
//! - `POST /simulate {gesture}`        -> {ok} (mock transport only)
//!
//! Deliberately hand-rolled on std::net to keep the dependency surface
//! small; it only ever parses requests it produced itself (CLI) or curl.

use crate::manager::{ConnectOptions, ManagerCore};
use agent_key_core::risk_policy::DEFAULT_TIMEOUT_MS;
use agent_key_core::{ApprovalRequest, ButtonGesture, StatusUpdate, SubmitOutcome};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

pub const DEFAULT_PORT: u16 = 43117;

pub struct ServerConfig {
    pub port: u16,
    pub token: Option<String>,
}

pub fn spawn(manager: Arc<ManagerCore>, config: ServerConfig) -> std::io::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", config.port))?;
    let port = listener.local_addr()?.port();
    log::info!("agent-key localhost API listening on 127.0.0.1:{port}");
    let token = Arc::new(config.token);
    std::thread::Builder::new()
        .name("agent-key-http".into())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let manager = manager.clone();
                let token = token.clone();
                std::thread::spawn(move || {
                    if let Err(e) = handle(stream, manager, token.as_deref()) {
                        log::debug!("http connection error: {e}");
                    }
                });
            }
        })?;
    Ok(port)
}

struct Request {
    method: String,
    path: String,
    query: String,
    body: Value,
    token: Option<String>,
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<Request> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_uppercase();
    let target = parts.next().unwrap_or("/").to_string();
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target, String::new()),
    };

    let mut content_length = 0usize;
    let mut token = None;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header)?;
        let header = header.trim();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            let name = name.trim().to_ascii_lowercase();
            let value = value.trim();
            if name == "content-length" {
                content_length = value.parse().unwrap_or(0);
            } else if name == "x-agent-key-token" {
                token = Some(value.to_string());
            }
        }
    }
    let mut body_bytes = vec![0u8; content_length.min(1_048_576)];
    if !body_bytes.is_empty() {
        reader.read_exact(&mut body_bytes)?;
    }
    let body = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);
    Ok(Request {
        method,
        path,
        query,
        body,
        token,
    })
}

fn respond(stream: &mut TcpStream, status: u16, body: &Value) -> std::io::Result<()> {
    let text = body.to_string();
    let reason = match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        409 => "Conflict",
        504 => "Gateway Timeout",
        _ => "Internal Server Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        text.len(),
        text
    )
}

fn query_flag(query: &str, key: &str, default: bool) -> bool {
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k == key {
                return v != "false" && v != "0";
            }
        } else if pair == key {
            return true;
        }
    }
    default
}

fn handle(
    mut stream: TcpStream,
    manager: Arc<ManagerCore>,
    token: Option<&str>,
) -> std::io::Result<()> {
    let req = read_request(&mut stream)?;

    if let Some(expected) = token {
        if req.token.as_deref() != Some(expected) {
            return respond(&mut stream, 401, &json!({ "error": "invalid or missing token" }));
        }
    }

    let result = route(&req, &manager);
    match result {
        Ok((status, body)) => respond(&mut stream, status, &body),
        Err(msg) => respond(&mut stream, 400, &json!({ "error": msg })),
    }
}

fn route(req: &Request, manager: &Arc<ManagerCore>) -> Result<(u16, Value), String> {
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/health") => Ok((200, serde_json::to_value(manager.get_health()).unwrap())),
        ("GET", "/state") => Ok((
            200,
            serde_json::to_value(manager.get_current_state()).unwrap(),
        )),
        ("GET", "/devices") => Ok((200, serde_json::to_value(manager.list_devices()).unwrap())),
        ("POST", "/connect") => {
            let options: ConnectOptions =
                serde_json::from_value(req.body.clone()).map_err(|e| e.to_string())?;
            let device = manager.connect(options).map_err(|e| e.to_string())?;
            Ok((200, serde_json::to_value(device).unwrap()))
        }
        ("POST", "/disconnect") => {
            manager.disconnect().map_err(|e| e.to_string())?;
            Ok((200, json!({ "ok": true })))
        }
        ("POST", "/status") => {
            let update: StatusUpdate =
                serde_json::from_value(req.body.clone()).map_err(|e| e.to_string())?;
            let state = manager.set_status(update).map_err(|e| e.to_string())?;
            Ok((200, serde_json::to_value(state).unwrap()))
        }
        ("POST", "/brightness") => {
            let value = req
                .body
                .get("value")
                .and_then(Value::as_u64)
                .ok_or("body must be {\"value\": 0-255}")?;
            let state = manager
                .set_brightness(value.min(255) as u8)
                .map_err(|e| e.to_string())?;
            Ok((200, serde_json::to_value(state).unwrap()))
        }
        ("POST", "/approval") => {
            let request: ApprovalRequest =
                serde_json::from_value(req.body.clone()).map_err(|e| e.to_string())?;
            let wait = query_flag(&req.query, "wait", true);
            let timeout = request.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
            let outcome = manager.request_approval(request).map_err(|e| e.to_string())?;
            match outcome {
                SubmitOutcome::Resolved(res) => Ok((200, serde_json::to_value(res).unwrap())),
                SubmitOutcome::Pending { id } if !wait => {
                    Ok((202, json!({ "status": "pending", "id": id })))
                }
                SubmitOutcome::Pending { id } => {
                    // grace period so queue timeouts fire before we give up
                    match manager.wait_for_resolution(&id, timeout + 2_000) {
                        Some(res) => Ok((200, serde_json::to_value(res).unwrap())),
                        None => Ok((504, json!({ "status": "unresolved", "id": id }))),
                    }
                }
            }
        }
        ("POST", "/simulate") => {
            let gesture: ButtonGesture = serde_json::from_value(
                req.body.get("gesture").cloned().unwrap_or(Value::Null),
            )
            .map_err(|_| "body must be {\"gesture\": \"single|double|long|very_long\"}")?;
            manager.simulate_button(gesture).map_err(|e| e.to_string())?;
            Ok((200, json!({ "ok": true })))
        }
        ("POST", path) => {
            // /approval/<id>/cancel
            if let Some(rest) = path.strip_prefix("/approval/") {
                if let Some(id) = rest.strip_suffix("/cancel") {
                    let res = manager.cancel_approval(id).map_err(|e| e.to_string())?;
                    return Ok((200, serde_json::to_value(res).unwrap()));
                }
            }
            Ok((404, json!({ "error": "not found" })))
        }
        _ => Ok((404, json!({ "error": "not found" }))),
    }
}
