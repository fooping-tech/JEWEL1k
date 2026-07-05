//! Minimal HTTP/1.1 client for the plugin's localhost API. std-only on
//! purpose: hooks must start fast and the API is always 127.0.0.1.

use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

pub struct Client {
    pub port: u16,
    pub token: Option<String>,
    /// Read timeout; long for blocking approval requests.
    pub timeout: Duration,
}

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub body: Value,
}

impl Client {
    pub fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Response, String> {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).map_err(|e| {
            format!(
                "cannot reach agent-key API on 127.0.0.1:{} ({e}). Is the tray app running?",
                self.port
            )
        })?;
        stream
            .set_read_timeout(Some(self.timeout))
            .map_err(|e| e.to_string())?;

        let payload = body.map(|b| b.to_string()).unwrap_or_default();
        let mut req = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
            self.port,
            payload.len()
        );
        if let Some(token) = &self.token {
            req.push_str(&format!("x-agent-key-token: {token}\r\n"));
        }
        req.push_str("\r\n");
        req.push_str(&payload);
        stream.write_all(req.as_bytes()).map_err(|e| e.to_string())?;

        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&raw);
        let mut sections = text.splitn(2, "\r\n\r\n");
        let head = sections.next().unwrap_or("");
        let body_text = sections.next().unwrap_or("").trim();
        let status: u16 = head
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|s| s.parse().ok())
            .ok_or("malformed HTTP response")?;
        let body = serde_json::from_str(body_text).unwrap_or(Value::Null);
        Ok(Response { status, body })
    }
}
