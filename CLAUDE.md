# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

**Burrow** — a Tauri v2 desktop app (macOS-first) that wraps real PTYs in a multi-workspace IDE shell, designed to run AI coding agents (Claude Code, Aider, Codex, etc.) side-by-side in terminal tabs. The product name is **Burrow**; the repo/package name is `agentic-ide`.

Stack: Vue 3 + Pinia + xterm.js frontend, Rust/Tauri backend, SQLite for persistence.

## Commands

```bash
# Frontend-only dev (browser, no Tauri)
pnpm dev

# Full Tauri dev (native window, hot-reload)
pnpm tauri:dev

# Type-check + production build
pnpm build          # vue-tsc + vite build
pnpm tauri:build    # full native bundle

# Rust only
cd src-tauri && cargo check
cd src-tauri && cargo build
```

No test suite exists yet.

## Architecture

### Frontend (`src/`)

**Pinia stores** are the backbone — components talk to stores, not each other:

| Store | Owns |
|-------|------|
| `workspace` | List of workspaces (SQLite-backed via Tauri invoke), which one is active, which are "opened" (PTYs kept alive) |
| `terminalTabs` | Lightweight mirror of each workspace's tab list for the Sidebar; the real Terminal component is source of truth |
| `agents` | Configurable agent presets (command, args, shortcut, color) persisted to `localStorage` |
| `ui` | Settings panel open/close, font + scale preferences (persisted to `localStorage`) |
| `terminal` | Legacy simple terminal store (mostly superseded by XTerm.vue) |
| `fileTree` | File tree state for the sidebar |
| `git` | Git status / diff for the right panel |

**Component hierarchy:**
```
App.vue
  TitleBar
  Settings (overlay)
  Sidebar              ← workspace list + nested tab list from terminalTabs store
  [resize handle]
  Terminal             ← one per opened workspace (kept mounted, hidden when inactive)
    TerminalSplitView  ← manages split panes
      XTerm.vue        ← wraps xterm.js, owns PTY lifecycle via Tauri invoke
  [resize handle]
  RightPanel           ← file tree + git panel
  Spotlight            ← ⌘P command palette
```

**Key keyboard shortcuts:** `⌘,` settings, `⌘P` spotlight.

### PTY / Agent state machine (`XTerm.vue`)

Each `XTerm` creates a native PTY in Rust (`create_pty`), streams bytes via a Tauri event `pty-data-{id}`, and sends input back via `write_pty`.

