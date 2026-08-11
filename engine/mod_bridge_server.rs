//! Local HTTP mod bridge for standalone Ruffle (Flashpoint / desktop).
//!
//! Listens on 127.0.0.1:PORT (default 8768). The HTTP thread never touches
//! `Player` (not Send) — it enqueues RPC work; the main thread drains via
//! `poll_pending` each frame.
//!
//! Hardened against panics / floods so a bad RPC cannot kill the game loop.
//!
//!   GET  /health  → JSON status
//!   POST /rpc     → body: mod_bridge JSON request, returns JSON string

use ruffle_core::Player;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

const MAX_BODY: usize = 256 * 1024;
const MAX_QUEUE: usize = 2;
const MAX_RPC_PER_FRAME: usize = 1;

struct PendingRpc {
    request: String,
    reply: SyncSender<String>,
}

struct BridgeMeta {
    title: String,
    has_player: bool,
}

static QUEUE: OnceLock<Mutex<VecDeque<PendingRpc>>> = OnceLock::new();
static META: OnceLock<Mutex<BridgeMeta>> = OnceLock::new();
static STARTED: AtomicBool = AtomicBool::new(false);
/// Paths forced every frame: (path, raw JSON value text e.g. `0` or `999999`)
static FREEZES: OnceLock<Mutex<Vec<(String, String)>>> = OnceLock::new();

fn freezes() -> &'static Mutex<Vec<(String, String)>> {
    FREEZES.get_or_init(|| Mutex::new(Vec::new()))
}

fn queue() -> &'static Mutex<VecDeque<PendingRpc>> {
    QUEUE.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn meta() -> &'static Mutex<BridgeMeta> {
    META.get_or_init(|| {
        Mutex::new(BridgeMeta {
            title: String::new(),
            has_player: false,
        })
    })
}

pub fn attach_player(_player: Arc<Mutex<Player>>, title: impl Into<String>) {
    if let Ok(mut m) = meta().lock() {
        m.title = title.into();
        m.has_player = true;
        tracing::info!("mod-bridge: player attached ({})", m.title);
    }
}

