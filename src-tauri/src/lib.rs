mod daemon_client;

use base64::{engine::general_purpose, Engine};
use daemon_client::DaemonClient;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager, State};

struct DaemonState {
    client: Mutex<Option<Arc<DaemonClient>>>,
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

// ── burrow CLI ────────────────────────────────────────────────────────────────

const BURROW_SCRIPT: &str = include_str!("../bin/burrow");

// Must stay identical to DAEMON_PROTO_VERSION in daemon_main.rs. Bumped only when
// daemon-side PTY behavior changes, so app-only updates don't needlessly restart
// the daemon (which would kill live PTY sessions). A mismatch on launch retires the
// stale daemon so the new behavior takes effect after an auto-update.
const DAEMON_PROTO_VERSION: &str = "2";

fn ensure_burrow_bin(app: &AppHandle) -> Option<std::path::PathBuf> {
    let dir = app.path().app_data_dir().ok()?.join("bin");
    std::fs::create_dir_all(&dir).ok()?;
    let script = dir.join("burrow");
    std::fs::write(&script, BURROW_SCRIPT).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755));
    }
    Some(dir)
}

fn burrow_session_dir(app: &AppHandle) -> Option<std::path::PathBuf> {
    let dir = app.path().app_data_dir().ok()?.join("sessions");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

// ── Agent config directories ───────────────────────────────────────────────────
// Hooks + agent docs are installed into each agent's *config dir*. By default that's
// ~/.claude (Claude) and ~/.codex (Codex), but a user can point an agent elsewhere via
// CLAUDE_CONFIG_DIR / CODEX_HOME — and crucially can use a DIFFERENT dir per work folder.
// We can't reliably read that per-folder env from here (the shell sets it after spawn),
// so the set of dirs to target is an explicit, persisted list the user edits in Settings.
// It's seeded with the defaults plus whatever CLAUDE_CONFIG_DIR/CODEX_HOME the app itself
// inherited at launch.

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct ConfigDirs {
    claude: Vec<String>,
    codex: Vec<String>,
    #[serde(default)]
    copilot: Vec<String>,
}

fn config_dirs_path(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_data_dir().ok()?.join("config-dirs.json"))
}

fn dedup(mut v: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    v.retain(|s| !s.trim().is_empty() && seen.insert(s.trim().to_string()));
    v.into_iter().map(|s| s.trim().to_string()).collect()
}

/// The dirs to install hooks/docs into. Reads the persisted list if present; otherwise
/// seeds from defaults (~/.claude, ~/.codex) + any CLAUDE_CONFIG_DIR/CODEX_HOME env.
fn load_config_dirs(app: &AppHandle) -> ConfigDirs {
    if let Some(path) = config_dirs_path(app) {
        if let Ok(s) = std::fs::read_to_string(&path) {
            if let Ok(cd) = serde_json::from_str::<ConfigDirs>(&s) {
                // Seed Copilot defaults for configs persisted before Copilot support existed.
                let copilot = if cd.copilot.is_empty() {
                    default_copilot_dirs(app)
                } else {
                    cd.copilot
                };
                return ConfigDirs {
                    claude: dedup(cd.claude),
                    codex: dedup(cd.codex),
                    copilot: dedup(copilot),
                };
            }
        }
    }
    // Seed defaults.
    let home = app.path().home_dir().ok();
    let mut claude = Vec::new();
    let mut codex = Vec::new();
    if let Some(h) = &home {
        claude.push(h.join(".claude").to_string_lossy().to_string());
        codex.push(h.join(".codex").to_string_lossy().to_string());
    }
    if let Ok(d) = std::env::var("CLAUDE_CONFIG_DIR") {
        claude.push(d);
    }
    if let Ok(d) = std::env::var("CODEX_HOME") {
        codex.push(d);
    }
    ConfigDirs {
        claude: dedup(claude),
        codex: dedup(codex),
        copilot: dedup(default_copilot_dirs(app)),
    }
}

/// Copilot CLI config dirs: ~/.copilot plus any $COPILOT_HOME the app inherited.
fn default_copilot_dirs(app: &AppHandle) -> Vec<String> {
    let mut copilot = Vec::new();
    if let Ok(h) = app.path().home_dir() {
        copilot.push(h.join(".copilot").to_string_lossy().to_string());
    }
    if let Ok(d) = std::env::var("COPILOT_HOME") {
        copilot.push(d);
    }
    copilot
}

fn save_config_dirs(app: &AppHandle, cd: &ConfigDirs) {
    let Some(path) = config_dirs_path(app) else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(s) = serde_json::to_string_pretty(cd) {
        let _ = std::fs::write(path, s);
    }
}

fn install_agent_docs(app: &AppHandle) {
    let dirs = load_config_dirs(app);

    for claude_dir in &dirs.claude {
        let skill_dir = Path::new(claude_dir).join("skills").join("burrow");
        if std::fs::create_dir_all(&skill_dir).is_ok() {
            let _ = std::fs::write(skill_dir.join("SKILL.md"), BURROW_SKILL_MD);
        }
    }

    for codex_dir in &dirs.codex {
        let codex_dir = Path::new(codex_dir);
        if std::fs::create_dir_all(codex_dir).is_ok() {
            let agents = codex_dir.join("AGENTS.md");
            let existing = std::fs::read_to_string(&agents).unwrap_or_default();
            let block = format!("<!-- BURROW:BEGIN -->\n{BURROW_AGENT_DOC}\n<!-- BURROW:END -->");
            let merged = match (existing.find("<!-- BURROW:BEGIN -->"), existing.find("<!-- BURROW:END -->")) {
                (Some(s), Some(e)) if e > s => {
                    let end = e + "<!-- BURROW:END -->".len();
                    format!("{}{}{}", &existing[..s], block, &existing[end..])
                }
                _ if existing.trim().is_empty() => block,
                _ => format!("{}\n\n{block}", existing.trim_end()),
            };
            let _ = std::fs::write(&agents, merged);
        }
    }

    // Copilot CLI: same SKILL.md spec as Claude (it also reads CLAUDE.md), but
    // skills are discovered from dirs listed in `skillDirectories` in
    // <copilot-dir>/settings.json — there's no implicit `skills/` lookup. So we
    // write <copilot-dir>/skills/burrow/SKILL.md AND register <copilot-dir>/skills
    // in skillDirectories (non-destructive merge). This is what makes `/burrow`
    // resolve in a Copilot session.
    for copilot_dir in &dirs.copilot {
        let skills_root = Path::new(copilot_dir).join("skills");
        let skill_dir = skills_root.join("burrow");
        if std::fs::create_dir_all(&skill_dir).is_ok() {
            let _ = std::fs::write(skill_dir.join("SKILL.md"), BURROW_SKILL_MD);
        }
        register_copilot_skill_dir(
            &Path::new(copilot_dir).join("settings.json"),
            &skills_root.to_string_lossy(),
        );
    }
}

