/// Burrow PTY daemon — runs as a detached OS process, owns all PTYs so they
/// survive app restarts. Listens on a Unix socket, speaks JSON-newline.
///
/// Start: spawned by the Tauri app on first run (if socket not found).
/// Stop:  lives until killed or system reboots.
///
/// Protocol overview:
///   Every message is a JSON object terminated by '\n'.
///   Client→daemon messages carry an "id" field for request/response matching.
///   Daemon→client events ("PtyData", "PtyExit") carry a "type" field.
///
///   Authentication: first message must be {"cmd":"Auth","token":"<token>"}.
///   Token is written to <data_dir>/daemon.token when the daemon starts.

use base64::{engine::general_purpose, Engine as _};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{unix::OwnedWriteHalf, UnixListener};
use tokio::sync::{broadcast, RwLock};

// ── Ring buffer ───────────────────────────────────────────────────────────────

const RING_LIMIT: usize = 512 * 1024; // 512 KB per PTY
// Generous so a bursty TUI repaint (Copilot/Claude) doesn't overrun a momentarily
// busy client before it drains; on overrun we still resync via the ring snapshot.
const BROADCAST_CAP: usize = 8192;

struct RingBuffer {
    chunks: VecDeque<Vec<u8>>,
    total: usize,
}

impl RingBuffer {
    fn new() -> Self {
        Self { chunks: VecDeque::new(), total: 0 }
    }

    fn push(&mut self, data: Vec<u8>) {
        self.total += data.len();
        self.chunks.push_back(data);
        while self.total > RING_LIMIT {
            match self.chunks.pop_front() {
                Some(old) => self.total = self.total.saturating_sub(old.len()),
                None => break,
            }
        }
    }

    fn snapshot(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.total);
        for c in &self.chunks {
            out.extend_from_slice(c);
        }
        out
    }
}

// ── PTY session ───────────────────────────────────────────────────────────────

struct PtySession {
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    ring: Mutex<RingBuffer>,
    cwd: Mutex<String>,
    title: Mutex<String>,
    alive: AtomicBool,
    tx: broadcast::Sender<Vec<u8>>,
}

// ── App state ─────────────────────────────────────────────────────────────────

struct DaemonState {
    sessions: RwLock<HashMap<u32, Arc<PtySession>>>,
    auth_token: String,
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let data_dir = PathBuf::from(
        std::env::var("BURROW_DATA_DIR").expect("BURROW_DATA_DIR not set"),
    );
    std::fs::create_dir_all(&data_dir).ok();

    // Write auth token (generate on first run, reuse on subsequent runs)
    let token_path = data_dir.join("daemon.token");
    let auth_token = std::fs::read_to_string(&token_path)
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| {
            let t = gen_token();
            let _ = std::fs::write(&token_path, &t);
            t
        });

    let socket_path = data_dir.join("daemon.sock");
    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path).expect("failed to bind daemon socket");

    let state = Arc::new(DaemonState {
        sessions: RwLock::new(HashMap::new()),
        auth_token,
    });

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let st = state.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_client(stream, st).await {
                        eprintln!("[burrow-daemon] client error: {e}");
                    }
                });
            }
            Err(e) => eprintln!("[burrow-daemon] accept error: {e}"),
        }
    }
}

