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

impl DaemonState {
    /// Clone the Arc<DaemonClient> and release the mutex immediately. The client
    /// is itself thread-safe (each cmd opens its own connection), so holding the
    /// outer mutex across the network round-trip only serialized unrelated calls
    /// against each other — e.g. a 2s `get_pty_foreground` poll (which shells out
    /// to `ps` daemon-side) would block a fresh `create_pty`, making new terminals
    /// appear to hang on init. Cloning the Arc lets concurrent commands proceed.
    fn client(&self) -> Option<Arc<DaemonClient>> {
        self.client.lock().unwrap().clone()
    }
}

struct DbState {
    conn: Mutex<Connection>,
}

#[derive(Debug, Serialize, Clone)]
struct FloatParams {
    pty_id: u32,
    ws_id: i64,
    title: String,
}

struct FloatParamsState {
    map: Mutex<std::collections::HashMap<String, FloatParams>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    fn from_str(s: &str) -> Corner {
        match s {
            "top-left" => Corner::TopLeft,
            "bottom-left" => Corner::BottomLeft,
            "bottom-right" => Corner::BottomRight,
            _ => Corner::TopRight,
        }
    }
}

/// One float window's layout slot. Sizes are LOGICAL px and tracked here (not
/// queried from the window) so re-stacking is deterministic even right after a
/// window is created, before its size is realized — that race was placing new
/// bubbles off-screen.
#[derive(Clone)]
struct FloatWin {
    label: String,
    w: f64,
    h: f64,
}

/// Float windows all snap to ONE user-chosen corner (a Setting, not where they're
/// dropped) and stack vertically there in insertion order. Dragging just returns
/// a window to that corner.
struct FloatLayoutState {
    corner: Mutex<Corner>,
    wins: Mutex<Vec<FloatWin>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Workspace {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub created_at: i64,
    pub last_opened: Option<i64>,
    pub parent_id: Option<i64>,
    pub worktree_branch: Option<String>,
}

// ── burrow CLI ────────────────────────────────────────────────────────────────

const BURROW_SCRIPT: &str = include_str!("../bin/burrow");
const TMUX_SHIM: &str = include_str!("../bin/tmux");

// Must stay identical to DAEMON_PROTO_VERSION in daemon_main.rs. Bumped only when
// daemon-side PTY behavior changes, so app-only updates don't needlessly restart
// the daemon (which would kill live PTY sessions). A mismatch on launch retires the
// stale daemon so the new behavior takes effect after an auto-update.
const DAEMON_PROTO_VERSION: &str = "2";

// Cached bin dir: written once per app session. Subsequent create_pty calls skip
// the file writes and chmod (2 writes + 2 fsyncs per tab was measurably slow).
static BURROW_BIN_DIR: OnceLock<PathBuf> = OnceLock::new();

fn ensure_burrow_bin(app: &AppHandle) -> Option<&'static PathBuf> {
    if let Some(dir) = BURROW_BIN_DIR.get() {
        return Some(dir);
    }

    let dir = app.path().app_data_dir().ok()?.join("bin");
    std::fs::create_dir_all(&dir).ok()?;

    let script = dir.join("burrow");
    std::fs::write(&script, BURROW_SCRIPT).ok()?;

