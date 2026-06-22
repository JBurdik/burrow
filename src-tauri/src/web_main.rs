//! Mobile-first HTTP gateway for a locally running Burrow instance.
//!
//! The gateway is a separate process from Tauri. It binds to loopback by default
//! and is intended to be published with Tailscale Serve. Its API is deliberately
//! narrow: pair a browser, inspect workspaces/PTYs, read snapshots, send terminal
//! input, and interrupt a foreground process.

use base64::{engine::general_purpose, Engine as _};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::net::IpAddr;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

const DEFAULT_BIND: &str = "127.0.0.1:9867";
const MAX_PAIR_BODY: u64 = 8 * 1024;
const MAX_INPUT_BODY: u64 = 64 * 1024;
const MAX_PAIR_FAILURES: u64 = 5;

struct Config {
    bind: String,
    data_dir: PathBuf,
    assets_dir: PathBuf,
}

struct AppState {
    data_dir: PathBuf,
    assets_dir: PathBuf,
    web_token: String,
    pairing_code: String,
    pairing_open: AtomicBool,
    pairing_failures: AtomicU64,
    next_daemon_id: AtomicU64,
}

#[derive(Deserialize)]
struct PairRequest {
    code: String,
}

#[derive(Deserialize)]
struct InputRequest {
    text: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WebSession {
    pty_id: u64,
    title: String,
    status: String,
    workspace_id: i64,
    workspace_name: String,
    cwd: String,
}

#[derive(Clone, Serialize)]
struct WebWorkspace {
    id: i64,
    name: String,
    path: String,
    sessions: Vec<WebSession>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("burrow-web: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let config = Config::from_args(std::env::args().skip(1))?;
    std::fs::create_dir_all(&config.data_dir).map_err(|e| format!("create data directory: {e}"))?;

    let web_token = load_or_create_secret(&config.data_dir.join("web.token"), 32)?;
    let pairing_code = random_pairing_code()?;
    let state = Arc::new(AppState {
        data_dir: config.data_dir,
        assets_dir: config.assets_dir,
        web_token,
        pairing_code: pairing_code.clone(),
        pairing_open: AtomicBool::new(true),
        pairing_failures: AtomicU64::new(0),
        next_daemon_id: AtomicU64::new(1),
    });

    if !is_loopback_bind(&config.bind) {
        return Err("refusing non-loopback bind; publish 127.0.0.1 through Tailscale Serve".into());
    }
    let server = Server::http(&config.bind).map_err(|e| format!("bind {}: {e}", config.bind))?;
    eprintln!("burrow-web: listening on http://{}", config.bind);
    eprintln!("burrow-web: one-time pairing code {pairing_code}");

    for request in server.incoming_requests() {
        handle_request(request, &state);
    }
    Ok(())
}

impl Config {
    fn from_args(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut bind = std::env::var("BURROW_WEB_BIND").unwrap_or_else(|_| DEFAULT_BIND.into());
        let mut data_dir = std::env::var_os("BURROW_DATA_DIR")
            .or_else(|| std::env::var_os("BURROW_HOME_DIR"))
            .map(PathBuf::from)
            .or_else(default_data_dir)
            .ok_or("cannot determine Burrow data directory; pass --data-dir")?;
        let mut assets_dir = std::env::var_os("BURROW_WEB_ASSETS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("../dist-mobile"));

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--bind" => bind = args.next().ok_or("--bind requires HOST:PORT")?,
                "--data-dir" => {
                    data_dir = PathBuf::from(args.next().ok_or("--data-dir requires PATH")?)
                }
                "--assets-dir" => {
                    assets_dir = PathBuf::from(args.next().ok_or("--assets-dir requires PATH")?)
                }
                "-h" | "--help" => {
                    println!(
                        "burrow-web [--bind HOST:PORT] [--data-dir PATH] [--assets-dir PATH]\n\
                         Environment: BURROW_WEB_BIND, BURROW_DATA_DIR/BURROW_HOME_DIR, \
                         BURROW_WEB_ASSETS_DIR"
                    );
                    std::process::exit(0);
                }
                _ => return Err(format!("unknown argument: {arg}")),
            }
        }
        Ok(Self {
            bind,
            data_dir,
            assets_dir,
        })
    }
}

fn handle_request(mut request: Request, state: &AppState) {
    let path = request.url().split('?').next().unwrap_or("/").to_string();
    let response = match (request.method(), path.as_str()) {
        (&Method::Get, "/health") => json_response(
            StatusCode(200),
            json!({"ok": true, "daemon": state.data_dir.join("daemon.sock").exists()}),
        ),
        (&Method::Get, "/api/pairing") => json_response(
            StatusCode(200),
            json!({"pairing_required": state.pairing_open.load(Ordering::Acquire)}),
        ),
        (&Method::Post, "/api/pair") => pair(&mut request, state),
        _ if path.starts_with("/api/") && !is_authorized(&request, &state.web_token) => {
            unauthorized()
        }
        (&Method::Get, "/api/sessions") => match list_daemon_sessions(state) {
            Ok(sessions) => json_response(StatusCode(200), json!({"sessions": sessions})),
            Err(error) => service_unavailable(error),
        },
        (&Method::Get, "/api/workspaces") => match list_workspaces(state) {
            Ok(workspaces) => json_response(StatusCode(200), json!(workspaces)),
            Err(error) => service_unavailable(error),
        },
        (&Method::Get, p) if p.starts_with("/api/output/") => {
            match parse_pty_id(p, "/api/output/").and_then(|id| pty_snapshot(state, id)) {
                Ok(output) => text_response(StatusCode(200), output),
                Err(error) => json_response(
                    StatusCode(404),
                    json!({"error": "output_unavailable", "detail": error}),
                ),
            }
        }
        (&Method::Post, p) if p.starts_with("/api/sessions/") && p.ends_with("/input") => {
            match parse_session_action(p, "/input")
                .and_then(|id| send_input(&mut request, state, id))
            {
                Ok(()) => json_response(StatusCode(200), json!({"ok": true})),
                Err(error) => json_response(
                    StatusCode(400),
                    json!({"error": "input_failed", "detail": error}),
                ),
            }
        }
        (&Method::Post, p) if p.starts_with("/api/sessions/") && p.ends_with("/interrupt") => {
            match parse_session_action(p, "/interrupt").and_then(|id| write_pty(state, id, &[3])) {
                Ok(()) => json_response(StatusCode(200), json!({"ok": true})),
                Err(error) => json_response(
                    StatusCode(400),
                    json!({"error": "interrupt_failed", "detail": error}),
                ),
            }
        }
        (_, p) if p.starts_with("/api/") => {
            json_response(StatusCode(404), json!({"error": "not_found"}))
        }
        (&Method::Get, _) | (&Method::Head, _) => static_response(&path, state),
        _ => json_response(StatusCode(405), json!({"error": "method_not_allowed"})),
    };

    if let Err(error) = request.respond(with_security_headers(response, path.starts_with("/api/")))
    {
        eprintln!("burrow-web: response error: {error}");
    }
}

fn pair(request: &mut Request, state: &AppState) -> Response<std::io::Cursor<Vec<u8>>> {
    if !state.pairing_open.load(Ordering::Acquire) {
        return json_response(StatusCode(409), json!({"error": "pairing_closed"}));
    }
    let mut body = String::new();
    if request
        .as_reader()
        .take(MAX_PAIR_BODY)
        .read_to_string(&mut body)
        .is_err()
    {
        return json_response(StatusCode(400), json!({"error": "invalid_body"}));
    }
    let Ok(pairing) = serde_json::from_str::<PairRequest>(&body) else {
        return json_response(StatusCode(400), json!({"error": "invalid_json"}));
    };
    if !constant_time_eq(
        pairing.code.trim().as_bytes(),
        state.pairing_code.as_bytes(),
    ) {
        let failures = state.pairing_failures.fetch_add(1, Ordering::AcqRel) + 1;
        if failures >= MAX_PAIR_FAILURES {
            state.pairing_open.store(false, Ordering::Release);
            return json_response(
                StatusCode(429),
                json!({"error": "pairing_locked", "restart_required": true}),
            );
        }
        return json_response(
            StatusCode(401),
            json!({"error": "invalid_pairing_code", "attempts_remaining": MAX_PAIR_FAILURES - failures}),
        );
    }
    if state
        .pairing_open
        .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return json_response(StatusCode(409), json!({"error": "pairing_closed"}));
    }
    json_response(
        StatusCode(200),
        json!({"token": state.web_token, "token_type": "Bearer"}),
    )
}

