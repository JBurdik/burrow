# Mission Control — feature backlog

Tank-style task dashboard inside Burrow (in-window view, `ui.mode === 'mission'`).
See `docs/mission-control.html` for the architecture and `docs/tank-analysis.html`
for the model it clones.

## Done

- [x] Core: spawn + list + live terminal (attach-on-demand)
- [x] Result capture (read last assistant turn from Claude JSONL transcript)
- [x] Follow-up conversation (write into the live PTY; never kill on done)
- [x] Sequential queue (`maxConcurrent` slots)
- [x] tank-style layout: rail (tasks grouped by workspace) + detail + continue bar
- [x] In-window view (not a separate window) — toggled from the ActivityBar 🚀
- [x] SQLite persistence (`mission_tasks` table, shared `workspaces.db`)
- [x] PTY-id collision fix (seq seeded from DB max + live daemon sessions)
- [x] **#2** Live PTY reconciliation — re-attach still-running PTYs after reload
- [x] **#3** Image attachments — paste/drop in composer → `save_temp_image` → argv
- [x] **#4** Send to tab — kill mission PTY + `claude --resume` in a real Burrow tab
- [x] **#5** Per-task git worktree (composer checkbox "Isolate in a git worktree")
- [x] Composer checkbox "⚠️ Skip permissions" (`--dangerously-skip-permissions`) — run a task unattended without approval prompts (stopgap until structured permissions land)
- [x] **Resume dead task** — `↻ Resume` respawns a mission PTY with `claude --resume <id>` so a restored read-only task goes live again (`resumeTask`)
- [x] **API error surfacing** — `read_claude_outcome` reads `isApiErrorMessage` from the transcript → task flips to `error` with a reason (429 rate-limit / 529 overloaded), instead of a false `done`
- [x] **UI pass** — scope driven by the sidebar's active workspace (`wsStore.active`) instead of a local picker; rail mirrors it read-only; blank "pick a workspace" state when none active; composer dropped the workspace/cwd fields (cwd implied by scope). Chrome migrated from emoji to Phosphor icons.

## Backlog

### #1 — AI auto-titles
On a task's first `done`, feed the result (or prompt+result) to a cheap model and
rename the task from the placeholder (currently the first prompt line). Reuse the
same call to title Burrow terminal tabs.
- Needs an OpenAI-compatible (or Claude) endpoint + a setting to enable/configure.
- tank gates this behind `ai_features_enabled` + `title_model_url`.

### #6 — Live streaming into the detail
Today an assistant turn only appears on `done` (transcript read). Parse the live
PTY output stream (`pty-data-{id}`, already buffered per task) to show the reply
progressively — at minimum a "TURN n · streaming…" marker like tank, ideally the
text as it lands. Tricky: strip ANSI/TUI chrome from claude's interactive render.

### #7 — Schedules (cron)
Per-task (or per-prompt-template) cron expression → auto-spawn on schedule. tank
uses `croniter` + `cron-descriptor`. Burrow options: a Rust cron crate + a tick
loop, or lean on the harness `/schedule` skill. Persist schedules in SQLite.

### #8 — PR button
After a task finishes (esp. a worktree task), one-click open a PR for its branch.
Burrow already has `run_git`; add a Rust command to push + create the PR (gh CLI
or a forge API). tank gates this behind `git_provider`.

### #9 — Notifications ✅ DONE
`done`/`waiting`/`error` transitions → toast (`notifications` store) + sound
(`playSound`, gated by Settings → sounds) + a system notification when the window
is unfocused. Gated by `isWatching(t)` = mission view up + task selected + window
focused, so a transition you're already looking at stays quiet. In `wireTask`'s
hook handler (notify only on a real change into the state) + `captureResult` for
the API-error case.

### Permission requests / AskUserQuestion
Today a task runs claude in a raw interactive PTY. Permission prompts and the
AskUserQuestion picker render **only in the live terminal modal** (xterm) — the
detail/conversation view is blind to them, so a task can silently hang at
`waiting` until the user opens the terminal and answers by hand.
- **Stopgap shipped:** composer "⚠️ Skip permissions" checkbox → `--dangerously-skip-permissions`.
- **Cheap next step (B):** when a task goes `waiting`, show a banner in the detail
  ("Claude needs input — open Terminal") so the hang is at least legible.
- **Proper fix (C):** move tasks to **stream-json** mode (like `ClaudeChat.vue`),
  where permission requests + questions arrive as structured events → render a
  native Allow/Deny UI in the detail and answer via the existing
  `claude_respond_permission` command. Also unlocks live streaming (#6) and is the
  path to subsuming claude-ui.

## Other ideas (unprioritized)

- Follow-up image attachments (composer images work; continue-bar doesn't yet).
- Live-stream replay buffer for reconciled tasks (currently attach is live-only —
  no scrollback after an app restart, since the buffer was in-memory).
- Per-workspace task filter / collapse groups in the rail.
- Delete-worktree-on-task-delete for isolated tasks (currently the worktree
  lingers; offer cleanup).
- Bulk actions (stop all, clear all done).
- Export a task's conversation to markdown.