// Add `dir` to the `skillDirectories` array in Copilot's settings.json without
// clobbering the user's other settings. Absent/empty file → create it; unparseable
// → skip (never destroy a file we can't read).
fn register_copilot_skill_dir(path: &Path, dir: &str) {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let mut root: serde_json::Value = if existing.trim().is_empty() {
        json!({})
    } else {
        match serde_json::from_str(&existing) {
            Ok(v) => v,
            Err(_) => return,
        }
    };
    let Some(obj) = root.as_object_mut() else { return };
    let arr = obj
        .entry("skillDirectories")
        .or_insert_with(|| json!([]));
    let Some(arr) = arr.as_array_mut() else { return };
    if arr.iter().any(|v| v.as_str() == Some(dir)) {
        return; // already registered
    }
    arr.push(json!(dir));
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(s) = serde_json::to_string_pretty(&root) {
        let _ = std::fs::write(path, s);
    }
}

// Install a persistent status hook into an agent's global hook config (Claude's
// ~/.claude/settings.json, Codex's ~/.codex/hooks.json — same schema). The hook
// runs `burrow hook`, which no-ops unless BURROW_PTY_ID is set (i.e. outside a
// Burrow PTY). Non-destructive: parses existing JSON, only APPENDS our entry to
// each event array, skips if already present, and backs up the original first.
// This is what makes status work for manually-started and reattached agent tabs,
// the same way Superset registers one global notify hook.
fn merge_status_hooks(path: &Path, events: &[&str], cmd: &str) {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    // Only proceed if the file is absent/empty or valid JSON — never clobber a file
    // we can't parse.
    let mut root: serde_json::Value = if existing.trim().is_empty() {
        json!({})
    } else {
        match serde_json::from_str(&existing) {
            Ok(v) => v,
            Err(_) => return,
        }
    };
    if !root.is_object() {
        return;
    }
    if !existing.trim().is_empty() {
        let _ = std::fs::write(path.with_extension("json.burrow-bak"), &existing);
    }

    let obj = root.as_object_mut().unwrap();
    let hooks = obj.entry("hooks").or_insert_with(|| json!({}));
    let Some(hooks) = hooks.as_object_mut() else { return };

    let mut changed = false;
    for ev in events {
        let arr = hooks.entry(*ev).or_insert_with(|| json!([]));
        let Some(arr) = arr.as_array_mut() else { continue };
        // Dedupe: our command is recognizable by mentioning BURROW_PTY_ID + "hook".
        let present = arr.iter().any(|grp| {
            grp.get("hooks")
                .and_then(|h| h.as_array())
                .is_some_and(|hs| {
                    hs.iter().any(|h| {
                        h.get("command")
                            .and_then(|c| c.as_str())
                            .is_some_and(|c| c.contains("BURROW_PTY_ID") && c.contains("hook"))
                    })
                })
        });
        if present {
            continue;
        }
        arr.push(json!({ "hooks": [ { "type": "command", "command": cmd } ] }));
        changed = true;
    }

    if changed {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(s) = serde_json::to_string_pretty(&root) {
            let _ = std::fs::write(path, s);
        }
    }
}

fn install_status_hooks(app: &AppHandle) {
    let Ok(data) = app.path().app_data_dir() else { return };
    let burrow = data.join("bin").join("burrow");
    // Single-quote the path — the macOS app-data dir contains "Application Support".
    let cmd = format!("[ -n \"$BURROW_PTY_ID\" ] && '{}' hook || true", burrow.display());

    let dirs = load_config_dirs(app);

    // Claude: status events. (Notification + PermissionRequest ≈ waiting for the user.)
    let claude_events = ["UserPromptSubmit", "PreToolUse", "PostToolUse", "Stop", "Notification", "PermissionRequest"];
    for d in &dirs.claude {
        merge_status_hooks(&Path::new(d).join("settings.json"), &claude_events, &cmd);
    }

    // Codex: same hook schema, in <codex-dir>/hooks.json.
    let codex_events = ["UserPromptSubmit", "PreToolUse", "PostToolUse", "Stop"];
    for d in &dirs.codex {
        merge_status_hooks(&Path::new(d).join("hooks.json"), &codex_events, &cmd);
    }

    // Copilot CLI: a *separate* schema — its own file per hook config at
    // <copilot-dir>/hooks/<name>.json (NOT merged into a shared settings file),
    // camelCase event names, and command objects keyed by "bash" not "command".
    // Because each event has its own array, we bake the target state straight into
    // the command (`burrow status <state>`) instead of routing through `burrow hook`
    // and parsing Copilot's stdin schema. We own the whole `burrow.json` file, so we
    // write/remove it wholesale rather than merging.
    for d in &dirs.copilot {
        write_copilot_hooks(&Path::new(d).join("hooks").join("burrow.json"), &burrow);
    }
}