    let tmux = dir.join("tmux");
    std::fs::write(&tmux, TMUX_SHIM).ok()?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        let _ = std::fs::set_permissions(&script, perms.clone());
        let _ = std::fs::set_permissions(&tmux, perms);
    }

    Some(BURROW_BIN_DIR.get_or_init(|| dir))
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

    // Claude: status events. SessionStart fires when a session (re)starts mid-tab.
    // Notification is telemetry (cmux model: real permission requests come via
    // PermissionRequest, not Notification), so we no longer hook it.
    let claude_events = ["UserPromptSubmit", "PreToolUse", "PostToolUse", "Stop", "SessionStart", "PermissionRequest"];
    for d in &dirs.claude {
        merge_status_hooks(&Path::new(d).join("settings.json"), &claude_events, &cmd);
    }

    // Codex: same hook schema, in <codex-dir>/hooks.json.
    let codex_events = ["UserPromptSubmit", "PreToolUse", "PostToolUse", "Stop", "SessionStart"];
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
- `burrow worktree <branch> [--base-ref REF] [--path DIR]` — create a git worktree off this repo on a new/existing branch; it appears in the Sidebar nested under the repo.\n\
- `burrow spawn --cwd /path claude \"...\"` — run the new tab in a different directory.\n\
- `burrow set-status \"text\"` / `burrow set-status` — show/clear a status label in this tab's header.\n\
- `burrow trigger-flash` — briefly flash this tab as a visual ping to the user.\n\
- `burrow diff --last-turn` — git diff from HEAD at the start of the current agent turn.\n\
- `burrow top` — table of all live Burrow PTY sessions.\n\n\
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
## Worktrees\n\
Create a git worktree off this repo on a new or existing branch — it shows up in the Sidebar nested under the repo, ready to open and run agents in.\n\
```\n\
burrow worktree feat/login                 # new branch off HEAD (or check out existing)\n\
burrow worktree hotfix --base-ref main      # new branch based on main\n\
burrow worktree feat/x --path ~/wt/x        # override the on-disk location\n\
```\n\
Use this to isolate a sub-task on its own branch/checkout instead of sharing the working tree. The worktree's disk path defaults to Burrow's configured worktrees dir.\n\n\
## Status labels, progress & logs\n\
```\n\
burrow set-status \"running tests...\"   # show a label next to this tab's status dot\n\
burrow set-status                        # clear the label\n\
burrow trigger-flash                     # briefly flash this tab (visual ping)\n\
burrow set-progress 0.4 --label \"Building\"  # show a progress bar in the tab (0.0–1.0)\n\
burrow set-progress                      # clear the progress bar\n\
burrow log -- \"Compiled 12 files\"       # append a timestamped log line below the tab bar\n\
burrow log --level warn -- \"Tests slow\" # levels: info (default), warn, error\n\
```\n\
Use these to communicate progress to the user without printing to the terminal.\n\n\
## Inspect what changed this turn\n\
```\n\
burrow diff --last-turn                  # git diff from HEAD at start of this turn\n\
```\n\
Shows exactly what files changed since the user submitted the prompt. Good for a quick sanity-check before reporting done.\n\n\
## Monitor all terminals\n\
```\n\
burrow top                               # table of all live Burrow PTY sessions\n\
```\n\n\
## Inspect / other dir\n\
```\n\
burrow sessions            # list live sub-agent tabs (--count for just the number)\n\
burrow spawn --cwd /path/to/other/project claude \"...\"\n\
```\n\n\
## Limits & notes\n\
- **Soft concurrency limit** (per workspace, default 3, set in Burrow Settings): `burrow spawn` prints the current cap. Respect it — don't exceed it. It is advisory, not enforced, so it's on you.\n\
- Sub-agents run **interactively on the subscription**. Never pass `-p`/`--print`; never use the Agent SDK.\n\
- Result capture works for `claude` sub-agents (via its Stop hook). Other agents spawn fine but only return a collectable result once they emit a done signal.\n\
- `burrow wait <token>` still exists (blocks until one finishes) but prefer `collect` so you stay productive instead of blocked.\n\n\
## Rules — when to use sidebar feedback\n\n\
These rules apply to every task you run inside Burrow. Follow them unless the user explicitly says otherwise.\n\n\
**Status label (`burrow set-status`):**\n\
- Call `burrow set-status \"<phase>\"` at the start of any meaningful work phase (e.g. `\"analyzing\"`, `\"running tests\"`, `\"applying fixes\"`).\n\
- Update the label when the phase changes (e.g. switch from `\"analyzing\"` to `\"running tests\"`).\n\
- Clear it with `burrow set-status` (no arg) when your turn ends so the tab header returns to the agent status dot.\n\
- Keep labels short — one or two words. The user reads them at a glance.\n\n\
**Visual flash (`burrow trigger-flash`):**\n\
- Call `burrow trigger-flash` once, at the very end of a turn, when you have finished a significant task and want to draw the user's attention to this tab (e.g. tests passed, a long refactor completed).\n\
- Do NOT flash mid-turn or on trivial steps — it is a \"done\" signal, not a progress ping.\n\
- Do NOT flash if the turn ended in an error or requires immediate user action (the status dot already signals that).\n\n\
**Diff check (`burrow diff --last-turn`):**\n\
- Before reporting a multi-file change as complete, run `burrow diff --last-turn` internally as a sanity check to confirm the expected files changed.\n\
- You may skip this for single-file edits or when the user's request was purely read-only.\n\n\
**Progress bar (`burrow set-progress`):**\n\
- Use `burrow set-progress <0.0-1.0> --label \"<phase>\"` for tasks with measurable progress (running many tests, compiling many files, processing a list).\n\
- Clear with `burrow set-progress` (no arg) when the task ends.\n\
- Do NOT use for tasks where progress is not measurable — `set-status` suffices.\n\n\
**Log strip (`burrow log`):**\n\
- Use `burrow log -- \"message\"` to record key milestones that are worth keeping visible (e.g. \"Compiled 12 files\", \"3 tests failed\", \"Wrote auth.ts\").\n\
- Use `--level warn` or `--level error` for problems.\n\
- Do NOT log every step — only events the user would want to scroll back and read. Aim for 3-8 log lines per turn max.\n\n\
**Example turn lifecycle:**\n\
```\n\
burrow set-status \"analyzing\"\n\
burrow log -- \"Reading 8 files\"\n\
# ...read files, understand the problem...\n\
burrow set-status \"fixing\"\n\
burrow set-progress 0.0 --label \"Editing\"\n\
# ...make edits, update progress as files done...\n\
burrow set-progress 1.0 --label \"Editing\"\n\
burrow set-status \"testing\"\n\
burrow set-progress 0.0 --label \"Tests\"\n\
# ...run tests...\n\
burrow log -- \"All tests passed\"\n\
burrow set-progress          # clear\n\
burrow diff --last-turn      # quick sanity check\n\
burrow set-status            # clear — turn done\n\
burrow trigger-flash         # ping user: \"this tab finished\"\n\
```";

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
            let url = req.url().to_string();
            let mut body = String::new();
            let _ = req.as_reader().read_to_string(&mut body);

            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&body) {
                match url.as_str() {
                    "/hook" | "/" => {
                        // Agent lifecycle hook → status dot update.
                        if let (Some(pty_id), Some(state)) =
                            (val["ptyId"].as_u64(), val["state"].as_str())
                        {
                            let _ = app.emit(&format!("pty-hook-{pty_id}"), state.to_string());
                        }
                    }
                    "/write" => {
                        // tmux send-keys path: write raw bytes into a PTY.
                        // Frontend (XTerm.vue) listens to `pty-write-{id}` and
                        // forwards to the daemon via write_pty.
                        if let (Some(pty_id), Some(data)) =
                            (val["ptyId"].as_u64(), val["data"].as_str())
                        {
                            let _ = app.emit(
                                &format!("pty-write-{pty_id}"),
                                data.to_string(),
                            );
                        }
                    }
                    "/set-status" => {
                        // burrow set-status: custom label shown in the tab header.
                        if let Some(pty_id) = val["ptyId"].as_u64() {
                            let text = val["text"].as_str().unwrap_or("").to_string();
                            let _ = app.emit(
                                &format!("pty-status-text-{pty_id}"),
                                text,
                            );
                        }
                    }
                    "/flash" => {
                        // burrow trigger-flash: briefly highlight the tab.
                        if let Some(pty_id) = val["ptyId"].as_u64() {
                            let _ = app.emit(&format!("pty-flash-{pty_id}"), ());
                        }
                    }
                    "/open-diff" => {
                        // burrow diff --last-turn: open a diff tab in the terminal UI.
                        if let Some(pty_id) = val["ptyId"].as_u64() {
                            let diff = val["diff"].as_str().unwrap_or("").to_string();
                            let title = val["title"].as_str().unwrap_or("diff: last turn").to_string();
                            let _ = app.emit(
                                &format!("pty-open-diff-{pty_id}"),
                                serde_json::json!({ "diff": diff, "title": title }),
                            );
                        }
                    }
                    "/set-progress" => {
                        // burrow set-progress: show/clear a progress bar in the tab header.
                        if let Some(pty_id) = val["ptyId"].as_u64() {
                            let _ = app.emit(
                                &format!("pty-progress-{pty_id}"),
                                serde_json::json!({
                                    "progress": val["progress"],
                                    "label": val["label"].as_str().unwrap_or("")
                                }),
                            );
                        }
                    }
                    "/log" => {
                        // burrow log: append a timestamped log entry for this tab.
                        if let Some(pty_id) = val["ptyId"].as_u64() {
                            let level = val["level"].as_str().unwrap_or("info").to_string();
                            let message = val["message"].as_str().unwrap_or("").to_string();
                            let _ = app.emit(
                                &format!("pty-log-{pty_id}"),
                                serde_json::json!({ "level": level, "message": message }),
                            );
                        }
                    }
                    "/session-id" => {
                        // burrow hook (UserPromptSubmit): persist Claude session_id for resume.
                        if let Some(pty_id) = val["ptyId"].as_u64() {
                            if let Some(sid) = val["sessionId"].as_str() {
                                let _ = app.emit(
                                    &format!("pty-session-id-{pty_id}"),
                                    sid.to_string(),
                                );
                            }
                        }
                    }
                    _ => {}
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

// Record the ptyId assigned to a tmux shim window ID (@N) so that subsequent
// `tmux send-keys -t @N` calls can look up the right PTY to write to. Called by
// Terminal.vue immediately after creating a tab that was spawned via the tmux shim.
#[tauri::command]
fn register_tmux_win(win_id: String, pty_id: u32, app: AppHandle) {
    let Ok(data) = app.path().app_data_dir() else { return };
    let wins_dir = data.join("tmux_wins");
    let _ = std::fs::create_dir_all(&wins_dir);
    let _ = std::fs::write(wins_dir.join(&win_id), pty_id.to_string());
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
    let client = daemon.client().ok_or("daemon not connected")?;

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
    // Enable Claude Code agent teams; the tmux shim in bin/ makes it functional.
    env.insert("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS".into(), json!("1"));
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
    let client = daemon.client().ok_or("daemon not connected")?;
    let enc = general_purpose::STANDARD.encode(&data);
    client.cmd(json!({"cmd": "WritePty", "pty_id": id, "data": enc}))?;
    Ok(())
}

#[tauri::command]
fn resize_pty(id: u32, cols: u16, rows: u16, daemon: State<DaemonState>) -> Result<(), String> {
    let client = daemon.client().ok_or("daemon not connected")?;
    client.cmd(json!({"cmd": "ResizePty", "pty_id": id, "cols": cols, "rows": rows}))?;
    Ok(())
}

/// Kill the PTY in the daemon (called when the user explicitly closes a tab).
#[tauri::command]
fn kill_pty(id: u32, daemon: State<DaemonState>) {
    if let Some(client) = daemon.client() {
        let _ = client.cmd(json!({"cmd": "KillPty", "pty_id": id}));
        client.stop_stream(id);
    }
}

/// Detach the data stream without killing the PTY (called on app close).
/// The PTY keeps running in the daemon so it can be reattached next session.
#[tauri::command]
fn detach_pty(id: u32, daemon: State<DaemonState>) {
    if let Some(client) = daemon.client() {
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
    let Some(client) = daemon.client() else { return vec![] };
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
    let Some(client) = daemon.client() else { return String::new() };
    let Ok(resp) = client.cmd(json!({"cmd": "GetForeground", "pty_id": id})) else {
        return String::new()
    };
    resp["process"].as_str().unwrap_or("").to_string()
}

// ── System & daemon stats (title-bar dropdown) ────────────────────────────────

#[derive(Serialize)]
pub struct SystemStats {
    /// Aggregate CPU usage across all cores, 0–100.
    pub cpu_percent: f32,
    pub mem_used: u64,
    pub mem_total: u64,
}

/// CPU + RAM usage for the whole machine. CPU needs two samples spaced by the
/// refresh interval, so we sleep briefly between them.
#[tauri::command]
fn system_stats() -> SystemStats {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_cpu_usage();
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    let cpu_percent = sys.global_cpu_usage();
    SystemStats {
        cpu_percent,
        mem_used: sys.used_memory(),
        mem_total: sys.total_memory(),
    }
}

#[derive(Serialize)]
pub struct DaemonStats {
    pub connected: bool,
    pub pid: Option<u32>,
    pub total: usize,
    pub alive: usize,
}

/// Session counts + pid for the live daemon — drives the title-bar dropdown.
#[tauri::command]
fn daemon_stats(daemon: State<DaemonState>, app: AppHandle) -> DaemonStats {
    let pid = app
        .path()
        .app_data_dir()
        .ok()
        .and_then(|d| std::fs::read_to_string(d.join("daemon.pid")).ok())
        .and_then(|s| s.trim().parse::<u32>().ok());
    let Some(client) = daemon.client() else {
        return DaemonStats { connected: false, pid, total: 0, alive: 0 };
    };
    let Ok(resp) = client.cmd(json!({"cmd": "ListSessions"})) else {
        return DaemonStats { connected: false, pid, total: 0, alive: 0 };
    };
    let sessions = resp["sessions"].as_array().cloned().unwrap_or_default();
    let alive = sessions.iter().filter(|s| s["alive"].as_bool() == Some(true)).count();
    DaemonStats { connected: true, pid, total: sessions.len(), alive }
}

/// Kill every dead (non-alive) PTY the daemon still holds. Returns count reaped.
#[tauri::command]
fn clean_daemon(daemon: State<DaemonState>) -> usize {
    let Some(client) = daemon.client() else { return 0 };
    let Ok(resp) = client.cmd(json!({"cmd": "ListSessions"})) else { return 0 };
    let mut reaped = 0;
    for s in resp["sessions"].as_array().cloned().unwrap_or_default() {
        if s["alive"].as_bool() == Some(false) {
            if let Some(pid) = s["pty_id"].as_u64() {
                let _ = client.cmd(json!({"cmd": "KillPty", "pty_id": pid}));
                client.stop_stream(pid as u32);
                reaped += 1;
            }
        }
    }
    reaped
}

/// Hard-restart the daemon: kill its process (taking all live PTYs with it) and
/// spawn a fresh one, swapping the connected client in place. Returns the new pid.
#[tauri::command]
fn restart_daemon(daemon: State<DaemonState>, app: AppHandle) -> Result<u32, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    // Kill the published pid; daemon_ensure reaps any orphan too, but do it up
    // front so the version-match reuse branch can't re-adopt the old process.
    if let Ok(pid) = std::fs::read_to_string(data_dir.join("daemon.pid")) {
        if let Ok(pid) = pid.trim().parse::<u32>() {
            let _ = std::process::Command::new("kill").arg("-9").arg(pid.to_string()).status();
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(150));
    let _ = std::fs::remove_file(data_dir.join("daemon.sock"));
    let client = daemon_ensure(&data_dir, &app)?;
    *daemon.client.lock().unwrap() = Some(client);
    std::fs::read_to_string(data_dir.join("daemon.pid"))
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .ok_or_else(|| "daemon restarted but pid unavailable".into())
}

// ── Spawn requests (burrow CLI) ───────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct SpawnRequest {
    /// "spawn" (open a tab running `cmd`) or "worktree" (create a git worktree workspace).
    pub kind: String,
    pub cmd: String,
    pub token: String,
    pub cwd: String,
    /// worktree only: branch name + optional base ref for a new branch.
    pub branch: String,
    pub base: String,
    /// tmux shim: window ID (@N) assigned by the shim's new-window/split-window command.
    /// Frontend registers ptyId→winId via register_tmux_win so send-keys can find the PTY.
    pub tmux_win: String,
}

#[tauri::command]
fn take_spawn_requests(cwd: String, app: AppHandle, db: State<DbState>) -> Vec<SpawnRequest> {
    let mut out = Vec::new();
    let Some(reqdir) = burrow_session_dir(&app).map(|d| d.join("requests")) else {
        return out;
    };
    let Ok(entries) = std::fs::read_dir(&reqdir) else { return out };
    for e in entries.flatten() {
        let d = e.path();
        if !d.is_dir() || !d.join("ready").exists() { continue; }
        let read = |name: &str| std::fs::read_to_string(d.join(name)).unwrap_or_default();
        let ws = read("ws");          // spawning workspace (request origin)
        let newcwd = read("cwd");     // dir the new tab should run in (may be a worktree)
        // Route the tab to the workspace it will actually run in: prefer the target
        // dir `newcwd` when that dir is itself a workspace (e.g. a worktree), so the
        // tab nests under it; otherwise fall back to the spawning workspace `ws`
        // (covers `spawn --cwd <arbitrary dir>` where the dir is not its own
        // workspace, and `worktree` requests where newcwd is empty). The DB is the
        // arbiter so a single Terminal claims each request — no double-claim race.
        let target = if newcwd.is_empty() { ws.clone() } else { newcwd.clone() };
        let target_is_ws = {
            let conn = db.conn.lock().unwrap();
            conn.query_row(
                "SELECT 1 FROM workspaces WHERE path = ?1",
                rusqlite::params![target],
                |_| Ok(()),
            ).is_ok()
        };
        let claimant = if target_is_ws { &target } else { &ws };
        if *claimant != cwd { continue; }
        // `kind` is absent on legacy `spawn` requests → default to "spawn".
        let kind = { let k = read("kind"); if k.is_empty() { "spawn".to_string() } else { k } };
        let cmd = read("cmd");
        let token = read("token");
        let branch = read("branch");
        let base = read("base");
        let tmux_win = read("tmux_win");
        let _ = std::fs::remove_dir_all(&d);
        match kind.as_str() {
            "worktree" if !branch.is_empty() => {
                out.push(SpawnRequest { kind, cmd: String::new(), token: String::new(), cwd: newcwd, branch, base, tmux_win: String::new() });
            }
            _ if !cmd.is_empty() => {
                out.push(SpawnRequest { kind: "spawn".to_string(), cmd, token, cwd: newcwd, branch: String::new(), base: String::new(), tmux_win });
            }
            _ => {}
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
        .prepare("SELECT id, name, path, created_at, last_opened, parent_id, worktree_branch FROM workspaces ORDER BY COALESCE(last_opened, 0) DESC, created_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| Ok(Workspace {
            id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
            created_at: row.get(3)?,
            last_opened: row.get(4)?,
            parent_id: row.get(5)?,
            worktree_branch: row.get(6)?,
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
    Ok(Workspace { id, name, path, created_at: now, last_opened: None, parent_id: None, worktree_branch: None })
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

// ── Git worktrees ───────────────────────────────────────────────────────────────

fn expand_tilde(app: &AppHandle, p: &str) -> String {
    if let Some(rest) = p.strip_prefix("~") {
        if let Ok(home) = app.path().home_dir() {
            return format!("{}{}", home.display(), rest);
        }
    }
    p.to_string()
}

/// Run git in `repo` with `args`, returning Ok(stdout) on success or Err(stderr) on failure.
fn git_in(repo: &str, args: &[&str]) -> Result<String, String> {
    match std::process::Command::new(git_binary()).args(args).current_dir(repo).output() {
        Ok(out) if out.status.success() => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr).into_owned();
            Err(if err.trim().is_empty() { format!("git exited with {}", out.status.code().unwrap_or(-1)) } else { err })
        }
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
fn create_worktree(
    parent_id: i64,
    branch: String,
    base_ref: Option<String>,
    path: String,
    app: AppHandle,
    db: State<DbState>,
) -> Result<Workspace, String> {
    let branch = branch.trim().to_string();
    if branch.is_empty() {
        return Err("Branch name is required".into());
    }
    // Resolve the parent repo path; `parent_id IS NULL` enforces "no worktree of a worktree".
    let repo: String = {
        let conn = db.conn.lock().unwrap();
        conn.query_row(
            "SELECT path FROM workspaces WHERE id = ?1 AND parent_id IS NULL",
            rusqlite::params![parent_id],
            |row| row.get(0),
        )
        .map_err(|_| "Parent repo not found (or it is itself a worktree)".to_string())?
    };

    let wt_path = expand_tilde(&app, &path);
    if let Some(parent) = std::path::Path::new(&wt_path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    // New-vs-existing branch: probe whether the local branch already exists.
    let exists = git_in(&repo, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{}", branch)]).is_ok();
    if exists {
        git_in(&repo, &["worktree", "add", &wt_path, &branch])?;
    } else {
        let base = base_ref.as_deref().filter(|s| !s.trim().is_empty()).unwrap_or("HEAD");
        git_in(&repo, &["worktree", "add", "-b", &branch, &wt_path, base])?;
    }

    let now = unix_now();
    let id = {
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO workspaces (name, path, created_at, parent_id, worktree_branch) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![branch, wt_path, now, parent_id, branch],
        )
        .map_err(|e| {
            // Roll back the git worktree so a DB collision doesn't leave an orphan on disk.
            let _ = git_in(&repo, &["worktree", "remove", "--force", &wt_path]);
            e.to_string()
        })?;
        conn.last_insert_rowid()
    };

    Ok(Workspace {
        id,
        name: branch.clone(),
        path: wt_path,
        created_at: now,
        last_opened: None,
        parent_id: Some(parent_id),
        worktree_branch: Some(branch),
    })
}

#[tauri::command]
fn remove_worktree(id: i64, force: bool, db: State<DbState>) -> Result<(), String> {
    let (wt_path, repo): (String, String) = {
        let conn = db.conn.lock().unwrap();
        conn.query_row(
            "SELECT w.path, p.path FROM workspaces w JOIN workspaces p ON w.parent_id = p.id WHERE w.id = ?1",
            rusqlite::params![id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| "Worktree not found".to_string())?
    };

    let dir_gone = !std::path::Path::new(&wt_path).exists();
    if dir_gone {
        // Directory removed out from under us — just reconcile git's registry.
        let _ = git_in(&repo, &["worktree", "prune"]);
    } else {
        let mut args = vec!["worktree", "remove", wt_path.as_str()];
        if force {
            args.push("--force");
        }
        if let Err(e) = git_in(&repo, &args) {
            // Surface the failure (e.g. uncommitted changes) so the UI can offer a force retry.
            return Err(e);
        }
    }

    db.conn.lock().unwrap()
        .execute("DELETE FROM workspaces WHERE id = ?1", rusqlite::params![id])
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
    /// The "Terminal N" fallback / user-renamed base title, separate from the
    /// live agent-set title stored in `title`. Added via idempotent migration.
    pub default_title: Option<String>,
    /// Claude Code session_id — set when a UserPromptSubmit hook fires. Used to
    /// auto-resume the conversation on app restart via `claude --resume <id>`.
    pub session_id: Option<String>,
}

#[tauri::command]
fn list_terminal_tabs(workspace_id: i64, db: State<DbState>) -> Result<Vec<TerminalTab>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT title, initial_cmd, pty_id, cwd, default_title, session_id FROM terminal_tabs WHERE workspace_id = ?1 ORDER BY ord ASC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![workspace_id], |row| Ok(TerminalTab {
            title: row.get(0)?,
            initial_cmd: row.get(1)?,
            pty_id: row.get(2)?,
            cwd: row.get(3)?,
            default_title: row.get(4)?,
            session_id: row.get(5)?,
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
            "INSERT INTO terminal_tabs (workspace_id, ord, title, initial_cmd, pty_id, cwd, default_title, session_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![workspace_id, ord as i64, tab.title, tab.initial_cmd, tab.pty_id, tab.cwd, tab.default_title, tab.session_id],
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

// Editor-grade read: distinguishes missing/empty/binary/too-large (unlike
// read_text_file which collapses every error to ""). Rejects binary (NUL byte or
// invalid UTF-8) and oversized files so the editor shows a placeholder instead of
// mounting garbage or freezing the renderer.
#[tauri::command]
fn read_text_file_checked(path: String) -> Result<String, String> {
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    if bytes.len() > 5_000_000 {
        return Err("too-large".into());
    }
    if bytes.contains(&0) {
        return Err("binary".into());
    }
    String::from_utf8(bytes).map_err(|_| "binary".into())
}

#[tauri::command]
fn read_file_base64(path: String) -> Result<String, String> {
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    Ok(general_purpose::STANDARD.encode(&bytes))
}

// ── LSP bridge ────────────────────────────────────────────────────────────────
// The webview can't spawn processes, so a language server runs here as a child
// process and we bridge its stdio JSON-RPC to the frontend: stdout frames →
// `lsp-msg-{id}` events; `lsp_send` writes Content-Length-framed messages to
// stdin. The CodeMirror lsp-client drives the protocol (initialize/didOpen/etc).
struct LspProc {
    stdin: std::process::ChildStdin,
    child: std::process::Child,
}

#[derive(Default)]
struct LspState {
    procs: Mutex<std::collections::HashMap<u32, LspProc>>,
}

// A GUI app launched from Finder inherits a minimal PATH, so language servers
// (node CLIs in the project, rust-analyzer in ~/.cargo/bin, brew binaries) often
// aren't found via PATH alone. Search the usual locations explicitly.
fn resolve_lsp_bin(name: &str, root: &str) -> Option<PathBuf> {
    let p = Path::new(name);
    if p.is_absolute() {
        return if p.exists() { Some(p.to_path_buf()) } else { None };
    }
    let mut dirs: Vec<PathBuf> = vec![Path::new(root).join("node_modules/.bin")];
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.push(home.join(".cargo/bin"));
        dirs.push(home.join(".npm-global/bin"));
        dirs.push(home.join(".local/bin"));
        dirs.push(home.join(".volta/bin"));
    }
    dirs.push(PathBuf::from("/opt/homebrew/bin"));
    dirs.push(PathBuf::from("/usr/local/bin"));
    dirs.push(PathBuf::from("/usr/bin"));
    if let Ok(path) = std::env::var("PATH") {
        dirs.extend(path.split(':').map(PathBuf::from));
    }
    dirs.into_iter().map(|d| d.join(name)).find(|c| c.exists())
}

// A Finder-launched GUI app has a bare PATH, so a node-based server's
// `#!/usr/bin/env node` shebang (and rust-analyzer's tool lookups) can fail to
// find their runtime. Prepend the usual toolchain dirs to the child's PATH.
fn augmented_path(root: &str) -> String {
    let mut parts: Vec<String> = vec![format!("{root}/node_modules/.bin")];
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        for d in [".cargo/bin", ".volta/bin", ".local/bin", ".npm-global/bin"] {
            parts.push(home.join(d).to_string_lossy().into_owned());
        }
    }
    parts.extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"].map(String::from));
    if let Ok(existing) = std::env::var("PATH") {
        parts.push(existing);
    }
    parts.join(":")
}

#[tauri::command]
fn lsp_start(
    app: AppHandle,
    state: State<LspState>,
    id: u32,
    name: String,
    args: Vec<String>,
    root_path: String,
) -> Result<(), String> {
    if state.procs.lock().unwrap().contains_key(&id) {
        return Ok(()); // already running for this id
    }
    let bin = resolve_lsp_bin(&name, &root_path)
        .ok_or_else(|| format!("language server '{name}' not found on PATH"))?;
    let mut child = std::process::Command::new(&bin)
        .args(&args)
        .current_dir(&root_path)
        .env("PATH", augmented_path(&root_path))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let stderr = child.stderr.take().ok_or("no stderr")?;

    // Reader: parse Content-Length framed JSON-RPC, emit one event per message.
    let app2 = app.clone();
    std::thread::spawn(move || {
        use std::io::{BufRead, BufReader, Read};
        let mut reader = BufReader::new(stdout);
        loop {
            let mut content_len: usize = 0;
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => return, // EOF or error
                    Ok(_) => {}
                }
                let t = line.trim_end();
                if t.is_empty() {
                    break; // blank line ends the header block
                }
                if let Some(v) = t.strip_prefix("Content-Length:") {
                    content_len = v.trim().parse().unwrap_or(0);
                }
            }
            if content_len == 0 {
                continue;
            }
            let mut buf = vec![0u8; content_len];
            if reader.read_exact(&mut buf).is_err() {
                return;
            }
            if let Ok(s) = String::from_utf8(buf) {
                let _ = app2.emit(&format!("lsp-msg-{id}"), s);
            }
        }
    });
    // Drain stderr so the server doesn't block on a full pipe.
    std::thread::spawn(move || {
        use std::io::Read;
        let mut s = stderr;
        let mut buf = [0u8; 4096];
        while let Ok(n) = s.read(&mut buf) {
            if n == 0 {
                break;
            }
        }
    });

    state.procs.lock().unwrap().insert(id, LspProc { stdin, child });
    Ok(())
}

#[tauri::command]
fn lsp_send(state: State<LspState>, id: u32, message: String) -> Result<(), String> {
    use std::io::Write;
    let mut guard = state.procs.lock().unwrap();
    let proc = guard.get_mut(&id).ok_or("lsp server not running")?;
    let header = format!("Content-Length: {}\r\n\r\n", message.len());
    proc.stdin
        .write_all(header.as_bytes())
        .and_then(|_| proc.stdin.write_all(message.as_bytes()))
        .and_then(|_| proc.stdin.flush())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn lsp_stop(state: State<LspState>, id: u32) {
    if let Some(mut proc) = state.procs.lock().unwrap().remove(&id) {
        let _ = proc.child.kill();
    }
}

// ── Claude chat bridge ────────────────────────────────────────────────────────
// Spawns `claude` in stream-json mode (the same mechanism as the VSCode extension,
// subscription-legal after the 2026-06-15 headless restriction). Each workspace
// gets its own persistent process (id = workspace_id); the session lives as long
// as the workspace is mounted. Modeled on the LSP bridge above.

// Read ~/.claude/settings.json and keep only stdio MCP servers.
// Remote servers (type=sse/ws/http) cause 30s+ hangs when spawned without a TTY
// because they try to connect to external endpoints that timeout. stdio servers
// spawn a local subprocess and are safe. Servers with no explicit type default to stdio.
fn build_mcp_config() -> String {
    let empty = r#"{"mcpServers":{}}"#.to_string();
    let config_dir = std::env::var("CLAUDE_CONFIG_DIR")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".claude")));
    let settings_path = match config_dir {
        Some(d) => d.join("settings.json"),
        None => return empty,
    };
    let raw = match std::fs::read_to_string(&settings_path) {
        Ok(s) => s,
        Err(_) => return empty,
    };
    let v: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return empty,
    };
    let servers = match v.get("mcpServers").and_then(|s| s.as_object()) {
        Some(m) => m,
        None => return empty,
    };
    // Keep only stdio (or untyped) servers — remote ones hang without a TTY.
    let local: serde_json::Map<String, serde_json::Value> = servers
        .iter()
        .filter(|(_, cfg)| {
            let t = cfg.get("type").and_then(|t| t.as_str()).unwrap_or("stdio");
            t == "stdio"
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    serde_json::to_string(&serde_json::json!({ "mcpServers": local }))
        .unwrap_or(empty)
}

struct ClaudeProc {
    stdin: std::process::ChildStdin,
    child: std::process::Child,
}

#[derive(Default)]
struct ClaudeState {
    procs: Mutex<std::collections::HashMap<u32, ClaudeProc>>,
}

#[tauri::command]
fn claude_start(
    app: AppHandle,
    state: State<ClaudeState>,
    id: u32,
    cwd: String,
    resume_session_id: Option<String>,
    bypass_permissions: Option<bool>,
) -> Result<(), String> {
    if state.procs.lock().unwrap().contains_key(&id) {
        return Ok(());
    }
    let bin = resolve_lsp_bin("claude", &cwd)
        .ok_or_else(|| "claude binary not found (checked ~/.local/bin, homebrew, PATH)".to_string())?;

    let mcp_config = build_mcp_config();

    let perm_mode = if bypass_permissions.unwrap_or(false) { "bypassPermissions" } else { "acceptEdits" };
    let mut args = vec![
        "--output-format".to_string(), "stream-json".to_string(),
        "--verbose".to_string(),
        "--input-format".to_string(), "stream-json".to_string(),
        "--include-partial-messages".to_string(),
        "--permission-mode".to_string(), perm_mode.to_string(),
        "--mcp-config".to_string(), mcp_config,
        "--strict-mcp-config".to_string(),
    ];
    if let Some(sid) = resume_session_id {
        args.push("--resume".to_string());
        args.push(sid);
    }

    // Strip env to minimal set — bare GUI PATH + subscription auth via keychain.
    // ANTHROPIC_API_KEY intentionally empty so subscription OAuth is used.
    let mut env_map = std::collections::HashMap::new();
    for key in ["HOME", "USER", "TMPDIR", "LANG", "CLAUDE_CONFIG_DIR"] {
        if let Ok(v) = std::env::var(key) {
            env_map.insert(key.to_string(), v);
        }
    }
    env_map.insert("PATH".to_string(), augmented_path(&cwd));
    env_map.insert("ANTHROPIC_API_KEY".to_string(), std::env::var("ANTHROPIC_API_KEY").unwrap_or_default());

    let mut child = std::process::Command::new(&bin)
        .args(&args)
        .current_dir(&cwd)
        .envs(&env_map)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn claude: {e}"))?;

    let stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let stderr = child.stderr.take().ok_or("no stderr")?;

    // Reader: one JSON line per event → emit claude-data-{id}
    let app2 = app.clone();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    let t = l.trim().to_string();
                    if t.is_empty() { continue; }
                    let _ = app2.emit(&format!("claude-data-{id}"), t);
                }
                Err(_) => break,
            }
        }
        let _ = app2.emit(&format!("claude-data-{id}"), r#"{"type":"exit"}"#);
    });
    // Drain stderr to prevent pipe stall.
    std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = [0u8; 4096];
        let mut s = stderr;
        while let Ok(n) = s.read(&mut buf) {
            if n == 0 { break; }
        }
    });

    state.procs.lock().unwrap().insert(id, ClaudeProc { stdin, child });
    Ok(())
}