Agent state (running / waiting for input / done) is detected two ways:
1. **Global persistent hooks (primary)** — at startup `install_status_hooks` (`lib.rs`) merges a status hook into each agent's own global config: Claude `~/.claude/settings.json`, Codex `~/.codex/hooks.json` (same schema). The hook command is `[ -n "$BURROW_PTY_ID" ] && '<app-data>/bin/burrow' hook || true` — a **no-op outside Burrow** (BURROW_PTY_ID unset). Inside a Burrow PTY, `burrow hook` reads the hook JSON on stdin, maps `hook_event_name` → state (`UserPromptSubmit`/`PreToolUse`/`PostToolUse`→running, `Stop`/`SessionEnd`→done, `Notification`→running/done **by message**: the idle "waiting for your input" ping → done since the turn is over, anything else → waiting) and `burrow status <state>` POSTs `{ptyId,state}` to a local `tiny_http` server (`start_hook_server`). The server re-emits Tauri event `pty-hook-{id}`; `XTerm.vue` listens → emits ONE semantic `agentState` (`running`/`waiting`/`done`) which `Terminal.vue`'s `onAgentState` turns into a clean status transition (a single event has no ordering hazard, so a trailing `waiting` can't clobber a fresh `done`). **Because the hooks are global + env-driven, status works for every agent session — launched-by-button, typed by hand, or reattached after restart.** The merge is non-destructive (appends, dedupes by marker, writes a `.burrow-bak`). Port survives restart: `burrow status` reads `BURROW_HOOK_PORT` else `<BURROW_HOME_DIR>/hook.port`.
   - Per-tab result capture (`burrow wait`) still needs a per-launch `--settings` with a `Stop→burrow capture <token>` hook, since the token is unique to a spawned sub-agent. That's the **only** remaining per-launch injection.
2. **Polling fallback** — every 2 s, `get_pty_foreground` → title only for agent processes. For an agent foreground proc the poll **never fabricates `busy`** (an agent stays foreground whether thinking or idle at its prompt — equating presence with busy was the old stuck-orange bug). It drives `busy` only for plain commands (`npm test`, `vim`), and clears state when the shell returns to foreground (rescues a Ctrl+C'd agent with no `done` hook).

**Status surfacing** (`Terminal.vue`): each leaf carries `status: idle|running|waiting|done|review`. On turn-finish, `settleDone()` checks `isWatching(tab)` (workspace visible + tab active + window focused): watching → transient `done` (lime, 4 s auto-clear); not watching → **`review`** (green pulse, persists until the tab is seen via `markTabSeen`). `tabStatus()` priority: waiting > running > review > done > idle. Surfaced as status dots in the tab bar + Sidebar (Superset-style "agent finished while you were away").

### `burrow` CLI (`src-tauri/bin/burrow`)

A POSIX `sh` script embedded in the Rust binary (`include_str!`) and written to `<app-data>/bin/burrow` on each PTY spawn (`ensure_burrow_bin`), with that dir prepended to the shell's `PATH` and `BURROW_SESSION_DIR=<app-data>/sessions` exported. Lets an agent delegate work to sub-agents in new tabs — subscription-safe (launches `claude` **interactively**, never `claude -p` / Agent SDK).

**Transport is file-based, NOT the OSC channel.** Claude's Bash tool and hooks run subprocesses with **no controlling tty**, so `> /dev/tty` fails (`Device not configured`) — the OSC trick can't reach the PTY from there. Instead `burrow spawn` drops a request dir that the frontend polls.

Subcommands:
- `burrow spawn [--token T] [--cwd DIR] <cmd...>` — writes a request dir `<session>/requests/req.XXXXXX/` with raw `cmd`/`token`/`cwd`/`ws` files + a `ready` marker (written last, to avoid reading a half-written request). The command is re-quoted (program name bare so XTerm's `claude` check matches; args single-quoted) so it re-parses correctly when typed into the new tab.
- `burrow wait <token> [--timeout S]` — blocks until `<session>/<token>.done` appears, prints `<token>.result`.
- `burrow capture <token>` — internal; run by the spawned Claude's **Stop hook** (only when the tab has a `resultToken`). Reads the Stop-hook JSON on stdin, extracts the last assistant message from the transcript (via `node`, always present), writes `<token>.result` + `<token>.done`, then **also calls `burrow status done`** — the per-launch `--settings` Stop hook takes precedence over the global `burrow hook` Stop in Claude Code, so without this a spawned sub-agent's status dot would stick orange after it finished. tty-independent.
- `burrow status <running|waiting|done>` — POSTs `{ptyId,state}` to the hook server. Port from the live `<BURROW_HOME_DIR>/hook.port` file (authoritative — rewritten every app launch) else `BURROW_HOOK_PORT` env. (Env-first was a bug: a daemon-reattached PTY carries a stale baked-in port and POSTs to a dead server.) The generic multi-agent status channel.
- `burrow sessions [--count]` — list the live PTY sessions the daemon is holding (or just their count). Talks the daemon's newline-JSON socket protocol (`Auth` then `ListSessions`) via `python3`, reading `daemon.sock` + `daemon.token` from `BURROW_HOME_DIR`.
- `burrow hook` — internal; invoked by the **globally-installed** Claude/Codex status hooks. Reads hook JSON on stdin, maps `hook_event_name` → `burrow status`. `sed`-based, no `node`/`jq`.
- `burrow notify <json>` — internal; legacy Codex `notify`-program path (maps `"type"`). Retained as a fallback; the global `~/.codex/hooks.json` hook is now primary.

`BURROW_*` env exported into every PTY: `BURROW_SESSION_DIR`, `BURROW_CWD`, `BURROW_PTY_ID`, `BURROW_HOOK_PORT`, `BURROW_HOME_DIR` (app-data dir, also holds `hook.port`).

Frontend: each `Terminal.vue` polls `take_spawn_requests(cwd)` every 1 s; the Rust command claims (deletes) only request dirs whose `ws` matches that workspace's cwd → routes the new tab to the right workspace. `--cwd` sets the new tab's own dir via `Leaf.cwd`. (`XTerm.vue` still parses an `OSC 9999;spawn` sequence as a latent direct-PTY path, but the CLI no longer emits it.)

**Agent docs install** (`install_agent_docs`, called once at Tauri `setup`): teaches agents the CLI. Claude → global skill `~/.claude/skills/burrow/SKILL.md`; Codex → managed `<!-- BURROW:BEGIN/END -->` block in `~/.codex/AGENTS.md` (non-destructive merge). Doc strings are `BURROW_SKILL_MD` / `BURROW_AGENT_DOC` consts in `lib.rs`.

**Status hooks install** (`install_status_hooks`, also at `setup`): merges the `burrow hook` status hook into `~/.claude/settings.json` + `~/.codex/hooks.json` via `merge_status_hooks` (parse → append-if-absent → `.burrow-bak`). Skips files it can't parse. This is what gives every agent session a status dot. Reverse via `unmerge_status_hooks` (drops only entries matching the `BURROW_PTY_ID`+`hook` marker, leaving the user's/Superset's hooks). Exposed as Tauri commands `reinstall_status_hooks` / `remove_status_hooks` for repair/teardown without a restart.

### Backend (`src-tauri/src/lib.rs`)

All Tauri commands are in `lib.rs`. Key areas:
- **PTY management** — `create_pty`, `write_pty`, `resize_pty`, `kill_pty`, `get_pty_foreground` using `portable-pty`
- **SQLite** (`rusqlite`, bundled) — `workspaces` and `terminal_tabs` tables; DB lives in `<app-data>/workspaces.db`
- **Git** — `run_git` wraps the system git binary (checks known paths)
- **FS** — `read_dir_shallow`, `write_text_file`

### OSC escape sequence protocol

| Sequence | Direction | Meaning |
|----------|-----------|---------|
| `\x1b]9998;running\x07` | PTY → app | Claude hook: processing user prompt |
| `\x1b]9998;waiting\x07` | PTY → app | Claude hook: waiting for user input |
| `\x1b]9998;done\x07` | PTY → app | Claude hook: turn complete |

OSC 9998 status writes go to `/dev/tty` with `2>/dev/null || true` (tolerated when no tty; status then falls back to `get_pty_foreground` polling). **`burrow spawn`/`wait`/`capture` do NOT use OSC** — they exchange files in `BURROW_SESSION_DIR` (`requests/` dirs in, `<token>.result`/`.done` out), because agent subprocesses have no controlling tty. `XTerm.vue` retains a latent `OSC 9999;spawn` parser but nothing emits it.

## Documentation (`docs/`)

Standalone HTML reference pages (no build step — open directly in a browser). Keep these in sync when you change the corresponding code:

| File | Covers | Update when |
|------|--------|-------------|
| `docs/context.html` | Whole-project overview: architecture, features, key files, Tauri commands, shortcuts | Adding/removing a component, store, Tauri command, agent, or shortcut |
| `docs/burrow.html` | The `burrow` CLI: spawn/wait/capture, OSC 9999 flow, result capture, agent-docs install | Changing the `burrow` script, OSC 9999 format, or `install_agent_docs` |
| `docs/superset-concept/index.html` | Concept study: how superset-sh/superset detects terminal/agent status (HTTP lifecycle hooks vs Burrow's OSC 9998 channel) | Reference only — reverse-engineered comparison, update if porting the hook model into Burrow |

`assets/` holds logos (`logo.png`, `burrowlogo-CUTOUT.png`). `index.html` is the **Vite app entry**, not documentation — do not treat it as a docs page.

## Plans (`docs/plans/`)

Feature plans and implementation notes live in `docs/plans/`. Read the relevant plan before starting a feature batch. Current plans:

| File | Covers |
|------|--------|
| `docs/plans/burrow-features-2026-06-02.md` | Status dots bug, tab reorder, Ctrl+1-9 tabs, ⌘1-9 workspace switch, project icons, git branch in title bar |