// Build + write Copilot's dedicated hooks file. State is embedded per-event; the
// command no-ops outside a Burrow PTY (BURROW_PTY_ID unset).
fn write_copilot_hooks(path: &Path, burrow: &Path) {
    let bash = |state: &str| {
        json!([{
            "type": "command",
            "bash": format!("[ -n \"$BURROW_PTY_ID\" ] && '{}' status {} || true", burrow.display(), state),
            "timeoutSec": 5
        }])
    };
    let doc = json!({
        "version": 1,
        "hooks": {
            "userPromptSubmitted": bash("running"),
            "preToolUse": bash("running"),
            "postToolUse": bash("running"),
            // Copilot fires `notification` when it needs the user (permission/input
            // prompt) — verified empirically: it fires on a permission request but
            // NOT on a normal `--allow-all` turn. Map it to `waiting` so the tab gets
            // the blue need-input dot, mirroring Claude's Notification handling.
            "notification": bash("waiting"),
            "agentStop": bash("done"),
            "sessionEnd": bash("done"),
        }
    });
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(s) = serde_json::to_string_pretty(&doc) {
        let _ = std::fs::write(path, s);
    }
}

// Reverse of merge_status_hooks: drop every hook group whose command is ours
// (recognizable by the BURROW_PTY_ID + "hook" marker), leaving any other hooks —
// including the user's own and Superset's — untouched.
fn unmerge_status_hooks(path: &Path) {
    let Ok(existing) = std::fs::read_to_string(path) else { return };
    let Ok(mut root) = serde_json::from_str::<serde_json::Value>(&existing) else { return };
    let Some(hooks) = root.get_mut("hooks").and_then(|h| h.as_object_mut()) else { return };

    let mut changed = false;
    for (_event, arr) in hooks.iter_mut() {
        let Some(arr) = arr.as_array_mut() else { continue };
        let before = arr.len();
        arr.retain(|grp| {
            !grp.get("hooks")
                .and_then(|h| h.as_array())
                .is_some_and(|hs| {
                    hs.iter().any(|h| {
                        h.get("command")
                            .and_then(|c| c.as_str())
                            .is_some_and(|c| c.contains("BURROW_PTY_ID") && c.contains("hook"))
                    })
                })
        });
        if arr.len() != before {
            changed = true;
        }
    }
    if changed {
        let _ = std::fs::write(path.with_extension("json.burrow-bak"), &existing);
        if let Ok(s) = serde_json::to_string_pretty(&root) {
            let _ = std::fs::write(path, s);
        }
    }
}

fn uninstall_status_hooks(app: &AppHandle) {
    let dirs = load_config_dirs(app);
    for d in &dirs.claude {
        unmerge_status_hooks(&Path::new(d).join("settings.json"));
    }
    for d in &dirs.codex {
        unmerge_status_hooks(&Path::new(d).join("hooks.json"));
    }
    // Copilot: we own the whole file, so just delete it.
    for d in &dirs.copilot {
        let _ = std::fs::remove_file(Path::new(d).join("hooks").join("burrow.json"));
    }
}

/// Re-install the global status hooks (idempotent). Exposed so the UI/CLI can
/// repair them without restarting the app.
#[tauri::command]
fn reinstall_status_hooks(app: AppHandle) {
    ensure_burrow_bin(&app);
    install_status_hooks(&app);
}

/// Remove the global status hooks from the Claude/Codex configs.
#[tauri::command]
fn remove_status_hooks(app: AppHandle) {
    uninstall_status_hooks(&app);
}

/// The agent config dirs hooks/docs are installed into (seeded with defaults + env).
#[tauri::command]
fn get_config_dirs(app: AppHandle) -> ConfigDirs {
    load_config_dirs(&app)
}

/// Persist a new set of agent config dirs, then re-install hooks + docs into them.
/// Returns the cleaned/deduped list that was saved.
#[tauri::command]
fn set_config_dirs(app: AppHandle, claude: Vec<String>, codex: Vec<String>, copilot: Vec<String>) -> ConfigDirs {
    let cd = ConfigDirs { claude: dedup(claude), codex: dedup(codex), copilot: dedup(copilot) };
    save_config_dirs(&app, &cd);
    ensure_burrow_bin(&app);
    install_status_hooks(&app);
    install_agent_docs(&app);
    cd
}

const BURROW_AGENT_DOC: &str = "## Delegating to sub-agents (`burrow`)\n\
You are running inside Burrow, which gives you a `burrow` CLI to delegate work to sub-agents in new terminal tabs. Reach for it whenever the user wants to **delegate to agents**, **spawn an agent to** do something, run work **in parallel**, **fan out** subtasks, or **hand off** a task.\n\n\
- `burrow spawn <command...>` — open a new tab in this project running <command> (fire-and-forget, returns immediately). Example: `burrow spawn claude \"write tests for src/foo\"`.\n\
- `burrow spawn --token t1 claude \"...\"` then later `burrow collect t1` — delegate with a tracking token, keep working, and pull the sub-agent's final message whenever you want (non-blocking). `burrow collect` with no token returns every finished sub-agent.\n\
- `burrow sessions` — list the live sub-agent tabs (or `--count`).\n\
- `burrow spawn --cwd /path claude \"...\"` — run the new tab in a different directory.\n\n\
Do NOT block waiting on sub-agents. Fan out the work, continue your own, then `burrow collect` for a recap. Respect the soft per-workspace concurrency limit `burrow spawn` reports. Sub-agents run interactively on the subscription (never use `claude -p`).";