#[tauri::command]
fn claude_send(state: State<ClaudeState>, id: u32, text: String, session_id: Option<String>, images: Option<Vec<String>>) -> Result<(), String> {
    use std::io::Write;
    let mut guard = state.procs.lock().unwrap();
    let proc = guard.get_mut(&id).ok_or("claude not running for this workspace")?;

    let mut content: Vec<serde_json::Value> = vec![];

    // Prepend image blocks — each entry is a data URI "data:<mime>;base64,<data>"
    for data_uri in images.unwrap_or_default() {
        if let Some(rest) = data_uri.strip_prefix("data:") {
            if let Some(comma) = rest.find(',') {
                let meta = &rest[..comma];
                let data = &rest[comma + 1..];
                let media_type = meta.split(';').next().unwrap_or("image/png");
                content.push(serde_json::json!({
                    "type": "image",
                    "source": { "type": "base64", "media_type": media_type, "data": data }
                }));
            }
        }
    }

    content.push(serde_json::json!({ "type": "text", "text": text }));

    let msg = serde_json::json!({
        "type": "user",
        "session_id": session_id.unwrap_or_default(),
        "message": { "role": "user", "content": content }
    });
    let line = msg.to_string() + "\n";
    proc.stdin.write_all(line.as_bytes()).and_then(|_| proc.stdin.flush()).map_err(|e| e.to_string())
}

