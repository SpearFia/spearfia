//! A minimal MCP client for `filmcraft-cli mcp`: newline-delimited JSON-RPC over the child's
//! stdio. Just `initialize` and `tools/call`; FilmCraft's tool results are JSON text.

use serde_json::{Value, json};
use spearfia_core::CoreError;
use spearfia_core::error::Result;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::Duration;

/// Longest wait for one reply, so a hung FilmCraft can't hang the caller.
const REPLY_TIMEOUT: Duration = Duration::from_secs(120);

pub struct Client {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    next_id: u64,
}

fn fail(msg: impl std::fmt::Display) -> CoreError {
    CoreError::Invalid(format!("FilmCraft: {msg}"))
}

/// Standard base64 (padding optional), or `None` on a stray character.
fn base64(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=') {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
#[test]
fn base64_decodes() {
    assert_eq!(base64("aGVsbG8=").unwrap(), b"hello");
    assert_eq!(base64("aGk").unwrap(), b"hi");
    assert!(base64("a*b").is_none());
}

impl Client {
    /// Start `cli mcp` headless (an empty in-memory project) and initialize the session.
    pub fn start(cli: &Path) -> Result<Self> {
        let mut child = Command::new(cli)
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|source| CoreError::Io {
                path: cli.into(),
                source,
            })?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return Err(fail("no stdio pipes"));
        };
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout)
                .lines()
                .map_while(std::io::Result::ok)
            {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut c = Self {
            child,
            stdin,
            lines,
            next_id: 1,
        };
        c.request(
            "initialize",
            json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "spearfia", "version": env!("CARGO_PKG_VERSION")}}),
        )?;
        c.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))?;
        Ok(c)
    }

    fn send(&mut self, msg: &Value) -> Result<()> {
        writeln!(self.stdin, "{msg}")
            .and_then(|()| self.stdin.flush())
            .map_err(|e| fail(format!("the server stopped ({e})")))
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        loop {
            let line = match self.lines.recv_timeout(REPLY_TIMEOUT) {
                Ok(l) => l,
                Err(RecvTimeoutError::Timeout) => {
                    return Err(fail(format!("no reply to {method}")));
                }
                Err(RecvTimeoutError::Disconnected) => return Err(fail("the server exited")),
            };
            // Skip notifications (progress) and anything that isn't our reply.
            let Ok(msg) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if msg.get("id") != Some(&json!(id)) {
                continue;
            }
            if let Some(e) = msg.get("error") {
                return Err(fail(
                    e.get("message").and_then(Value::as_str).unwrap_or("error"),
                ));
            }
            return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    /// Call a tool. Its text result is parsed as JSON (FilmCraft's tools return JSON).
    pub fn tool(&mut self, name: &str, args: Value) -> Result<Value> {
        self.call(name, args, name)
    }

    /// Run one FilmCraft command (`command_run`).
    pub fn command(&mut self, id: &str, params: Value) -> Result<Value> {
        let what = format!("{id} {params}");
        self.call("command_run", json!({"id": id, "params": params}), &what)
    }

    /// Render the sequence frame at `seconds` as PNG bytes (`render_frame`).
    pub fn frame_png(&mut self, seconds: f64, max_side: u32) -> Result<Vec<u8>> {
        let r = self.request(
            "tools/call",
            json!({"name": "render_frame", "arguments": {"seconds": seconds, "max_side": max_side}}),
        )?;
        r["content"]
            .as_array()
            .and_then(|c| c.iter().find_map(|c| c["data"].as_str()))
            .and_then(base64)
            .ok_or_else(|| fail(format!("render_frame at {seconds} s returned no image")))
    }

    fn call(&mut self, name: &str, args: Value, what: &str) -> Result<Value> {
        let r = self.request("tools/call", json!({"name": name, "arguments": args}))?;
        let text = r["content"]
            .as_array()
            .and_then(|c| c.iter().find_map(|c| c["text"].as_str()))
            .unwrap_or_default();
        if r["isError"].as_bool() == Some(true) {
            return Err(fail(format!("{what}: {text}")));
        }
        Ok(serde_json::from_str(text).unwrap_or(Value::Null))
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // The headless project lives only in this process; whatever wasn't saved is dropped.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