const BURROW_SKILL_MD: &str = "---\n\
name: burrow\n\
description: Delegate work to sub-agents by spawning new terminal tabs from inside the Burrow IDE. Use when the user asks to run work in parallel, hand a task to another agent, or when you want to fan out independent subtasks and collect their results without blocking.\n\
---\n\n\
# Delegating with `burrow`\n\n\
You are running inside **Burrow**. The `burrow` CLI opens new terminal tabs running sub-agents, so you can delegate work. The model is **fire-and-forget + collect**: spawn agents, keep doing your own work, then pull their results when you want — never sit blocked waiting on them.\n\n\
## Spawn sub-agents (fire-and-forget)\n\
```\n\
burrow spawn claude \"write unit tests for src/foo\"\n\
burrow spawn codex \"refactor the auth module\"\n\
```\n\
Opens a new tab in the **current project** and runs the command interactively. Returns immediately.\n\n\
## Fan out with tokens, then collect (non-blocking)\n\
```\n\
burrow spawn --token a claude \"audit src/auth for bugs\"\n\
burrow spawn --token b claude \"audit src/api for bugs\"\n\
# ...go do your own work in the meantime...\n\
burrow collect a b      # prints results of whichever have FINISHED, and only those\n\
burrow collect          # or: collect every finished sub-agent, no token list\n\
```\n\
`burrow collect` never blocks: it prints the final message of each finished token and **consumes** it, so a later `collect` returns only newly-finished ones. Tokens still running are reported as pending. Loop back and `collect` again later to pick them up — do useful work between calls, don't poll in a tight loop.\n\n\
## Recap pattern\n\
Spawn N agents up front → continue your task → near the end, `burrow collect` (optionally a few times as stragglers finish) → summarize what each returned for the user. You drive the recap; the sub-agents just drop their results for you.\n\n\
## Inspect / other dir\n\
```\n\
burrow sessions            # list live sub-agent tabs (--count for just the number)\n\
burrow spawn --cwd /path/to/other/project claude \"...\"\n\
```\n\n\
## Limits & notes\n\
- **Soft concurrency limit** (per workspace, default 3, set in Burrow Settings): `burrow spawn` prints the current cap. Respect it — don't exceed it. It is advisory, not enforced, so it's on you.\n\
- Sub-agents run **interactively on the subscription**. Never pass `-p`/`--print`; never use the Agent SDK.\n\
- Result capture works for `claude` sub-agents (via its Stop hook). Other agents spawn fine but only return a collectable result once they emit a done signal.\n\
- `burrow wait <token>` still exists (blocks until one finishes) but prefer `collect` so you stay productive instead of blocked.";

// ── Hook HTTP server ──────────────────────────────────────────────────────────

static HOOK_SERVER_PORT: OnceLock<u16> = OnceLock::new();

fn start_hook_server(app: AppHandle) {
    let server = tiny_http::Server::http("127.0.0.1:0").expect("hook server bind failed");
    let port = server.server_addr().to_ip().expect("hook server has no IP addr").port();
    let _ = HOOK_SERVER_PORT.set(port);
    // Publish the port to a stable file so globally-installed agent hooks can find
    // the CURRENT server after an app restart (the port is random each launch).
    if let Ok(data) = app.path().app_data_dir() {
        let _ = std::fs::create_dir_all(&data);
        let _ = std::fs::write(data.join("hook.port"), port.to_string());
    }

    std::thread::spawn(move || {
        for mut req in server.incoming_requests() {
            let mut body = String::new();
            let _ = req.as_reader().read_to_string(&mut body);
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&body) {
                if let (Some(pty_id), Some(state)) =
                    (val["ptyId"].as_u64(), val["state"].as_str())
                {
                    let _ = app.emit(&format!("pty-hook-{pty_id}"), state.to_string());
                }
            }
            let _ = req.respond(tiny_http::Response::empty(200));
        }
    });
}

#[tauri::command]
fn get_hook_server_port() -> u16 {
    *HOOK_SERVER_PORT.get().unwrap_or(&0)
}

// Publish the user's configured max-concurrent-sub-agents to a file the `burrow`
// CLI can read (localStorage lives in the frontend; the CLI can't see it). Same
// file-bridge pattern as hook.port. The limit is a SOFT cap surfaced to agents.
#[tauri::command]
fn set_max_agents(n: u32, app: AppHandle) {
    if let Ok(data) = app.path().app_data_dir() {
        let _ = std::fs::create_dir_all(&data);
        let _ = std::fs::write(data.join("max_agents"), n.max(1).to_string());
    }
}

// ── Daemon management ─────────────────────────────────────────────────────────

fn find_daemon_binary(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    // Dev override
    if let Ok(p) = std::env::var("BURROW_DAEMON_BIN") {
        let path = std::path::PathBuf::from(p);
        if path.exists() { return Ok(path); }
    }
    // Alongside current executable (production bundle)
    if let Ok(exe) = std::env::current_exe() {
        let candidate = exe.parent().unwrap_or(Path::new(".")).join("burrow-daemon");
        if candidate.exists() { return Ok(candidate); }
    }
    // Tauri resource dir (bundled sidecar)
    if let Ok(res) = app.path().resource_dir() {
        for name in &["burrow-daemon", "burrow-daemon-aarch64-apple-darwin", "burrow-daemon-x86_64-apple-darwin"] {
            let c = res.join(name);
            if c.exists() { return Ok(c); }
        }
    }
    Err("burrow-daemon binary not found. Set BURROW_DAEMON_BIN env var in development.".into())
}