#[tauri::command]
fn claude_stop(state: State<ClaudeState>, id: u32) {
    if let Some(mut proc) = state.procs.lock().unwrap().remove(&id) {
        let _ = proc.child.kill();
    }
}

// Abort the current turn by sending SIGINT — lets claude finalize gracefully
// (it emits a result event) rather than SIGKILL which just drops the pipe.
// The stdout reader thread will see EOF and emit the exit event normally.
#[tauri::command]
fn claude_abort(state: State<ClaudeState>, id: u32) {
    let guard = state.procs.lock().unwrap();
    if let Some(proc) = guard.get(&id) {
        let pid = proc.child.id();
        drop(guard);
        #[cfg(unix)]
        {
            std::process::Command::new("kill")
                .args(["-INT", &pid.to_string()])
                .spawn()
                .ok();
        }
    }
}

// Write a control_response to claude's stdin (approve/deny a permission prompt).
// Format inferred from control_request event: {type,request_id,request:{type,...}}
#[tauri::command]
fn claude_respond_permission(state: State<ClaudeState>, id: u32, request_id: String, allow: bool) -> Result<(), String> {
    use std::io::Write;
    let mut guard = state.procs.lock().unwrap();
    let proc = guard.get_mut(&id).ok_or("claude not running")?;
    let msg = serde_json::json!({ "type": "control_response", "request_id": request_id, "allow": allow });
    let line = msg.to_string() + "\n";
    proc.stdin.write_all(line.as_bytes()).and_then(|_| proc.stdin.flush()).map_err(|e| e.to_string())
}

