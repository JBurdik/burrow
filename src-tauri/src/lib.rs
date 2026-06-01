use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

struct PtyInstance {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
}

struct PtyState {
    ptys: Mutex<HashMap<u32, PtyInstance>>,
}

struct DbState {
    conn: Mutex<Connection>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Workspace {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub created_at: i64,
    pub last_opened: Option<i64>,
}

// ── PTY commands ──────────────────────────────────────────────────────────────

#[tauri::command]
fn create_pty(
    id: u32,
    cwd: String,
    cols: u16,
    rows: u16,
    pty_state: State<PtyState>,
    app: AppHandle,
) -> Result<(), String> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| e.to_string())?;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let mut cmd = CommandBuilder::new(&shell);
    cmd.cwd(&cwd);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");

    pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;

    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;

    let event_name = format!("pty-data-{id}");
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let data = buf[..n].to_vec();
                    let _ = app.emit(&event_name, data);
                }
            }
        }
    });

    let mut ptys = pty_state.ptys.lock().unwrap();
    ptys.insert(id, PtyInstance { master: pair.master, writer });

    Ok(())
}

#[tauri::command]
fn write_pty(id: u32, data: Vec<u8>, pty_state: State<PtyState>) -> Result<(), String> {
    let mut ptys = pty_state.ptys.lock().unwrap();
    if let Some(pty) = ptys.get_mut(&id) {
        pty.writer.write_all(&data).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn resize_pty(id: u32, cols: u16, rows: u16, pty_state: State<PtyState>) -> Result<(), String> {
    let ptys = pty_state.ptys.lock().unwrap();
    if let Some(pty) = ptys.get(&id) {
        pty.master
            .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn kill_pty(id: u32, pty_state: State<PtyState>) {
    let mut ptys = pty_state.ptys.lock().unwrap();
    ptys.remove(&id);
}

#[tauri::command]
fn get_pty_foreground(id: u32, pty_state: State<PtyState>) -> String {
    let ptys = pty_state.ptys.lock().unwrap();
    let Some(pty) = ptys.get(&id) else { return String::new() };
    let Some(pgid) = pty.master.process_group_leader() else { return String::new() };

    let Ok(out) = std::process::Command::new("ps")
        .args(["-g", &pgid.to_string(), "-o", "comm="])
        .output()
    else {
        return String::new();
    };

    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let name = line.trim();
        if !name.is_empty()
            && !matches!(name, "zsh" | "bash" | "sh" | "fish" | "csh" | "tcsh" | "dash")
        {
            return name.to_string();
        }
    }
    String::new()
}

// ── Git command ───────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct GitOutput {
    stdout: String,
    stderr: String,
    code: i32,
}

fn git_binary() -> &'static str {
    for p in &["/usr/bin/git", "/usr/local/bin/git", "/opt/homebrew/bin/git"] {
        if std::path::Path::new(p).exists() {
            return p;
        }
    }
    "/usr/bin/git"
}

#[tauri::command]
fn run_git(cwd: String, args: Vec<String>) -> GitOutput {
    let git = git_binary();
    match std::process::Command::new(git).args(&args).current_dir(&cwd).output() {
        Ok(out) => GitOutput {
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            code: out.status.code().unwrap_or(-1),
        },
        Err(e) => GitOutput { stdout: String::new(), stderr: e.to_string(), code: -1 },
    }
}

// ── File system commands ──────────────────────────────────────────────────────

#[derive(Serialize)]
struct DirEntry {
    name: String,
    is_dir: bool,
}

#[tauri::command]
fn read_dir_shallow(path: String) -> Result<Vec<DirEntry>, String> {
    let entries = std::fs::read_dir(&path).map_err(|e| e.to_string())?;
    let mut result: Vec<DirEntry> = entries
        .filter_map(|e| e.ok())
        .map(|e| DirEntry {
            name: e.file_name().to_string_lossy().into_owned(),
            is_dir: e.file_type().map(|t| t.is_dir()).unwrap_or(false),
        })
        .collect();
    result.sort_by(|a, b| {
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });
    Ok(result)
}

// ── Workspace commands ────────────────────────────────────────────────────────

#[tauri::command]
fn list_workspaces(db: State<DbState>) -> Result<Vec<Workspace>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, name, path, created_at, last_opened
             FROM workspaces
             ORDER BY COALESCE(last_opened, 0) DESC, created_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(Workspace {
                id: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
                created_at: row.get(3)?,
                last_opened: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(rows)
}

#[tauri::command]
fn create_workspace(name: String, path: String, db: State<DbState>) -> Result<Workspace, String> {
    let conn = db.conn.lock().unwrap();
    let now = unix_now();
    conn.execute(
        "INSERT OR IGNORE INTO workspaces (name, path, created_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![name, path, now],
    )
    .map_err(|e| e.to_string())?;

    let id = conn.last_insert_rowid();
    Ok(Workspace { id, name, path, created_at: now, last_opened: None })
}

#[tauri::command]
fn delete_workspace(id: i64, db: State<DbState>) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    conn.execute("DELETE FROM workspaces WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn touch_workspace(id: i64, db: State<DbState>) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "UPDATE workspaces SET last_opened = ?1 WHERE id = ?2",
        rusqlite::params![unix_now(), id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn init_db(app: &AppHandle) -> Result<Connection, rusqlite::Error> {
    let data_dir = app.path().app_data_dir().expect("no app data dir");
    std::fs::create_dir_all(&data_dir).ok();
    let conn = Connection::open(data_dir.join("workspaces.db"))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS workspaces (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT    NOT NULL,
            path        TEXT    NOT NULL UNIQUE,
            created_at  INTEGER NOT NULL,
            last_opened INTEGER
        );",
    )?;
    Ok(conn)
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_os::init())
        .setup(|app| {
            let conn = init_db(app.handle()).expect("DB init failed");
            app.manage(DbState { conn: Mutex::new(conn) });
            app.manage(PtyState { ptys: Mutex::new(HashMap::new()) });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            create_pty,
            write_pty,
            resize_pty,
            kill_pty,
            get_pty_foreground,
            run_git,
            read_dir_shallow,
            list_workspaces,
            create_workspace,
            delete_workspace,
            touch_workspace,
            get_app_version,
        ])
        .run(tauri::generate_context!())
        .expect("error running tauri application");
}