fn daemon_ensure(data_dir: &Path, app: &AppHandle) -> Result<Arc<DaemonClient>, String> {
    let socket_path = data_dir.join("daemon.sock");
    let token_path = data_dir.join("daemon.token");

    // Try existing daemon
    if let Ok(token) = std::fs::read_to_string(&token_path) {
        let token = token.trim().to_string();
        if socket_path.exists() {
            let client = Arc::new(DaemonClient::new(socket_path.clone(), token, app.clone()));
            if client.probe() {
                // Reuse only if it's THIS build. The daemon survives app restarts, so
                // after an auto-update the running daemon is the previous version and
                // lacks this build's PTY-level changes (e.g. the login-shell `-l`).
                // On a version mismatch, retire it (kill its published PID) and fall
                // through to spawn a fresh daemon, which removes + rebinds the socket.
                if client.version().as_deref() == Some(DAEMON_PROTO_VERSION) {
                    return Ok(client);
                }
                if let Ok(pid) = std::fs::read_to_string(data_dir.join("daemon.pid")) {
                    if let Ok(pid) = pid.trim().parse::<u32>() {
                        let _ = std::process::Command::new("kill")
                            .arg("-9")
                            .arg(pid.to_string())
                            .status();
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(150));
                let _ = std::fs::remove_file(&socket_path);
            }
        }
    }

    // Reaching here means no usable daemon (probe failed, socket missing, or a
    // version mismatch we already killed). The published PID — if still alive — is
    // therefore an unreachable orphan: it lost the socket but keeps running until
    // reboot, leaking a process + its dead PTYs every time this path is hit. Reap it
    // before spawning a replacement (kill is a no-op if it's already gone).
    if let Ok(pid) = std::fs::read_to_string(data_dir.join("daemon.pid")) {
        if let Ok(pid) = pid.trim().parse::<u32>() {
            let _ = std::process::Command::new("kill").arg("-9").arg(pid.to_string()).status();
        }
    }

    // Spawn new daemon
    let daemon_bin = find_daemon_binary(app)?;
    std::process::Command::new(&daemon_bin)
        .env("BURROW_DATA_DIR", data_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn daemon: {e}"))?;

    // Wait up to 3 s for socket + token
    for _ in 0..30 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if let Ok(token) = std::fs::read_to_string(&token_path) {
            let token = token.trim().to_string();
            if socket_path.exists() {
                let client = Arc::new(DaemonClient::new(socket_path.clone(), token, app.clone()));
                if client.probe() {
                    return Ok(client);
                }
            }
        }
    }
    Err("burrow-daemon did not start in time".into())
}

// ── PTY commands ──────────────────────────────────────────────────────────────

#[tauri::command]
fn create_pty(
    id: u32,
    cwd: String,
    cols: u16,
    rows: u16,
    daemon: State<DaemonState>,
    app: AppHandle,
) -> Result<(), String> {
    let guard = daemon.client.lock().unwrap();
    let client = guard.as_ref().ok_or("daemon not connected")?;

    // Build env for the shell: PATH (with burrow bin), BURROW_* vars
    let mut env = serde_json::Map::new();
    env.insert("TERM".into(), json!("xterm-256color"));
    env.insert("COLORTERM".into(), json!("truecolor"));
    // Some TUIs (e.g. GitHub Copilot CLI) gate their full-screen rendering on a
    // non-empty TERM_PROGRAM — an unset value reads as a dumb/non-interactive
    // terminal and they refuse to draw (blank screen). Real emulators always set
    // it (Apple_Terminal, WarpTerminal, vscode…), so we identify ourselves too.
    env.insert("TERM_PROGRAM".into(), json!("Burrow"));
    env.insert("TERM_PROGRAM_VERSION".into(), json!(env!("CARGO_PKG_VERSION")));

    if let Some(bin_dir) = ensure_burrow_bin(&app) {
        let existing = std::env::var("PATH").unwrap_or_default();
        env.insert("PATH".into(), json!(format!("{}:{}", bin_dir.display(), existing)));
    }
    if let Some(sess) = burrow_session_dir(&app) {
        env.insert("BURROW_SESSION_DIR".into(), json!(sess.to_string_lossy().to_string()));
    }
    env.insert("BURROW_CWD".into(), json!(&cwd));
    // Let any agent's hook/notify program report status back via `burrow status`.
    // BURROW_PTY_ID identifies this tab; the port is read live (env first, then the
    // hook.port file under BURROW_HOME_DIR) so it survives an app restart.
    env.insert("BURROW_PTY_ID".into(), json!(id.to_string()));
    if let Some(port) = HOOK_SERVER_PORT.get() {
        env.insert("BURROW_HOOK_PORT".into(), json!(port.to_string()));
    }
    if let Ok(data) = app.path().app_data_dir() {
        env.insert("BURROW_HOME_DIR".into(), json!(data.to_string_lossy().to_string()));
    }

    let resp = client.cmd(json!({
        "cmd": "CreatePty",
        "pty_id": id,
        "cwd": cwd,
        "cols": cols,
        "rows": rows,
        "env": env,
    }))?;

    if resp["ok"].as_bool() != Some(true) {
        return Err(resp["error"].as_str().unwrap_or("CreatePty failed").to_string());
    }

    // Open data stream: daemon → Tauri event pty-data-{id}
    client.start_stream(id);
    Ok(())
}

#[tauri::command]
fn write_pty(id: u32, data: Vec<u8>, daemon: State<DaemonState>) -> Result<(), String> {
    let guard = daemon.client.lock().unwrap();
    let client = guard.as_ref().ok_or("daemon not connected")?;
    let enc = general_purpose::STANDARD.encode(&data);
    client.cmd(json!({"cmd": "WritePty", "pty_id": id, "data": enc}))?;
    Ok(())
}

#[tauri::command]
fn resize_pty(id: u32, cols: u16, rows: u16, daemon: State<DaemonState>) -> Result<(), String> {
    let guard = daemon.client.lock().unwrap();
    let client = guard.as_ref().ok_or("daemon not connected")?;
    client.cmd(json!({"cmd": "ResizePty", "pty_id": id, "cols": cols, "rows": rows}))?;
    Ok(())
}

/// Kill the PTY in the daemon (called when the user explicitly closes a tab).
#[tauri::command]
fn kill_pty(id: u32, daemon: State<DaemonState>) {
    let guard = daemon.client.lock().unwrap();
    if let Some(client) = guard.as_ref() {
        let _ = client.cmd(json!({"cmd": "KillPty", "pty_id": id}));
        client.stop_stream(id);
    }
}

/// Detach the data stream without killing the PTY (called on app close).
/// The PTY keeps running in the daemon so it can be reattached next session.
#[tauri::command]
fn detach_pty(id: u32, daemon: State<DaemonState>) {
    let guard = daemon.client.lock().unwrap();
    if let Some(client) = guard.as_ref() {
        client.stop_stream(id);
    }
}

#[derive(Serialize)]
pub struct PtySessionInfo {
    pub pty_id: u32,
    pub cwd: String,
    pub title: String,
    pub alive: bool,
}

/// List PTY sessions known to the daemon — used by Terminal.vue to restore tabs.
#[tauri::command]
fn list_pty_sessions(daemon: State<DaemonState>) -> Vec<PtySessionInfo> {
    let guard = daemon.client.lock().unwrap();
    let Some(client) = guard.as_ref() else { return vec![] };
    let Ok(resp) = client.cmd(json!({"cmd": "ListSessions"})) else { return vec![] };
    resp["sessions"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|s| Some(PtySessionInfo {
            pty_id: s["pty_id"].as_u64()? as u32,
            cwd: s["cwd"].as_str()?.to_string(),
            title: s["title"].as_str()?.to_string(),
            alive: s["alive"].as_bool()?,
        }))
        .collect()
}

#[tauri::command]
fn get_pty_foreground(id: u32, daemon: State<DaemonState>) -> String {
    let guard = daemon.client.lock().unwrap();
    let Some(client) = guard.as_ref() else { return String::new() };
    let Ok(resp) = client.cmd(json!({"cmd": "GetForeground", "pty_id": id})) else {
        return String::new()
    };
    resp["process"].as_str().unwrap_or("").to_string()
}

// ── Spawn requests (burrow CLI) ───────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct SpawnRequest {
    pub cmd: String,
    pub token: String,
    pub cwd: String,
}