// ── Claude account info ───────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
struct ClaudeAccountInfo {
    email: String,
    display_name: String,
    organization_type: String,   // e.g. "claude_max"
    rate_limit_tier: String,     // e.g. "default_claude_max_5x"
    status_text: String,         // raw stdout of `claude status` (for 5h window parsing)
}

#[derive(Default)]
struct AccountInfoCache(Mutex<Option<ClaudeAccountInfo>>);

#[tauri::command]
fn claude_get_account(state: State<AccountInfoCache>, cwd: String) -> ClaudeAccountInfo {
    // Return cached value if already fetched — avoids N concurrent `claude status` spawns.
    if let Some(cached) = state.0.lock().unwrap().clone() {
        return cached;
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let path = std::path::Path::new(&home).join(".claude.json");
    let json: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(json!({}));
    let acct = json.get("oauthAccount").cloned().unwrap_or(json!({}));
    let email = acct.get("emailAddress").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let display_name = acct.get("displayName").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let organization_type = acct.get("organizationType").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let rate_limit_tier = acct.get("organizationRateLimitTier").and_then(|v| v.as_str()).unwrap_or("").to_string();

    // Run `claude status` with a 4s timeout — it can hang on network/TTY detection.
    let status_text = if let Some(bin) = resolve_lsp_bin("claude", &cwd) {
        let mut env_map = std::collections::HashMap::new();
        for key in &["HOME", "USER", "TMPDIR", "LANG"] {
            if let Ok(v) = std::env::var(key) { env_map.insert(key.to_string(), v); }
        }
        env_map.insert("PATH".to_string(), augmented_path(&cwd));
        env_map.insert("ANTHROPIC_API_KEY".to_string(), std::env::var("ANTHROPIC_API_KEY").unwrap_or_default());
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        std::thread::spawn(move || {
            let out = std::process::Command::new(bin)
                .args(["status"])
                .envs(&env_map)
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default();
            let _ = tx.send(out);
        });
        rx.recv_timeout(std::time::Duration::from_secs(4)).unwrap_or_default()
    } else {
        String::new()
    };

    let info = ClaudeAccountInfo { email, display_name, organization_type, rate_limit_tier, status_text };
    *state.0.lock().unwrap() = Some(info.clone());
    info
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
    let _ = conn.execute_batch("ALTER TABLE workspaces ADD COLUMN parent_id INTEGER");
    let _ = conn.execute_batch("ALTER TABLE workspaces ADD COLUMN worktree_branch TEXT");
    let _ = conn.execute_batch("ALTER TABLE terminal_tabs ADD COLUMN default_title TEXT");
    let _ = conn.execute_batch("ALTER TABLE terminal_tabs ADD COLUMN session_id TEXT");
    Ok(conn)
}

// ── Float window ─────────────────────────────────────────────────────────────

#[tauri::command]
fn open_float_window(
    app: AppHandle,
    float_params: State<FloatParamsState>,
    layout: State<FloatLayoutState>,
    pty_id: u32,
    title: String,
    ws_id: i64,
) -> Result<(), String> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    let label = format!("float-{pty_id}");

    if let Some(win) = app.get_webview_window(&label) {
        win.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    // Store params — float window retrieves via get_float_params on mount
    float_params.map.lock().unwrap().insert(
        label.clone(),
        FloatParams { pty_id, ws_id, title },
    );

    let win = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("index.html".into()))
        .title("")
        .inner_size(240.0, 36.0)
        .min_inner_size(180.0, 32.0)
        .always_on_top(true)
        .hidden_title(true)
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .transparent(true)
        .shadow(false)
        .resizable(true)
        .build()
        .map_err(|e| e.to_string())?;

    // Keep the window DECORATED (Overlay) so the standard window buttons exist —
    // tauri_plugin_decorum's on_window_ready installs a resize delegate that
    // re-runs position_traffic_lights() on every resize, and that derefs
    // close.superview() which is NULL on a borderless window → crash. With the
    // buttons present decorum is happy; we just hide them (setHidden is safe on a
    // non-null button, and decorum's delegate only repositions, never un-hides).
    #[cfg(target_os = "macos")]
    {
        use cocoa::appkit::{NSWindow, NSWindowButton, NSWindowCollectionBehavior};
        use cocoa::base::id;
        use objc::{msg_send, sel, sel_impl};
        if let Ok(ptr) = win.ns_window() {
            let ns_win = ptr as id;
            unsafe {
                for btn in [
                    NSWindowButton::NSWindowCloseButton,
                    NSWindowButton::NSWindowMiniaturizeButton,
                    NSWindowButton::NSWindowZoomButton,
                ] {
                    let b: id = ns_win.standardWindowButton_(btn);
                    if !b.is_null() {
                        let _: () = msg_send![b, setHidden: true];
                    }
                }
                // Float across ALL macOS Spaces (and stay visible in fullscreen
                // apps) — the bubble follows you to every desktop. Safe here: raw
                // NSWindow msg_send on the built window works (unlike decorum's
                // crashing traffic-light path).
                let behavior = ns_win.collectionBehavior()
                    | NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
                    | NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary;
                ns_win.setCollectionBehavior_(behavior);
                // Kill the OS window shadow — a decorated window draws a square
                // shadow behind the round/transparent bubble. The pill/panel draw
                // their own CSS shadow instead.
                let _: () = msg_send![ns_win, setHasShadow: false];
            }
        }
    }

    // Register in the layout (collapsed-bar size) and stack/position it
    // deterministically at the configured corner — uses the tracked size, so no
    // off-screen race against the not-yet-realized window size.
    {
        let mut wins = layout.wins.lock().unwrap();
        if !wins.iter().any(|f| f.label == label) {
            wins.push(FloatWin { label: label.clone(), w: 240.0, h: 36.0 });
        }
    }
    reflow(&app, &layout);

    Ok(())
}