fn list_daemon_sessions(state: &AppState) -> Result<Vec<Value>, String> {
    let response = daemon_request(state, json!({"cmd": "ListSessions"}))?;
    Ok(response["sessions"].as_array().cloned().unwrap_or_default())
}

fn list_workspaces(state: &AppState) -> Result<Vec<WebWorkspace>, String> {
    let daemon_sessions = list_daemon_sessions(state)?;
    let mut workspaces = read_workspaces(&state.data_dir.join("workspaces.db"));
    if workspaces.is_empty() {
        for (index, session) in daemon_sessions.iter().enumerate() {
            let cwd = session["cwd"].as_str().unwrap_or("/").to_string();
            if workspaces.iter().all(|workspace| workspace.path != cwd) {
                workspaces.push(WebWorkspace {
                    id: -((index as i64) + 1),
                    name: Path::new(&cwd)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("Workspace")
                        .to_string(),
                    path: cwd,
                    sessions: Vec::new(),
                });
            }
        }
    }

    for session in daemon_sessions {
        let cwd = session["cwd"].as_str().unwrap_or("/").to_string();
        let target = workspaces
            .iter()
            .enumerate()
            .filter(|(_, workspace)| Path::new(&cwd).starts_with(Path::new(&workspace.path)))
            .max_by_key(|(_, workspace)| workspace.path.len())
            .map(|(index, _)| index);
        let target = target
            .or_else(|| workspaces.iter().position(|workspace| workspace.id == 0))
            .or_else(|| {
                workspaces.push(WebWorkspace {
                    id: 0,
                    name: "Other".into(),
                    path: "/".into(),
                    sessions: Vec::new(),
                });
                Some(workspaces.len() - 1)
            })
            .unwrap();
        let workspace = &mut workspaces[target];
        let pty_id = session["pty_id"].as_u64().unwrap_or(0);
        let title = session["title"]
            .as_str()
            .filter(|title| !title.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("PTY {pty_id}"));
        workspace.sessions.push(WebSession {
            pty_id,
            title,
            status: if session["alive"].as_bool().unwrap_or(false) {
                "running"
            } else {
                "done"
            }
            .into(),
            workspace_id: workspace.id,
            workspace_name: workspace.name.clone(),
            cwd,
        });
    }
    workspaces.retain(|workspace| !workspace.sessions.is_empty() || workspace.id > 0);
    Ok(workspaces)
}