#[tauri::command]
fn take_spawn_requests(cwd: String, app: AppHandle) -> Vec<SpawnRequest> {
    let mut out = Vec::new();
    let Some(reqdir) = burrow_session_dir(&app).map(|d| d.join("requests")) else {
        return out;
    };
    let Ok(entries) = std::fs::read_dir(&reqdir) else { return out };
    for e in entries.flatten() {
        let d = e.path();
        if !d.is_dir() || !d.join("ready").exists() { continue; }
        let read = |name: &str| std::fs::read_to_string(d.join(name)).unwrap_or_default();
        let ws = read("ws");
        if ws != cwd { continue; }
        let cmd = read("cmd");
        let token = read("token");
        let newcwd = read("cwd");
        let _ = std::fs::remove_dir_all(&d);
        if !cmd.is_empty() {
            out.push(SpawnRequest { cmd, token, cwd: newcwd });
        }
    }
    out
}

// ── Git command ───────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct GitOutput { stdout: String, stderr: String, code: i32 }

fn git_binary() -> &'static str {
    for p in &["/usr/bin/git", "/usr/local/bin/git", "/opt/homebrew/bin/git"] {
        if std::path::Path::new(p).exists() { return p; }
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

// ── Open path in external app ─────────────────────────────────────────────────
// target: "finder" (reveal in Finder/Explorer), "vscode", "zed".
#[cfg(target_os = "macos")]
fn first_existing(paths: &[&str]) -> Option<String> {
    paths.iter().find(|p| std::path::Path::new(p).exists()).map(|p| p.to_string())
}

#[tauri::command]
fn open_path_in(path: String, target: String) -> Result<(), String> {
    // On macOS, `open -a App <folder>` just foregrounds an already-running editor
    // instead of opening the folder. Use the editor's own CLI (which opens the
    // path as a project/workspace), falling back to `open -a` if no CLI is found.
    #[cfg(target_os = "macos")]
    let mut cmd = match target.as_str() {
        "vscode" => {
            match first_existing(&[
                "/opt/homebrew/bin/code",
                "/usr/local/bin/code",
                "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code",
            ]) {
                Some(bin) => { let mut c = std::process::Command::new(bin); c.arg(&path); c }
                None => { let mut c = std::process::Command::new("open"); c.args(["-a", "Visual Studio Code", &path]); c }
            }
        }
        "zed" => {
            match first_existing(&[
                "/opt/homebrew/bin/zed",
                "/usr/local/bin/zed",
                "/Applications/Zed.app/Contents/MacOS/cli",
            ]) {
                Some(bin) => { let mut c = std::process::Command::new(bin); c.arg(&path); c }
                None => { let mut c = std::process::Command::new("open"); c.args(["-a", "Zed", &path]); c }
            }
        }
        _ => { let mut c = std::process::Command::new("open"); c.arg(&path); c }
    };
    #[cfg(target_os = "windows")]
    let mut cmd = match target.as_str() {
        "vscode" => { let mut c = std::process::Command::new("code"); c.arg(&path); c }
        "zed" => { let mut c = std::process::Command::new("zed"); c.arg(&path); c }
        _ => { let mut c = std::process::Command::new("explorer"); c.arg(&path); c }
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = match target.as_str() {
        "vscode" => { let mut c = std::process::Command::new("code"); c.arg(&path); c }
        "zed" => { let mut c = std::process::Command::new("zed"); c.arg(&path); c }
        _ => { let mut c = std::process::Command::new("xdg-open"); c.arg(&path); c }
    };
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}

// ── File system commands ──────────────────────────────────────────────────────

#[derive(Serialize)]
struct DirEntry { name: String, is_dir: bool }

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
    result.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    Ok(result)
}

// ── Workspace commands ────────────────────────────────────────────────────────

#[tauri::command]
fn list_workspaces(db: State<DbState>) -> Result<Vec<Workspace>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT id, name, path, created_at, last_opened FROM workspaces ORDER BY COALESCE(last_opened, 0) DESC, created_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| Ok(Workspace {
            id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
            created_at: row.get(3)?,
            last_opened: row.get(4)?,
        }))
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
    ).map_err(|e| e.to_string())?;
    let id = conn.last_insert_rowid();
    Ok(Workspace { id, name, path, created_at: now, last_opened: None })
}