pub fn detach_player() {
    if let Ok(mut m) = meta().lock() {
        m.title.clear();
        m.has_player = false;
        tracing::info!("mod-bridge: player detached");
    }
    // Fail any stuck waiters
    if let Ok(mut q) = queue().lock() {
        while let Some(job) = q.pop_front() {
            let _ = job
                .reply
                .send(r#"{"ok":false,"error":"player detached"}"#.into());
        }
    }
    // Drop freezes when movie closes
    if let Ok(mut f) = freezes().lock() {
        f.clear();
    }
}

/// Drain freezes + at most a few RPCs on the **main** thread. Panics are caught.
pub fn poll_pending(player: &mut Player) {
    // Apply freezes every frame (e.g. buildWait=0 for no spawn cooldown)
    if let Ok(list) = freezes().lock() {
        for (path, value_json) in list.iter() {
            let req = format!(
                r#"{{"op":"set","path":{},"value":{}}}"#,
                json_escape_raw(path),
                value_json
            );
            let _ = catch_unwind(AssertUnwindSafe(|| player.mod_bridge_rpc(&req)));
        }
    }

    for _ in 0..MAX_RPC_PER_FRAME {
        let job = {
            let Ok(mut q) = queue().lock() else {
                return;
            };
            q.pop_front()
        };
        let Some(job) = job else {
            break;
        };

        let result = catch_unwind(AssertUnwindSafe(|| handle_rpc(player, &job.request)));
        let body = match result {
            Ok(s) => s,
            Err(payload) => {
                let msg = panic_message(&payload);
                tracing::error!("mod-bridge: RPC panicked (contained): {msg}");
                format!(
                    r#"{{"ok":false,"error":{}}}"#,
                    json_escape_raw(&format!("rpc panicked: {msg}"))
                )
            }
        };
        let _ = job.reply.send(body);
    }
}

/// Handle freeze ops locally; everything else goes to core mod_bridge.
fn handle_rpc(player: &mut Player, request: &str) -> String {
    let trimmed = request.trim();
    // Minimal parse without pulling serde into desktop path critically
    if trimmed.contains("\"op\":\"freeze\"") || trimmed.contains("\"op\": \"freeze\"") {
        return op_freeze(trimmed);
    }
    if trimmed.contains("\"op\":\"unfreeze\"") || trimmed.contains("\"op\": \"unfreeze\"") {
        return op_unfreeze(trimmed);
    }
    if trimmed.contains("\"op\":\"list_freezes\"") || trimmed.contains("\"op\": \"list_freezes\"") {
        return op_list_freezes();
    }
    player.mod_bridge_rpc(request)
}

fn extract_json_string(blob: &str, key: &str) -> Option<String> {
    // "key":"value" or "key": "value"
    let patterns = [
        format!("\"{key}\":\""),
        format!("\"{key}\": \""),
    ];
    for p in patterns {
        if let Some(i) = blob.find(&p) {
            let rest = &blob[i + p.len()..];
            let mut out = String::new();
            let mut chars = rest.chars();
            while let Some(c) = chars.next() {
                if c == '\\' {
                    if let Some(n) = chars.next() {
                        out.push(n);
                    }
                } else if c == '"' {
                    return Some(out);
                } else {
                    out.push(c);
                }
            }
        }
    }
    None
}

fn extract_json_value_raw(blob: &str, key: &str) -> Option<String> {
    // value after "key":  — number, bool, null, or string
    let patterns = [format!("\"{key}\":"), format!("\"{key}\": ")];
    for p in patterns {
        if let Some(i) = blob.find(&p) {
            let rest = blob[i + p.len()..].trim_start();
            if rest.starts_with('"') {
                return extract_json_string(blob, key).map(|s| format!("\"{s}\""));
            }
            let end = rest
                .find(|c: char| c == ',' || c == '}' || c == '\n' || c == ' ')
                .unwrap_or(rest.len());
            let v = rest[..end].trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn op_freeze(blob: &str) -> String {
    let path = match extract_json_string(blob, "path") {
        Some(p) => p,
        None => return r#"{"ok":false,"error":"freeze needs path"}"#.into(),
    };
    let value = match extract_json_value_raw(blob, "value") {
        Some(v) => v,
        None => return r#"{"ok":false,"error":"freeze needs value"}"#.into(),
    };
    if let Ok(mut list) = freezes().lock() {
        list.retain(|(p, _)| p != &path);
        list.push((path.clone(), value.clone()));
        return format!(
            r#"{{"ok":true,"frozen":true,"path":{},"value":{},"count":{}}}"#,
            json_escape_raw(&path),
            value,
            list.len()
        );
    }
    r#"{"ok":false,"error":"freeze lock failed"}"#.into()
}

fn op_unfreeze(blob: &str) -> String {
    let path = extract_json_string(blob, "path");
    if let Ok(mut list) = freezes().lock() {
        if let Some(p) = path {
            list.retain(|(x, _)| x != &p);
            return format!(
                r#"{{"ok":true,"unfrozen":true,"path":{},"count":{}}}"#,
                json_escape_raw(&p),
                list.len()
            );
        }
        list.clear();
        return r#"{"ok":true,"unfrozen":"all","count":0}"#.into();
    }
    r#"{"ok":false,"error":"unfreeze lock failed"}"#.into()
}

fn op_list_freezes() -> String {
    if let Ok(list) = freezes().lock() {
        let items: Vec<String> = list
            .iter()
            .map(|(p, v)| format!(r#"{{"path":{},"value":{}}}"#, json_escape_raw(p), v))
            .collect();
        return format!(r#"{{"ok":true,"freezes":[{}],"count":{}}}"#, items.join(","), list.len());
    }
    r#"{"ok":false,"error":"list_freezes lock failed"}"#.into()
}

/// Start background HTTP server. Port 0 disables. Safe to call once.
pub fn start(port: u16) {
    if port == 0 {
        tracing::info!("mod-bridge: disabled (port 0)");
        return;
    }
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    thread::Builder::new()
        .name("ruffle-mod-bridge".into())
        .spawn(move || {
            if let Err(e) = serve(port) {
                tracing::error!("mod-bridge server error: {e}");
            }
        })
        .ok();
    tracing::info!("mod-bridge: listening on http://127.0.0.1:{port}");
}

fn serve(port: u16) -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    // Accept loop — each client handled on a short-lived thread so one blocked
    // RPC does not freeze /health.
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(20)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(20)));
                thread::Builder::new()
                    .name("ruffle-mod-bridge-client".into())
                    .spawn(move || {
                        let mut stream = stream;
                        if let Err(e) = handle_client(&mut stream) {
                            tracing::debug!("mod-bridge client error: {e}");
                        }
                    })
                    .ok();
            }
            Err(e) => tracing::debug!("mod-bridge accept: {e}"),
        }
    }
    Ok(())
}

fn handle_client(stream: &mut TcpStream) -> std::io::Result<()> {
    let mut buf = vec![0u8; 65536];
    let n = stream.read(&mut buf)?;
    if n == 0 {
        return Ok(());
    }
    let req = String::from_utf8_lossy(&buf[..n]);
    let first = req.lines().next().unwrap_or("");
    let is_health = first.starts_with("GET /health") || first.starts_with("GET / ");
    let is_rpc = first.starts_with("POST /rpc");
    let is_options = first.starts_with("OPTIONS ");

    let (status, body) = if is_options {
        (
            "204 No Content",
            String::new(),
        )
    } else if is_health {
        let (has, title) = meta()
            .lock()
            .map(|m| (m.has_player, m.title.clone()))
            .unwrap_or((false, String::new()));
        let qlen = queue().lock().map(|q| q.len()).unwrap_or(0);
        (
            "200 OK",
            format!(
                r#"{{"ok":true,"mod_bridge":true,"desktop":true,"has_player":{},"title":{},"queue":{}}}"#,
                has,
                json_escape_raw(&title),
                qlen
            ),
        )
    } else if is_rpc {
        let body_start = req.find("\r\n\r\n").map(|i| i + 4).unwrap_or(req.len());
        let content_len = req
            .lines()
            .find_map(|l| {
                let lower = l.to_ascii_lowercase();
                lower
                    .strip_prefix("content-length:")
                    .map(|v| v.trim().parse::<usize>().unwrap_or(0))
            })
            .unwrap_or(0)
            .min(MAX_BODY);
        let mut payload = req[body_start..].as_bytes().to_vec();
        while payload.len() < content_len {
            let mut more = vec![0u8; (content_len - payload.len()).min(8192)];
            match stream.read(&mut more) {
                Ok(0) => break,
                Ok(m) => payload.extend_from_slice(&more[..m]),
                Err(_) => break,
            }
        }
        if content_len > 0 {
            payload.truncate(content_len);
        }
        if payload.len() > MAX_BODY {
            (
                "413 Payload Too Large",
                r#"{"ok":false,"error":"body too large"}"#.into(),
            )
        } else {
            let request_json = String::from_utf8_lossy(&payload).to_string();
            let result = enqueue_rpc(request_json);
            ("200 OK", result)
        }
    } else {
        (
            "404 Not Found",
            r#"{"ok":false,"error":"use GET /health or POST /rpc"}"#.into(),
        )
    };

    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(resp.as_bytes());
    Ok(())
}

fn enqueue_rpc(request: String) -> String {
    let has = meta().lock().map(|m| m.has_player).unwrap_or(false);
    if !has {
        return r#"{"ok":false,"error":"no movie loaded yet"}"#.into();
    }

    let (tx, rx): (SyncSender<String>, Receiver<String>) = mpsc::sync_channel(1);
    {
        let Ok(mut q) = queue().lock() else {
            return r#"{"ok":false,"error":"queue lock failed"}"#.into();
        };
        // Cap waiters so a storm cannot pile up or stall the game.
        if q.len() >= MAX_QUEUE {
            return r#"{"ok":false,"error":"mod-bridge busy — one RPC at a time, retry"}"#.into();
        }
        q.push_back(PendingRpc {
            request,
            reply: tx,
        });
    }

    match rx.recv_timeout(Duration::from_secs(12)) {
        Ok(s) => s,
        Err(_) => {
            // Best-effort: drop our job if it never started (reply channel still open).
            if let Ok(mut q) = queue().lock() {
                q.retain(|j| !j.reply.send("".into()).is_err());
                // retain all — SyncSender can't identity-match; leave orphans;
                // poll_pending send to closed channel is fine.
                let _ = &mut q;
            }
            r#"{"ok":false,"error":"mod-bridge timeout (game may be paused / not ticking)"}"#
                .into()
        }
    }
}

fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".into()
    }
}

fn json_escape_raw(s: &str) -> String {
    // produce a JSON string literal content with quotes
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