fn read_workspaces(path: &Path) -> Vec<WebWorkspace> {
    let Ok(connection) = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return Vec::new();
    };
    let Ok(mut statement) = connection.prepare(
        "SELECT id, name, path FROM workspaces ORDER BY COALESCE(last_opened, created_at) DESC",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = statement.query_map([], |row| {
        Ok(WebWorkspace {
            id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
            sessions: Vec::new(),
        })
    }) else {
        return Vec::new();
    };
    rows.filter_map(Result::ok).collect()
}

fn pty_snapshot(state: &AppState, pty_id: u64) -> Result<String, String> {
    let response = daemon_request(state, json!({"cmd": "SnapshotPty", "pty_id": pty_id}))?;
    let encoded = response["data"].as_str().unwrap_or("");
    let bytes = general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| format!("decode snapshot: {e}"))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn send_input(request: &mut Request, state: &AppState, pty_id: u64) -> Result<(), String> {
    let mut body = String::new();
    request
        .as_reader()
        .take(MAX_INPUT_BODY)
        .read_to_string(&mut body)
        .map_err(|e| format!("read input: {e}"))?;
    let input: InputRequest =
        serde_json::from_str(&body).map_err(|e| format!("parse input: {e}"))?;
    if input.text.len() > MAX_INPUT_BODY as usize {
        return Err("input too large".into());
    }
    write_pty(state, pty_id, input.text.as_bytes())
}

fn write_pty(state: &AppState, pty_id: u64, data: &[u8]) -> Result<(), String> {
    daemon_request(
        state,
        json!({
            "cmd": "WritePty",
            "pty_id": pty_id,
            "data": general_purpose::STANDARD.encode(data),
        }),
    )?;
    Ok(())
}