#[tauri::command]
fn delete_workspace(id: i64, db: State<DbState>) -> Result<(), String> {
    db.conn.lock().unwrap()
        .execute("DELETE FROM workspaces WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn rename_workspace(id: i64, name: String, db: State<DbState>) -> Result<(), String> {
    db.conn.lock().unwrap()
        .execute("UPDATE workspaces SET name = ?1 WHERE id = ?2", rusqlite::params![name, id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn touch_workspace(id: i64, db: State<DbState>) -> Result<(), String> {
    db.conn.lock().unwrap()
        .execute("UPDATE workspaces SET last_opened = ?1 WHERE id = ?2", rusqlite::params![unix_now(), id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ── Terminal tab persistence ───────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TerminalTab {
    pub title: Option<String>,
    pub initial_cmd: Option<String>,
    pub pty_id: Option<u32>,
    pub cwd: Option<String>,
}

#[tauri::command]
fn list_terminal_tabs(workspace_id: i64, db: State<DbState>) -> Result<Vec<TerminalTab>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT title, initial_cmd, pty_id, cwd FROM terminal_tabs WHERE workspace_id = ?1 ORDER BY ord ASC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![workspace_id], |row| Ok(TerminalTab {
            title: row.get(0)?,
            initial_cmd: row.get(1)?,
            pty_id: row.get(2)?,
            cwd: row.get(3)?,
        }))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

#[tauri::command]
fn save_terminal_tabs(
    workspace_id: i64,
    tabs: Vec<TerminalTab>,
    db: State<DbState>,
) -> Result<(), String> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM terminal_tabs WHERE workspace_id = ?1", rusqlite::params![workspace_id])
        .map_err(|e| e.to_string())?;
    for (ord, tab) in tabs.iter().enumerate() {
        tx.execute(
            "INSERT INTO terminal_tabs (workspace_id, ord, title, initial_cmd, pty_id, cwd) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![workspace_id, ord as i64, tab.title, tab.initial_cmd, tab.pty_id, tab.cwd],
        ).map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
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

#[tauri::command]
fn write_text_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

#[tauri::command]
fn read_text_file(path: String) -> String {
    std::fs::read_to_string(&path).unwrap_or_default()
}

#[tauri::command]
fn read_file_base64(path: String) -> Result<String, String> {
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    Ok(general_purpose::STANDARD.encode(&bytes))
}

// ── Skills manager ────────────────────────────────────────────────────────────
// Claude skills live as <claude-dir>/skills/<name>/SKILL.md (YAML frontmatter with
// `name` + `description`). Disabling a skill renames its SKILL.md → SKILL.md.off so
// Claude stops loading it (the dir is ignored without a SKILL.md), reversibly.

#[derive(Serialize)]
struct SkillInfo {
    name: String,
    description: String,
    dir: String,
    enabled: bool,
}

/// Pull `name:`/`description:` out of a SKILL.md YAML frontmatter block. Plain line
/// scan — the frontmatter values are single-line in every skill we ship.
fn parse_skill_frontmatter(md: &str) -> (Option<String>, Option<String>) {
    let mut name = None;
    let mut desc = None;
    let mut in_fm = false;
    for (i, line) in md.lines().enumerate() {
        let t = line.trim();
        if t == "---" {
            if i == 0 { in_fm = true; continue; }
            if in_fm { break; }
        }
        if !in_fm { continue; }
        if let Some(v) = t.strip_prefix("name:") {
            name = Some(v.trim().trim_matches('"').to_string());
        } else if let Some(v) = t.strip_prefix("description:") {
            desc = Some(v.trim().trim_matches('"').to_string());
        }
    }
    (name, desc)
}

#[tauri::command]
fn list_skills(app: AppHandle) -> Vec<SkillInfo> {
    let mut out: Vec<SkillInfo> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for cdir in &load_config_dirs(&app).claude {
        let skills_dir = Path::new(cdir).join("skills");
        let Ok(entries) = std::fs::read_dir(&skills_dir) else { continue };
        for e in entries.filter_map(|e| e.ok()) {
            if !e.file_type().map(|t| t.is_dir()).unwrap_or(false) { continue; }
            let dir = e.path();
            let on = dir.join("SKILL.md");
            let off = dir.join("SKILL.md.off");
            let (enabled, md_path) = if on.exists() {
                (true, on)
            } else if off.exists() {
                (false, off)
            } else {
                continue;
            };
            let dir_str = dir.to_string_lossy().into_owned();
            if !seen.insert(dir_str.clone()) { continue; }
            let md = std::fs::read_to_string(&md_path).unwrap_or_default();
            let (fm_name, fm_desc) = parse_skill_frontmatter(&md);
            out.push(SkillInfo {
                name: fm_name.unwrap_or_else(|| e.file_name().to_string_lossy().into_owned()),
                description: fm_desc.unwrap_or_default(),
                dir: dir_str,
                enabled,
            });
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

#[tauri::command]
fn set_skill_enabled(dir: String, enabled: bool) -> Result<(), String> {
    let on = Path::new(&dir).join("SKILL.md");
    let off = Path::new(&dir).join("SKILL.md.off");
    if enabled {
        if off.exists() { std::fs::rename(&off, &on).map_err(|e| e.to_string())?; }
    } else if on.exists() {
        std::fs::rename(&on, &off).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn delete_skill(dir: String) -> Result<(), String> {
    // Guard: only remove a dir that actually looks like a skill.
    let p = Path::new(&dir);
    if !p.join("SKILL.md").exists() && !p.join("SKILL.md.off").exists() {
        return Err("not a skill directory".into());
    }
    std::fs::remove_dir_all(p).map_err(|e| e.to_string())
}

// ── MCP server manager ────────────────────────────────────────────────────────
// Claude Code stores MCP servers under the top-level `mcpServers` key of
// ~/.claude.json. serde_json's preserve_order feature keeps the rest of that large
// file byte-stable across the round-trip; we only touch `mcpServers`.

fn claude_json_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path().home_dir().map(|h| h.join(".claude.json")).map_err(|e| e.to_string())
}

fn read_claude_json(app: &AppHandle) -> Result<serde_json::Value, String> {
    let path = claude_json_path(app)?;
    let txt = std::fs::read_to_string(&path).unwrap_or_default();
    if txt.trim().is_empty() { return Ok(json!({})); }
    serde_json::from_str(&txt).map_err(|e| e.to_string())
}

fn write_claude_json(app: &AppHandle, root: &serde_json::Value) -> Result<(), String> {
    let path = claude_json_path(app)?;
    if let Ok(prev) = std::fs::read_to_string(&path) {
        if !prev.trim().is_empty() {
            let _ = std::fs::write(path.with_extension("json.burrow-bak"), &prev);
        }
    }
    let s = serde_json::to_string_pretty(root).map_err(|e| e.to_string())?;
    std::fs::write(&path, s).map_err(|e| e.to_string())
}

#[derive(Serialize)]
struct McpServer {
    name: String,
    /// The raw config object, re-serialized to a string for the frontend to display.
    config: String,
}

#[tauri::command]
fn list_mcp_servers(app: AppHandle) -> Result<Vec<McpServer>, String> {
    let root = read_claude_json(&app)?;
    let mut out = Vec::new();
    if let Some(servers) = root.get("mcpServers").and_then(|v| v.as_object()) {
        for (name, cfg) in servers {
            out.push(McpServer {
                name: name.clone(),
                config: serde_json::to_string_pretty(cfg).unwrap_or_default(),
            });
        }
    }
    Ok(out)
}

/// Add or replace an MCP server. `config` is the JSON object as a string (so the
/// frontend can hand over whatever shape the user typed — stdio command, http url…).
#[tauri::command]
fn add_mcp_server(app: AppHandle, name: String, config: String) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() { return Err("server name is required".into()); }
    let cfg: serde_json::Value = serde_json::from_str(config.trim())
        .map_err(|e| format!("config is not valid JSON: {e}"))?;
    if !cfg.is_object() { return Err("config must be a JSON object".into()); }

    let mut root = read_claude_json(&app)?;
    if !root.is_object() { return Err("~/.claude.json is not a JSON object".into()); }
    let obj = root.as_object_mut().unwrap();
    let servers = obj.entry("mcpServers").or_insert_with(|| json!({}));
    let Some(servers) = servers.as_object_mut() else {
        return Err("mcpServers is not an object".into());
    };
    servers.insert(name, cfg);
    write_claude_json(&app, &root)
}

#[tauri::command]
fn remove_mcp_server(app: AppHandle, name: String) -> Result<(), String> {
    let mut root = read_claude_json(&app)?;
    if let Some(servers) = root.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
        servers.remove(&name);
    }
    write_claude_json(&app, &root)
}

/// Save a base64-encoded image (pasted from the clipboard) to a temp file and
/// return its path, so the frontend can type that path into a PTY for an agent
/// (Claude Code et al.) to read. Drag-dropped files already have a real path, so
/// they skip this — only clipboard bytes need persisting.
#[tauri::command]
fn save_temp_image(b64: String, ext: String) -> Result<String, String> {
    let bytes = general_purpose::STANDARD
        .decode(b64.as_bytes())
        .map_err(|e| e.to_string())?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let safe_ext: String = ext
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(5)
        .collect();
    let safe_ext = if safe_ext.is_empty() { "png".into() } else { safe_ext };
    let path = std::env::temp_dir().join(format!("burrow-paste-{nanos}.{safe_ext}"));
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

fn init_db(app: &AppHandle) -> Result<Connection, rusqlite::Error> {
    let data_dir = app.path().app_data_dir().expect("no app data dir");
    std::fs::create_dir_all(&data_dir).ok();
    let conn = Connection::open(data_dir.join("workspaces.db"))?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS workspaces (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT    NOT NULL,
            path        TEXT    NOT NULL UNIQUE,
            created_at  INTEGER NOT NULL,
            last_opened INTEGER
        );
        CREATE TABLE IF NOT EXISTS terminal_tabs (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            workspace_id INTEGER NOT NULL,
            ord          INTEGER NOT NULL,
            title        TEXT,
            initial_cmd  TEXT,
            FOREIGN KEY(workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
        );",
    )?;
    // Idempotent migrations: add new columns (ignored if already present)
    let _ = conn.execute_batch("ALTER TABLE terminal_tabs ADD COLUMN pty_id INTEGER");
    let _ = conn.execute_batch("ALTER TABLE terminal_tabs ADD COLUMN cwd TEXT");
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
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_decorum::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let conn = init_db(app.handle()).expect("DB init failed");
            app.manage(DbState { conn: Mutex::new(conn) });

            // Vertically center the native traffic lights in the 36px titlebar.
            // (--titlebar-height = 36; button cluster ~16px tall → y ≈ 10.)
            #[cfg(target_os = "macos")]
            {
                use tauri_plugin_decorum::WebviewWindowExt;
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.set_traffic_lights_inset(13.0, 10.0);
                    // OS vibrancy intentionally NOT applied: it caused system-wide
                    // lag/freezes on this machine. All themes are opaque, so there
                    // is nothing to frost anyway. (Window is also no longer
                    // transparent — see tauri.conf.json.)
                }
            }

            // Connect to (or spawn) burrow-daemon
            let data_dir = app.path().app_data_dir().expect("no app data dir");
            let client = daemon_ensure(&data_dir, app.handle())
                .map_err(|e| {
                    eprintln!("[burrow] daemon error: {e}");
                    e
                })
                .ok();
            app.manage(DaemonState { client: Mutex::new(client) });

            start_hook_server(app.handle().clone());
            install_agent_docs(app.handle());
            // Write the burrow bin now so the global hook command path is valid even
            // before the first PTY spawn, then register the persistent status hooks.
            ensure_burrow_bin(app.handle());
            install_status_hooks(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            create_pty,
            write_pty,
            resize_pty,
            kill_pty,
            detach_pty,
            list_pty_sessions,
            get_pty_foreground,
            take_spawn_requests,
            reinstall_status_hooks,
            remove_status_hooks,
            get_config_dirs,
            set_config_dirs,
            run_git,
            open_path_in,
            read_dir_shallow,
            list_workspaces,
            create_workspace,
            delete_workspace,
            rename_workspace,
            touch_workspace,
            list_terminal_tabs,
            save_terminal_tabs,
            get_app_version,
            write_text_file,
            read_text_file,
            read_file_base64,
            save_temp_image,
            get_hook_server_port,
            set_max_agents,
            list_skills,
            set_skill_enabled,
            delete_skill,
            list_mcp_servers,
            add_mcp_server,
            remove_mcp_server,
        ])
        .run(tauri::generate_context!())
        .expect("error running tauri application");
}