#[tauri::command]
fn set_window_size(
    app: AppHandle,
    label: String,
    width: f64,
    height: f64,
    layout: State<FloatLayoutState>,
) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(&label) {
        // LOGICAL so it matches the layout math (Physical would halve on retina).
        win.set_size(tauri::Size::Logical(tauri::LogicalSize { width, height }))
            .map_err(|e| e.to_string())?;
    }
    // Track the new size + re-stack: a collapse/expand changes this window's
    // height, so everything below it shifts.
    {
        let mut wins = layout.wins.lock().unwrap();
        if let Some(e) = wins.iter_mut().find(|f| f.label == label) {
            e.w = width;
            e.h = height;
        }
    }
    reflow(&app, &layout);
    Ok(())
}

#[tauri::command]
fn get_float_params(
    label: String,
    float_params: State<FloatParamsState>,
) -> Option<FloatParams> {
    float_params.map.lock().unwrap().remove(&label)
}

// ── Float layout (corner snapping + vertical stacking) ───────────────────────

const FLOAT_SIDE_INSET: f64 = 12.0;
const FLOAT_TOP_INSET: f64 = 36.0; // clear the menu bar on the top corners
const FLOAT_BOTTOM_INSET: f64 = 14.0;
const FLOAT_GAP: f64 = 10.0;