fn daemon_request(state: &AppState, mut message: Value) -> Result<Value, String> {
    let daemon_token = std::fs::read_to_string(state.data_dir.join("daemon.token"))
        .map_err(|e| format!("read daemon token: {e}"))?;
    let stream = UnixStream::connect(state.data_dir.join("daemon.sock"))
        .map_err(|e| format!("connect daemon socket: {e}"))?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .map_err(|e| format!("set daemon timeout: {e}"))?;
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut writer = BufWriter::new(stream);
    daemon_command(
        &mut reader,
        &mut writer,
        json!({
            "id": state.next_daemon_id.fetch_add(1, Ordering::Relaxed),
            "cmd": "Auth",
            "token": daemon_token.trim(),
        }),
    )?;
    message["id"] = json!(state.next_daemon_id.fetch_add(1, Ordering::Relaxed));
    daemon_command(&mut reader, &mut writer, message)
}

fn daemon_command(
    reader: &mut BufReader<UnixStream>,
    writer: &mut BufWriter<UnixStream>,
    message: Value,
) -> Result<Value, String> {
    writeln!(writer, "{message}").map_err(|e| format!("write daemon request: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("flush daemon request: {e}"))?;
    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(0) => return Err("daemon closed connection".into()),
        Ok(_) => {}
        Err(e) => return Err(format!("read daemon response: {e}")),
    }
    let response: Value =
        serde_json::from_str(line.trim()).map_err(|e| format!("parse daemon response: {e}"))?;
    if response["ok"].as_bool() != Some(true) {
        return Err(response["error"]
            .as_str()
            .unwrap_or("daemon request failed")
            .into());
    }
    Ok(response)
}

fn parse_pty_id(path: &str, prefix: &str) -> Result<u64, String> {
    path.strip_prefix(prefix)
        .ok_or("invalid path")?
        .parse::<u64>()
        .map_err(|_| "invalid PTY id".into())
}

fn parse_session_action(path: &str, suffix: &str) -> Result<u64, String> {
    let value = path
        .strip_prefix("/api/sessions/")
        .and_then(|value| value.strip_suffix(suffix))
        .ok_or("invalid path")?;
    value.parse::<u64>().map_err(|_| "invalid PTY id".into())
}

fn static_response(path: &str, state: &AppState) -> Response<std::io::Cursor<Vec<u8>>> {
    let requested = safe_asset_path(&state.assets_dir, path);
    let asset = requested.filter(|path| path.is_file()).or_else(|| {
        let mobile = state.assets_dir.join("mobile.html");
        mobile.is_file().then_some(mobile)
    });
    let Some(asset) = asset else {
        return json_response(StatusCode(404), json!({"error": "assets_not_found"}));
    };
    match std::fs::read(&asset) {
        Ok(bytes) => {
            let mut response = Response::from_data(bytes).with_status_code(StatusCode(200));
            add_header(&mut response, "Content-Type", content_type(&asset));
            response
        }
        Err(error) => json_response(
            StatusCode(500),
            json!({"error": "asset_read_failed", "detail": error.to_string()}),
        ),
    }
}

fn safe_asset_path(root: &Path, url_path: &str) -> Option<PathBuf> {
    let relative = url_path.trim_start_matches('/');
    let relative = if relative.is_empty() {
        "mobile.html"
    } else {
        relative
    };
    let path = Path::new(relative);
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return None;
    }
    Some(root.join(path))
}

fn is_authorized(request: &Request, token: &str) -> bool {
    request.headers().iter().any(|header| {
        header.field.equiv("Authorization")
            && header
                .value
                .as_str()
                .strip_prefix("Bearer ")
                .is_some_and(|candidate| constant_time_eq(candidate.as_bytes(), token.as_bytes()))
    })
}

fn unauthorized() -> Response<std::io::Cursor<Vec<u8>>> {
    let mut response = json_response(StatusCode(401), json!({"error": "unauthorized"}));
    add_header(&mut response, "WWW-Authenticate", "Bearer");
    response
}

fn service_unavailable(error: String) -> Response<std::io::Cursor<Vec<u8>>> {
    json_response(
        StatusCode(503),
        json!({"error": "daemon_unavailable", "detail": error}),
    )
}