fn gen_token() -> String {
    let mut bytes = [0u8; 32];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = <std::fs::File as Read>::read_exact(&mut f, &mut bytes);
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ── Client handler ────────────────────────────────────────────────────────────

type WriterRef = Arc<tokio::sync::Mutex<OwnedWriteHalf>>;
type Res = Result<(), Box<dyn std::error::Error + Send + Sync>>;

async fn handle_client(stream: tokio::net::UnixStream, state: Arc<DaemonState>) -> Res {
    let (read_half, write_half) = stream.into_split();
    let mut lines = BufReader::new(read_half).lines();
    let w = Arc::new(tokio::sync::Mutex::new(write_half));
    let mut authed = false;

    while let Some(line) = lines.next_line().await? {
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let id = msg["id"].as_u64().unwrap_or(0);
        let cmd = msg["cmd"].as_str().unwrap_or("");

        if cmd == "Auth" {
            let token = msg["token"].as_str().unwrap_or("");
            if token == state.auth_token {
                authed = true;
                reply(&w, id, json!({"ok": true})).await?;
            } else {
                reply(&w, id, json!({"ok": false, "error": "bad token"})).await?;
                return Ok(());
            }
            continue;
        }

        if !authed {
            reply(&w, id, json!({"ok": false, "error": "not authenticated"})).await?;
            return Ok(());
        }

        match cmd {
            "CreatePty" => cmd_create(&msg, id, &w, &state).await?,
            "AttachPty" => {
                cmd_attach(&msg, id, &w, &state).await?;
                return Ok(()); // connection becomes one-way stream
            }
            "WritePty" => cmd_write(&msg, id, &w, &state).await?,
            "ResizePty" => cmd_resize(&msg, id, &w, &state).await?,
            "KillPty" => cmd_kill(&msg, id, &w, &state).await?,
            "GetForeground" => cmd_foreground(&msg, id, &w, &state).await?,
            "ListSessions" => cmd_list(id, &w, &state).await?,
            _ => reply(&w, id, json!({"ok": false, "error": "unknown command"})).await?,
        }
    }
    Ok(())
}

// ── Command handlers ──────────────────────────────────────────────────────────

async fn cmd_create(msg: &Value, id: u64, w: &WriterRef, state: &Arc<DaemonState>) -> Res {
    let pty_id = msg["pty_id"].as_u64().unwrap_or(0) as u32;
    let cwd = msg["cwd"].as_str().unwrap_or("/").to_string();
    let cols = msg["cols"].as_u64().unwrap_or(220) as u16;
    let rows = msg["rows"].as_u64().unwrap_or(50) as u16;
    let env = msg["env"].as_object().cloned().unwrap_or_default();

    // Reuse existing alive session — just reattach on next AttachPty call
    {
        let sessions = state.sessions.read().await;
        if let Some(s) = sessions.get(&pty_id) {
            if s.alive.load(Ordering::Acquire) {
                return reply(w, id, json!({"ok": true, "existing": true})).await;
            }
        }
    }

    match spawn_session(pty_id, cwd, cols, rows, env, state).await {
        Ok(_) => reply(w, id, json!({"ok": true, "existing": false})).await,
        Err(e) => reply(w, id, json!({"ok": false, "error": e.to_string()})).await,
    }
}

async fn cmd_attach(msg: &Value, id: u64, w: &WriterRef, state: &Arc<DaemonState>) -> Res {
    let pty_id = msg["pty_id"].as_u64().unwrap_or(0) as u32;

    let session = {
        let sessions = state.sessions.read().await;
        sessions.get(&pty_id).cloned()
    };

    let Some(session) = session else {
        return reply(w, id, json!({"ok": false, "error": "session not found"})).await;
    };

    reply(w, id, json!({"ok": true})).await?;

    // Replay buffered output so the client sees everything missed while disconnected
    let snapshot = session.ring.lock().unwrap().snapshot();
    if !snapshot.is_empty() {
        let enc = general_purpose::STANDARD.encode(&snapshot);
        write_line(w, &json!({"type": "PtyData", "pty_id": pty_id, "data": enc})).await?;
    }

    // Forward live output until PTY exits or client disconnects
    let mut rx = session.tx.subscribe();
    loop {
        match rx.recv().await {
            Ok(data) if data.is_empty() => {
                let _ = write_line(w, &json!({"type": "PtyExit", "pty_id": pty_id})).await;
                return Ok(());
            }
            Ok(data) => {
                let enc = general_purpose::STANDARD.encode(&data);
                if write_line(w, &json!({"type": "PtyData", "pty_id": pty_id, "data": enc}))
                    .await
                    .is_err()
                {
                    return Ok(());
                }
            }
            // Client fell behind and the channel dropped messages (a TUI flooding
            // frames faster than the IPC drains). Just skip ahead — do NOT replay the
            // ring snapshot here: the snapshot contains the app's terminal QUERIES
            // (DA, DECRQM, OSC color), which xterm would answer AGAIN, feeding
            // duplicate responses back into the app → a redraw/flood feedback loop
            // (observed as Copilot emitting 500KB+ and never painting). The larger
            // BROADCAST_CAP makes lag rare in the first place.
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(_) => return Ok(()),
        }
    }
}

async fn cmd_write(msg: &Value, id: u64, w: &WriterRef, state: &Arc<DaemonState>) -> Res {
    let pty_id = msg["pty_id"].as_u64().unwrap_or(0) as u32;
    let data = msg["data"].as_str()
        .and_then(|s| general_purpose::STANDARD.decode(s).ok())
        .unwrap_or_default();

    let sessions = state.sessions.read().await;
    if let Some(s) = sessions.get(&pty_id) {
        let _ = s.writer.lock().unwrap().write_all(&data);
    }
    reply(w, id, json!({"ok": true})).await
}

async fn cmd_resize(msg: &Value, id: u64, w: &WriterRef, state: &Arc<DaemonState>) -> Res {
    let pty_id = msg["pty_id"].as_u64().unwrap_or(0) as u32;
    let cols = msg["cols"].as_u64().unwrap_or(220) as u16;
    let rows = msg["rows"].as_u64().unwrap_or(50) as u16;

    let sessions = state.sessions.read().await;
    if let Some(s) = sessions.get(&pty_id) {
        let _ = s.master.lock().unwrap().resize(
            PtySize { rows, cols, pixel_width: 0, pixel_height: 0 },
        );
    }
    reply(w, id, json!({"ok": true})).await
}

async fn cmd_kill(msg: &Value, id: u64, w: &WriterRef, state: &Arc<DaemonState>) -> Res {
    let pty_id = msg["pty_id"].as_u64().unwrap_or(0) as u32;
    state.sessions.write().await.remove(&pty_id);
    reply(w, id, json!({"ok": true})).await
}

async fn cmd_foreground(msg: &Value, id: u64, w: &WriterRef, state: &Arc<DaemonState>) -> Res {
    let pty_id = msg["pty_id"].as_u64().unwrap_or(0) as u32;
    let sessions = state.sessions.read().await;
    let proc = sessions.get(&pty_id)
        .and_then(|s| s.master.lock().unwrap().process_group_leader())
        .map(foreground_name)
        .unwrap_or_default();
    reply(w, id, json!({"ok": true, "process": proc})).await
}

async fn cmd_list(id: u64, w: &WriterRef, state: &Arc<DaemonState>) -> Res {
    let sessions = state.sessions.read().await;
    let list: Vec<Value> = sessions.iter().map(|(pid, s)| json!({
        "pty_id": pid,
        "cwd": *s.cwd.lock().unwrap(),
        "title": *s.title.lock().unwrap(),
        "alive": s.alive.load(Ordering::Acquire),
    })).collect();
    reply(w, id, json!({"ok": true, "sessions": list})).await
}

// ── PTY spawning ──────────────────────────────────────────────────────────────

async fn spawn_session(
    pty_id: u32,
    cwd: String,
    cols: u16,
    rows: u16,
    env: serde_json::Map<String, Value>,
    state: &Arc<DaemonState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| e.to_string())?;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let mut cmd = CommandBuilder::new(&shell);
    cmd.cwd(&cwd);
    for (k, v) in &env {
        if let Some(s) = v.as_str() {
            cmd.env(k, s);
        }
    }

    pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;

    let reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let (tx, _) = broadcast::channel(BROADCAST_CAP);

    let session = Arc::new(PtySession {
        writer: Mutex::new(writer),
        master: Mutex::new(pair.master),
        ring: Mutex::new(RingBuffer::new()),
        cwd: Mutex::new(cwd),
        title: Mutex::new(String::new()),
        alive: AtomicBool::new(true),
        tx: tx.clone(),
    });

    // Reader thread: feeds ring buffer and broadcast channel
    let sess_ref = session.clone();
    std::thread::spawn(move || {
        let mut reader = reader;
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => {
                    sess_ref.alive.store(false, Ordering::Release);
                    let _ = tx.send(vec![]); // empty = EOF signal
                    break;
                }
                Ok(n) => {
                    let data = buf[..n].to_vec();
                    sess_ref.ring.lock().unwrap().push(data.clone());
                    let _ = tx.send(data);
                }
            }
        }
    });

    state.sessions.write().await.insert(pty_id, session);
    Ok(())
}

// ── I/O helpers ───────────────────────────────────────────────────────────────

async fn reply(w: &WriterRef, id: u64, mut msg: Value) -> Res {
    msg["id"] = json!(id);
    write_line(w, &msg).await
}

async fn write_line(w: &WriterRef, msg: &Value) -> Res {
    let mut line = serde_json::to_string(msg)?;
    line.push('\n');
    let mut guard = w.lock().await;
    guard.write_all(line.as_bytes()).await?;
    guard.flush().await?;
    Ok(())
}

fn foreground_name(pgid: i32) -> String {
    let Ok(out) = std::process::Command::new("ps")
        .args(["-g", &pgid.to_string(), "-o", "comm="])
        .output()
    else {
        return String::new();
    };
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let raw = line.trim();
        if raw.is_empty() { continue; }
        let name = raw.rsplit('/').next().unwrap_or(raw).trim_start_matches('-');
        if !matches!(name, "zsh" | "bash" | "sh" | "fish" | "csh" | "tcsh" | "dash") {
            return name.to_string();
        }
    }
    String::new()
}