/// The monitor a float layout should anchor to — the one the MAIN window is
/// currently on (not the system primary), so bubbles land on the screen the user
/// is actually looking at on a multi-monitor setup.
fn float_monitor(app: &AppHandle) -> Option<tauri::Monitor> {
    let w = app
        .get_webview_window("main")
        .or_else(|| app.webview_windows().into_values().next())?;
    w.current_monitor()
        .ok()
        .flatten()
        .or_else(|| w.primary_monitor().ok().flatten())
}

/// Re-stack every float window at the single configured corner, in insertion
/// order, offsetting by each window's tracked height + a gap. Uses the LOGICAL
/// sizes stored in FloatWin (not live queries) so it's correct the instant a
/// window is created.
fn reposition_floats(app: &AppHandle, corner: Corner, wins: &[FloatWin]) {
    let Some(mon) = float_monitor(app) else { return };
    let scale = mon.scale_factor();
    let msize = mon.size().to_logical::<f64>(scale);
    let mpos = mon.position().to_logical::<f64>(scale);

    let mut cum = 0.0;
    for fw in wins {
        let Some(win) = app.get_webview_window(&fw.label) else { continue };
        let (w, h) = (fw.w, fw.h);
        let x = match corner {
            Corner::TopLeft | Corner::BottomLeft => mpos.x + FLOAT_SIDE_INSET,
            Corner::TopRight | Corner::BottomRight => mpos.x + msize.width - w - FLOAT_SIDE_INSET,
        };
        let y = match corner {
            Corner::TopLeft | Corner::TopRight => mpos.y + FLOAT_TOP_INSET + cum,
            Corner::BottomLeft | Corner::BottomRight => {
                mpos.y + msize.height - FLOAT_BOTTOM_INSET - cum - h
            }
        };
        // Clamp fully on-screen (a manually-resized window can exceed its slot;
        // never let it spill off the monitor edge).
        let x = x.max(mpos.x).min((mpos.x + msize.width - w).max(mpos.x));
        let y = y.max(mpos.y).min((mpos.y + msize.height - h).max(mpos.y));
        let _ = win.set_position(tauri::LogicalPosition::new(x, y));
        cum += h + FLOAT_GAP;
    }
}

/// Re-stack using the currently-configured corner + tracked window list.
fn reflow(app: &AppHandle, layout: &FloatLayoutState) {
    let corner = *layout.corner.lock().unwrap();
    let wins = layout.wins.lock().unwrap().clone();
    reposition_floats(app, corner, &wins);
}

/// Setting changed (or initial sync): set the corner all floats snap to + re-stack.
#[tauri::command]
fn set_float_corner(app: AppHandle, corner: String, layout: State<FloatLayoutState>) {
    *layout.corner.lock().unwrap() = Corner::from_str(&corner);
    reflow(&app, &layout);
}

/// Drag-end: the corner is fixed by the Setting, so just return the window to its
/// stack slot at the configured corner.
#[tauri::command]
fn snap_float_window(app: AppHandle, label: String, layout: State<FloatLayoutState>) {
    let _ = label;
    reflow(&app, &layout);
}

