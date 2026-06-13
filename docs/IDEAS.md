# Burrow — Feature Ideas

Inspired by cmux (manaflow-ai/cmux). Grouped by effort.

---

## Easy (quick wins)

### Workspace colors
Custom color per workspace row in the Sidebar. Configurable via context menu. Useful for visual organization when many projects are open.

### Workspace descriptions
Optional multi-line description per workspace, shown as a tooltip or sub-label in the Sidebar. Useful for agent workspaces where context matters.

### `tmux wait-for`
`burrow wait-for <signal>` / `burrow wait-for -S <signal>` — cross-pane synchronization primitive via named semaphore file in `$BURROW_SESSION_DIR`. Lets orchestration scripts coordinate without polling.

### `tmux buffer shim`
Extend the tmux shim with `set-buffer`, `paste-buffer`, `list-buffers` for copy/paste workflows. Named buffer store per session.

---

## Medium

### Directory-scoped config (`.burrow/burrow.json`)
Project-level config file (upward walk from `$BURROW_CWD`). Defines:
- Custom command palette actions for the project
- Workspace color / icon override
- Soft concurrency limit for `burrow spawn`
- Custom agent launch shortcuts

Trust gate: first use of project actions shows a one-time approval prompt (fingerprint stored in `~/.burrow/trusted`).

### Custom command palette actions (Spotlight)
Define project-specific actions in `.burrow/burrow.json` → `actions[]`. Each action has a name, optional icon, and a `burrow spawn` command. They appear in `⌘P` Spotlight alongside built-in commands.

### Notification panel
A collapsible panel (or popover from the sidebar unread badge) listing ALL review-status tabs across workspaces with their titles and timestamps. Click to jump. Currently only the `⌘⇧U` shortcut exists.

### Session restore for non-Claude agents
Extend auto-resume to Codex (`codex --session <id>`) and other agents that support session resume. Requires mapping agent name → resume flag in `Terminal.vue` restore logic.

### Agent hibernation
When live open-tab count exceeds a threshold (e.g. 10), auto-detach idle non-agent PTYs to free daemon resources. Track idle time per leaf. Opt-in via Settings.

---

## Hard (big features)

### Dock (right-sidebar control panel)
Pin persistent TUI commands into the right panel as mini-PTY controls:
- lazygit
- `pnpm test --watch`
- `tail -f` log files
- Any shell command
Each control is a real PTY (small fixed height). Configured in `.burrow/burrow.json` → `dock[]`.
Requires significant UI work (per-control PTY in RightPanel).

### Feed (agent inline decisions)
Right-panel mode for permission requests and Plan-mode decisions from agents. Instead of the agent blocking in the terminal, it pushes the decision to the Feed via a hook. User approves/denies in the UI; response is written back via a file/socket so the agent unblocks.
Requires: hook JSON parsing for `PermissionRequest` → write response file; Feed panel component; XTerm unlisten while waiting.

### `burrow read-screen`
Return current PTY visible text + scrollback as JSON. Lets agents "see" other terminals without screenshot. Requires daemon-side scrollback buffering (deferred since v2.1).

### Event stream (`burrow events`)
`burrow events` streams newline-delimited JSON of all Burrow activity (tab open/close, status changes, logs, spawns). Enables external tools and sub-agents to observe workspace state in real time. Requires a daemon-side pub/sub channel + hook server SSE endpoint.

### SSH remote workspaces
Create a workspace backed by a remote SSH host. PTYs run on the remote machine via `cmuxd`-style daemon. Browser traffic egresses from the remote host. Very hard — requires separate daemon binary, persistent PTY sessions, proxy infra.

---

## Architecture patterns worth borrowing

### V2 JSON-RPC socket protocol
Structured versioned protocol for the daemon socket (alongside current newline-JSON). Enables typed params/responses, error objects, and future extensibility without breaking existing clients.

### Completeness critic pattern
After any large multi-agent fan-out, spawn a final "what did we miss?" agent that reviews the collected results and flags gaps. Already possible with `burrow spawn` + `burrow collect`; worth documenting as a SKILL.md pattern.