fn text_response(status: StatusCode, value: String) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut response = Response::from_string(value).with_status_code(status);
    add_header(&mut response, "Content-Type", "text/plain; charset=utf-8");
    response
}

fn json_response(status: StatusCode, value: Value) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut response = Response::from_data(serde_json::to_vec(&value).unwrap_or_default())
        .with_status_code(status);
    add_header(
        &mut response,
        "Content-Type",
        "application/json; charset=utf-8",
    );
    response
}

fn with_security_headers(
    mut response: Response<std::io::Cursor<Vec<u8>>>,
    api: bool,
) -> Response<std::io::Cursor<Vec<u8>>> {
    add_header(&mut response, "Content-Security-Policy", "default-src 'self'; connect-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self'; base-uri 'none'; frame-ancestors 'none'");
    add_header(&mut response, "Referrer-Policy", "no-referrer");
    add_header(&mut response, "X-Content-Type-Options", "nosniff");
    add_header(&mut response, "X-Frame-Options", "DENY");
    if api {
        add_header(&mut response, "Cache-Control", "no-store");
    } else {
        add_header(&mut response, "Cache-Control", "no-cache");
    }
    response
}

fn add_header(response: &mut Response<std::io::Cursor<Vec<u8>>>, name: &str, value: &str) {
    if let Ok(header) = Header::from_bytes(name, value) {
        response.add_header(header);
    }
}

fn load_or_create_secret(path: &Path, byte_count: usize) -> Result<String, String> {
    if let Ok(existing) = std::fs::read_to_string(path) {
        let existing = existing.trim();
        if !existing.is_empty() {
            return Ok(existing.to_owned());
        }
    }
    let secret = random_hex(byte_count)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    match options.open(path) {
        Ok(mut file) => {
            file.write_all(secret.as_bytes())
                .map_err(|e| format!("write web token: {e}"))?;
            Ok(secret)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => std::fs::read_to_string(path)
            .map(|value| value.trim().to_owned())
            .map_err(|e| format!("read concurrently-created web token: {e}")),
        Err(e) => Err(format!("create web token: {e}")),
    }
}

fn random_hex(byte_count: usize) -> Result<String, String> {
    let mut bytes = vec![0; byte_count];
    File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .map_err(|e| format!("read secure randomness: {e}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn random_pairing_code() -> Result<String, String> {
    let random = random_hex(4)?;
    let value = u32::from_str_radix(&random, 16).map_err(|e| e.to_string())? % 1_000_000;
    Ok(format!("{value:06}"))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    for index in 0..left.len().max(right.len()) {
        difference |= usize::from(
            left.get(index).copied().unwrap_or(0) ^ right.get(index).copied().unwrap_or(0),
        );
    }
    difference == 0
}

fn is_loopback_bind(bind: &str) -> bool {
    bind.rsplit_once(':')
        .and_then(|(host, _)| host.trim_matches(['[', ']']).parse::<IpAddr>().ok())
        .is_some_and(|ip| ip.is_loopback())
}

fn default_data_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    #[cfg(target_os = "macos")]
    return Some(home.join("Library/Application Support/com.agenticide.app"));
    #[cfg(not(target_os = "macos"))]
    return Some(
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"))
            .join("com.agenticide.app"),
    );
}

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
    {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_paths_are_confined_to_asset_root() {
        let root = Path::new("/assets");
        assert_eq!(safe_asset_path(root, "/"), Some(root.join("mobile.html")));
        assert_eq!(
            safe_asset_path(root, "/assets/main.js"),
            Some(root.join("assets/main.js"))
        );
        assert_eq!(safe_asset_path(root, "/../daemon.token"), None);
    }

    #[test]
    fn token_comparison_handles_length_and_content() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secrex"));
        assert!(!constant_time_eq(b"secret", b"secret-longer"));
    }

    #[test]
    fn default_bind_is_loopback() {
        assert!(is_loopback_bind(DEFAULT_BIND));
        assert!(is_loopback_bind("[::1]:9867"));
        assert!(!is_loopback_bind("0.0.0.0:9867"));
    }

    #[test]
    fn parses_session_action_paths() {
        assert_eq!(
            parse_session_action("/api/sessions/42/input", "/input"),
            Ok(42)
        );
        assert!(parse_session_action("/api/sessions/nope/input", "/input").is_err());
    }
}