/// Called on manual window resize: read the window's real size into the layout
/// (otherwise stacking uses a stale size → overlap/overflow) and re-stack.
#[tauri::command]
fn sync_float_size(app: AppHandle, label: String, layout: State<FloatLayoutState>) {
    let Some(win) = app.get_webview_window(&label) else { return };
    let scale = win.scale_factor().unwrap_or(1.0);
    let Ok(sz) = win.inner_size() else { return };
    let lsz = sz.to_logical::<f64>(scale);
    {
        let mut wins = layout.wins.lock().unwrap();
        if let Some(e) = wins.iter_mut().find(|f| f.label == label) {
            e.w = lsz.width;
            e.h = lsz.height;
        }
    }
    reflow(&app, &layout);
}

#[tauri::command]
fn close_float_window(app: AppHandle, label: String, layout: State<FloatLayoutState>) {
    layout.wins.lock().unwrap().retain(|f| f.label != label);
    if let Some(win) = app.get_webview_window(&label) {
        let _ = win.close();
    }
    reflow(&app, &layout);
}

/// Snapshot handshake routed through Rust app.emit (frontend→frontend `emit`
/// does NOT reliably cross windows; app.emit reaches every window — proven by the
/// pty-hook channel). Float asks; the main XTerm for this pty answers via
/// send_float_snapshot.
#[tauri::command]
fn request_float_snapshot(app: AppHandle, pty_id: u32) {
    let _ = app.emit(&format!("float-snap-req-{pty_id}"), ());
}

#[tauri::command]
fn send_float_snapshot(app: AppHandle, pty_id: u32, data: String, cols: u16, rows: u16) {
    let _ = app.emit(
        &format!("float-snap-{pty_id}"),
        json!({ "data": data, "cols": cols, "rows": rows }),
    );
}

/// Source terminal resized → tell its float mirror the new grid dims so it can
/// match (the shared PTY's resize already triggers the agent to repaint).
#[tauri::command]
fn notify_float_grid(app: AppHandle, pty_id: u32, cols: u16, rows: u16) {
    let _ = app.emit(&format!("float-grid-{pty_id}"), json!({ "cols": cols, "rows": rows }));
}

// ── Claude 5h usage ───────────────────────────────────────────────────────────

/// Parse "2026-06-02T07:06:19.987Z" → ms since Unix epoch. No external crates.
fn iso_to_unix_ms(s: &str) -> Option<u64> {
    let s = s.strip_suffix('Z').unwrap_or(s);
    let (date, time_part) = s.split_once('T')?;
    let mut dp = date.splitn(3, '-');
    let year: u32 = dp.next()?.parse().ok()?;
    let month: u32 = dp.next()?.parse().ok()?;
    let day: u32 = dp.next()?.parse().ok()?;
    let (hms, frac_str) = time_part.split_once('.').unwrap_or((time_part, ""));
    let mut tp = hms.splitn(3, ':');
    let hour: u64 = tp.next()?.parse().ok()?;
    let min: u64 = tp.next()?.parse().ok()?;
    let sec: u64 = tp.next()?.parse().ok()?;

    let leap = |y: u32| (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let mut days: u64 = 0;
    for y in 1970..year {
        days += if leap(y) { 366 } else { 365 };
    }
    let mdays: [u32; 12] = [31, if leap(year) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 0..(month.saturating_sub(1)) as usize {
        days += mdays[m] as u64;
    }
    days += day.saturating_sub(1) as u64;

    let frac_ms: u64 = {
        let trimmed = &frac_str[..frac_str.len().min(3)];
        format!("{:0<3}", trimmed).parse().unwrap_or(0)
    };
    Some(days * 86_400_000 + hour * 3_600_000 + min * 60_000 + sec * 1_000 + frac_ms)
}

/// Scan ~/.claude/projects/**/*.jsonl and aggregate assistant turn data from the
/// last 5 hours. Returns { outputTokens, turnCount } — no external crates needed.
#[tauri::command]
fn claude_usage_5h(app: AppHandle) -> serde_json::Value {
    use std::io::{BufRead, BufReader};

    let home = match app.path().home_dir() {
        Ok(h) => h,
        Err(_) => return json!({ "outputTokens": 0, "turnCount": 0 }),
    };
    let projects_dir = home.join(".claude/projects");

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let cutoff_ms = now_ms.saturating_sub(5 * 3600 * 1000);

    let mut output_tokens: u64 = 0;
    let mut turn_count: u32 = 0;

    let Ok(project_dirs) = std::fs::read_dir(&projects_dir) else {
        return json!({ "outputTokens": 0, "turnCount": 0 });
    };

    for project_entry in project_dirs.flatten() {
        if !project_entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        // Skip files too old to contain recent entries (file mtime heuristic).
        if let Ok(meta) = project_entry.path().metadata() {
            if let Ok(modified) = meta.modified() {
                let mtime_ms = modified
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                if mtime_ms < cutoff_ms {
                    continue;
                }
            }
        }
        let Ok(jsonl_files) = std::fs::read_dir(project_entry.path()) else { continue };
        for file_entry in jsonl_files.flatten() {
            let path = file_entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            // Same mtime heuristic for individual files.
            if let Ok(meta) = path.metadata() {
                if let Ok(modified) = meta.modified() {
                    let mtime_ms = modified
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    if mtime_ms < cutoff_ms {
                        continue;
                    }
                }
            }
            let Ok(f) = std::fs::File::open(&path) else { continue };
            for line in BufReader::new(f).lines().flatten() {
                if !line.contains("\"assistant\"") {
                    continue;
                }
                let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
                if v["type"].as_str() != Some("assistant") {
                    continue;
                }
                let ts_str = match v["timestamp"].as_str() {
                    Some(s) => s,
                    None => continue,
                };
                let ts_ms = match iso_to_unix_ms(ts_str) {
                    Some(ms) => ms,
                    None => continue,
                };
                if ts_ms < cutoff_ms {
                    continue;
                }
                if let Some(tokens) = v["message"]["usage"]["output_tokens"].as_u64() {
                    output_tokens += tokens;
                    turn_count += 1;
                }
            }
        }
    }

    json!({ "outputTokens": output_tokens, "turnCount": turn_count })
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
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            let conn = init_db(app.handle()).expect("DB init failed");
            app.manage(DbState { conn: Mutex::new(conn) });
            app.manage(FloatParamsState { map: Mutex::new(std::collections::HashMap::new()) });
            app.manage(FloatLayoutState {
                corner: Mutex::new(Corner::TopRight),
                wins: Mutex::new(Vec::new()),
            });

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
            app.manage(LspState::default());
            app.manage(ClaudeState::default());
            app.manage(AccountInfoCache::default());

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
            system_stats,
            daemon_stats,
            clean_daemon,
            restart_daemon,
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
            create_worktree,
            remove_worktree,
            list_terminal_tabs,
            save_terminal_tabs,
            get_app_version,
            write_text_file,
            read_text_file,
            read_text_file_checked,
            lsp_start,
            lsp_send,
            lsp_stop,
            claude_start,
            claude_send,
            claude_stop,
            claude_abort,
            claude_respond_permission,
            claude_get_account,
            read_file_base64,
            save_temp_image,
            get_hook_server_port,
            set_max_agents,
            register_tmux_win,
            list_skills,
            set_skill_enabled,
            delete_skill,
            list_mcp_servers,
            add_mcp_server,
            remove_mcp_server,
            open_float_window,
            get_float_params,
            set_window_size,
            snap_float_window,
            sync_float_size,
            close_float_window,
            request_float_snapshot,
            send_float_snapshot,
            notify_float_grid,
            set_float_corner,
            claude_usage_5h,
        ])
        .run(tauri::generate_context!())
        .expect("error running tauri application");
}
