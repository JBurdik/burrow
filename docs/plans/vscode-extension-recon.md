# VS Code Extension Reverse Engineering — Feature Recon for Burrow

Source: `~/.vscode/extensions/anthropic.claude-code-2.1.181-darwin-arm64/extension.js` (2.2MB minified), `package.json`, `claude-code-settings.schema.json`.

**Constraint:** Only features compatible with interactive `claude` CLI. No Agents SDK / `claude -p`. Usage/quota API already covered in `vscode-usage-reverse-eng.md` — excluded here.

---

## 1. Session Management (Past Conversations Sidebar)

### 1a. Session List with Metadata
The extension has a dedicated **sessions list sidebar** (`claudeVSCodeSessionsList` webview). Each session row exposes:
- `id` (session UUID)
- `lastModified`, `fileSize`
- `summary`, `customTitle`
- `gitBranch` — the branch the session was started on (read from JSONL sidecar)
- `worktree` — whether this session was in a worktree
- `isCurrentWorkspace` — highlight the current project's sessions

```js
// Confirmed via:
r.map((s) => ({
  id: s.sessionId, lastModified: s.lastModified, fileSize: s.fileSize,
  summary: s.summary, customTitle: s.customTitle, gitBranch: s.gitBranch,
  worktree: xU(s.cwd), isCurrentWorkspace: BHe(s.cwd, this.cwd)
}))
```

**Needs Agents SDK?** No — JSONL files are read directly from `~/.claude/projects/<hash>/`. Burrow's Rust already reads these files.

**Difficulty for Burrow:** Medium. Rust already scans JSONL; just need to expose `gitBranch` (stored in JSONL metadata lines) and add a session-browser panel to the sidebar. The "Past Conversations" panel in Burrow's ClaudeChat should mirror this.

### 1b. Reopen Last Closed Session (`Cmd+Shift+T`)
The extension tracks the last closed session tab and `Cmd+Shift+T` reopens it — only intercepts when a Claude session was the last thing closed, otherwise falls through to VS Code's normal behavior.

```json
// package.json keybinding:
{
  "command": "claude-vscode.reopenClosedSession",
  "key": "cmd+shift+t",
  "when": "config.claudeCode.enableReopenClosedSessionShortcut && claude-vscode.lastClosedWasSession"
}
```

**Difficulty for Burrow:** Easy. Track last closed tab; bind `Cmd+Shift+T` to reopen it when `lastClosedWasSession` state is true.

### 1c. Session Forking (`--fork-session` / `--resume-session-at`)
Sessions can be branched from any prior message. The extension supports:
- `--fork-session` flag to `claude` — creates a new session branching from `forkedFromSession` at message `resumeSessionAt`
- Enables "rewind to this message and try again" UX

```js
if (this.options.forkSession) B.push("--fork-session");
if (this.options.resumeSessionAt) B.push("--resume-session-at", this.options.resumeSessionAt);
// Also via IPC: { type: "fork_conversation", forkedFromSession, resumeSessionAt }
```

**Needs Agents SDK?** The IPC path requires the SDK channel. The CLI flag path (`--fork-session --resume-session-at <msgId>`) works with interactive `claude`.

**Difficulty for Burrow:** Medium. Requires parsing message IDs from JSONL + passing CLI flags when spawning claude. Very powerful "branch conversation" feature.

### 1d. Session Rename
```js
case "rename_session": return this.renameSession(e.request.sessionId, e.request.title, e.request.onlyIfNoCustomTitle)
```

Sessions can be renamed via IPC. The extension also has `generateSessionTitle` which calls the model to auto-title a session. Both are useful.

**Difficulty for Burrow:** Easy for rename (write to JSONL sidecar); Medium for auto-title (requires SDK IPC).

---

## 2. File Diff Viewer (Accept / Reject Proposed Changes)

The extension opens a VS Code diff editor when Claude makes file changes. Key patterns:

```js
// Opens diff with vscode.diff command:
await vscode.commands.executeCommand("vscode.diff", originalUri, modifiedUri, title, { preview: false });

// Accept: fires event { accepted: true }
// Reject: fires event { accepted: false }
// Title bar gets ✓ Accept / ✗ Reject buttons via: "claude-vscode.viewingProposedDiff" context
```

The diff is live-editable — the user can manually modify the proposed content before accepting. The extension handles `autoSave: off` by waiting for explicit file save vs `autoSave: on` where accepting is enough.

**Needs Agents SDK?** Yes — the extension receives file changes over the SDK channel. With interactive `claude`, we can't intercept file writes in real-time.

**Partial alternative for Burrow:** After a turn completes, read git diff and show it. The `/rewind` approach (see §3) is more actionable.

---

## 3. File Checkpointing + `/rewind`

Setting `fileCheckpointingEnabled: true` snapshots files before edits so `/rewind` can restore them. The extension exposes a "rewind code" action per message.

```js
// CLI flag (works without SDK):
"enableFileCheckpointing" // passed to claude process

// Rewind via IPC:
case "rewind_code": {
  let s = await n.query.rewindFiles(userMessageId, { dryRun });
  // returns: { canRewind, filesChanged, insertions, deletions }
}
```

**Needs Agents SDK?** `fileCheckpointingEnabled` is a flag Claude handles internally — Burrow can pass it via `claude --setting fileCheckpointingEnabled=true`. The rewind command itself is `/rewind` typed in the PTY, which works interactively.

**Difficulty for Burrow:** Easy. Just enable the flag when spawning claude. No UI needed — `/rewind` works in the terminal.

---

## 4. Keyboard Shortcuts Worth Stealing

From `package.json`:

| Shortcut | Action |
|----------|--------|
| `Cmd+Escape` | Toggle focus: editor → Claude / Claude → editor |
| `Cmd+Shift+Escape` | Open Claude in new tab |
| `Alt+K` | Insert `@filename#L42-50` mention at cursor position |
| `Cmd+N` (when Claude focused) | New conversation (opt-in: `enableNewConversationShortcut`) |
| `Cmd+Shift+T` | Reopen last closed session tab |

The **Alt+K `@mention` shortcut** is particularly useful: it takes the current editor's file path + selected line range and inserts `@path/to/file.ts#L10-25` into Claude's input. Works with interactive claude (just types into the PTY).

**Difficulty for Burrow:** Easy-Medium. Burrow already has Spotlight (`Cmd+P`). The `@mention` shortcut needs a way to pipe text into the active PTY's stdin — Burrow can do this via `write_pty`.

---

## 5. `@mention` File Attachment

When a user types `@` in the input or presses Alt+K:
1. Extension reads the active editor's file path and selection
2. Constructs `@filepath#L10-25` (line range included if selected)
3. Inserts that into Claude's chat input

```js
// Alt+K handler:
let s = vscode.workspace.asRelativePath(n.fileName), o = i.selection;
if (o.isEmpty) { t.fire(`@${s}`); return; }
let a = o.start.line + 1, c = o.end.line + 1;
let l = a !== c ? `@${s}#${a}-${c}` : `@${s}#${a}`;
t.fire(l);
```

The extension also supports a configurable `fileSuggestion.command` — a shell command that outputs file suggestions for the `@` autocomplete popup.

**Difficulty for Burrow:** Medium. The file tree sidebar is already there. Add a "Copy @mention" context menu item + keyboard shortcut that writes `@relative/path#L1-N` to the active PTY.

---

## 6. Status / Progress Indicators

### 6a. Turn Duration Display
Setting `showTurnDuration: true` shows "Cooked for 2m 15s" after each assistant turn.

```js
showTurnDuration: v.boolean().optional().describe('Show "Cooked for Nm Ns" after each assistant turn')
```

**Difficulty for Burrow:** Easy. Already tracking turn timing via hook events; just display elapsed time in the status dot tooltip or as a transient badge when `done` state fires.

### 6b. Message Timestamps
Setting `showMessageTimestamps: true` stamps each assistant message with its arrival time.

**Difficulty for Burrow:** Easy. Already done for status dots.

### 6c. OSC 9;4 Terminal Progress Bar
Setting `terminalProgressBarEnabled: true` makes claude emit `OSC 9;4` sequences during long operations. These show a progress bar in modern terminals (iTerm2, Ghostty, Kitty).

```js
terminalProgressBarEnabled: v.boolean().optional().describe("Emit OSC 9;4 progress sequences during long operations")
```

**Difficulty for Burrow:** Easy. Pass `--setting terminalProgressBarEnabled=true` when launching claude. The PTY passes these sequences through automatically — no Burrow-side handling needed.

### 6d. Custom Spinner Verbs
```js
spinnerVerbs: v.object({ mode: v.enum(["append", "replace"]), verbs: v.array(v.string()) })
  .optional().describe('Customize spinner verbs. mode: "append" adds verbs to defaults, "replace" uses only your verbs.')
```

Allows changing the "thinking..." / "working..." text during tool calls. Can be set via `--setting spinnerVerbs.mode=append spinnerVerbs.verbs[0]=hacking`.

**Difficulty for Burrow:** Easy. Settings panel addition.

---

## 7. OS Notifications

### 7a. Preferred Notification Channel
Claude supports multiple OS notification backends:
```js
Qze = ["auto", "iterm2", "iterm2_with_bell", "terminal_bell", "kitty", "ghostty", "notifications_disabled"]
```

Setting `preferredNotifChannel` controls which method is used. Burrow already fires native OS notifications (`notifyPermission()`). Could expose this as a setting.

### 7b. Mobile Push Notifications
```js
inputNeededNotifEnabled: "Push to mobile when a permission prompt or question is waiting"
agentPushNotifEnabled: "Allow Claude to push proactive mobile notifications"
```

These are claude.ai account features — claude sends push notifications to the user's phone when a permission prompt appears. No Burrow-side implementation needed; exposing the settings is enough.

**Difficulty for Burrow:** Easy. Settings panel checkboxes that write to `~/.claude/settings.json`.

---

## 8. Interesting Settings Not in Burrow

From `claude-code-settings.schema.json`, settings Burrow could expose in its UI:

| Setting | Description | Value for Burrow |
|---------|-------------|------------------|
| `cleanupPeriodDays` | Retain chat transcripts N days (default 30) | Settings UI |
| `fileCheckpointingEnabled` | Snapshot files before edits (enables `/rewind`) | Default on |
| `showTurnDuration` | Show "Cooked for Nm Ns" after turns | Settings toggle |
| `showMessageTimestamps` | Timestamp each assistant message | Settings toggle |
| `terminalProgressBarEnabled` | OSC 9;4 progress during long ops | Default on for supported terminals |
| `autoCompactEnabled` | Auto-compact when context fills | Settings toggle |
| `autoScrollEnabled` | Auto-scroll to bottom in fullscreen | Settings toggle |
| `spinnerVerbs` | Custom loading text | Power user setting |
| `todoFeatureEnabled` | Enable todo/task tracking panel | Could integrate with Burrow sidebar |
| `promptSuggestionEnabled` | Prompt suggestions below input | Already in Burrow ClaudeChat |
| `awaySummaryEnabled` | Session recap after 5+ min away | Interesting for Burrow |
| `prUrlTemplate` | Custom PR URL format | Git panel feature |
| `footerLinksRegexes` | Regex-matched clickable badges in output | Neat; shows PR/ticket links |
| `defaultShell` | `bash` or `powershell` for `!` commands | Already bash in Burrow |
| `effortLevel` | Persisted effort (low/medium/high/xhigh) | Model settings |
| `attribution.commit` / `.pr` | Customize co-author attribution | Git panel setting |
| `includeGitInstructions` | Include built-in commit/PR workflow instructions | Default on |

---

## 9. Away Summary (Session Recap)

When the user returns after being away 5+ minutes, claude shows a session recap.

```js
awaySummaryEnabled: v.boolean().optional().describe(
  "@internal When false, the session recap (shown when you return after being away for 5+ minutes) is disabled."
)
```

This is claude-internal behavior (no UI hooks needed). Burrow just needs to not disable it. Currently works in PTY automatically.

---

## 10. Session Teleport / Mirror to claude.ai

```js
autoUploadSessions: "Mirror local sessions to claude.ai as view-only (no remote control)"
// CLI flag: --session-mirror
// settings: sessionMirror
```

When enabled, sessions sync to claude.ai for viewing on mobile. The `--session-mirror` flag is passed to the claude process.

**Difficulty for Burrow:** Easy to expose. Pass `--session-mirror` when launching claude for users with this setting. The sync happens inside claude.

---

## 11. Worktree Enhancements

### 11a. Symlink Directories
```js
worktree.symlinkDirectories: "Directories to symlink from main repository to worktrees to avoid disk bloat."
// Examples: "node_modules", ".cache", ".bin"
```

When creating a worktree, claude symlinks `node_modules/` etc. from the main repo. Saves gigabytes.

**Difficulty for Burrow:** Medium. Burrow's `create_worktree` Rust command runs `git worktree add`; after that, create symlinks for each configured directory.

### 11b. Sparse Checkout for Worktrees
```js
worktree.sparsePaths: "Directories to include via git sparse-checkout (cone mode). Dramatically faster in large monorepos."
worktree.baseRef: "'fresh' branches from origin/<default> for clean tree; 'head' includes unpushed commits"
```

**Difficulty for Burrow:** Medium. Add sparse-checkout config to `create_worktree` command.

---

## 12. Remote Control Bridge

The extension supports `toggleRemoteControl` — a WebSocket bridge that lets claude.ai control the local session.

```js
remoteControlAtStartup: "Start Remote Control bridge automatically each session"
isolatePeerMachines: "Require explicit approval before SendMessage can reach a peer session on another machine"
// Enabled via: claude.query.enableRemoteControl(true)
// Returns: { session_url } — the claude.ai URL for this session
```

**Needs Agents SDK?** Yes — requires the SDK channel to toggle. But `--remote-control` / `--rc` CLI flag works interactively.

**Difficulty for Burrow:** Medium. Pass `--rc` flag when launching claude; Burrow could display the returned session URL as a clickable link in the tab header.

---

## 13. Dictation / Voice Input

The extension has a full voice recording implementation:

```js
// Detects: native module (audio-capture.node) OR "rec" (sox) / "arecord"
// Checks microphone permissions on macOS
// Captures 16kHz 16-bit mono raw PCM
// Transcribes and inserts into input
// Command: "claude-vscode.toggleDictation"
```

Audio pipeline: native module > sox `rec` > `arecord` fallback. Transcription happens inside claude's chat UI (not the extension).

**Needs Agents SDK?** Transcription happens inside Claude's chat input UI — this is a claude-internal feature, not extension-specific. For the PTY-based Burrow terminal, this already works if the user types `/dictate` or uses claude's built-in voice.

**Difficulty for Burrow as a feature:** Hard — PTY doesn't have a chat input box to inject into.

---

## 14. Footer Link Badges

```js
footerLinksRegexes: "Extra clickable footer badges that appear when a regex matches turn output"
// Format: [{ type: "regex", pattern: "PR-\\d+", url: "https://github.com/org/repo/pull/{id}", label: "PR #{id}" }]
// At most 5 badges; displaced by newer matches; /clear removes them
```

When Claude mentions a PR number, Jira ticket, Linear issue etc. that matches a user-configured regex, a clickable badge appears. Excellent for dev teams.

**Difficulty for Burrow:** Medium. Parse PTY output against user-configured regexes; show badge chips in the tab status area or below the tab title. Purely frontend.

---

## 15. MCP Management UI

The extension handles MCP approval/deny via UI dialogs. Key message types:
- `list_plugins` / `install_plugin` / `uninstall_plugin` / `set_plugin_enabled`
- `list_marketplaces` / `add_marketplace` / `remove_marketplace` / `refresh_marketplace`
- `enabledMcpjsonServers` / `disabledMcpjsonServers` — persisted in `~/.claude/settings.json`

**Needs Agents SDK?** The listing/enable/disable operations go through the SDK channel. But the underlying data is in `~/.claude/settings.json` which Burrow can read/write directly.

**Difficulty for Burrow:** Medium. Read `~/.claude/settings.json`, present approved/denied MCP servers list, allow toggling. No SDK needed for the settings UI — only for live MCP status during a running session.

---

## 16. Python Environment Auto-Activation

```js
// Detects active Python environment (VirtualEnvironment or Conda)
// Sets VIRTUAL_ENV + prepends bin dir to PATH before launching claude
// Controlled by: claudeCode.usePythonEnvironment (default: true)
```

```js
a.VIRTUAL_ENV = o;
a.PATH = `${c}${path.delimiter}${l}`;
```

**Difficulty for Burrow:** Easy. When `BURROW_CWD` contains a `.venv/` or is in a conda env, detect and set env vars before spawning the PTY shell.

---

## 17. Ctrl+Enter to Send (vs Enter for Newline)

```js
useCtrlEnterToSend: "When enabled, use Ctrl/Cmd+Enter to send prompts instead of just Enter. Allows Enter to create new lines."
```

This is a Claude chat UI setting passed to the webview. For Burrow's ClaudeChat component, this is already in the Vue component. Confirm it's also exposed in the Settings panel.

---

## 18. What Burrow Should Actually Implement

Prioritized by impact vs effort:

### High Priority (Easy + High Value)
1. **`fileCheckpointingEnabled: true` as default** — enables `/rewind` for all users, zero UI needed
2. **`terminalProgressBarEnabled: true`** — progress bar in supported terminals, zero UI needed
3. **Turn duration display** — show "⏱ 2m 15s" in status dot tooltip on `done` state
4. **Session list with `gitBranch`** — parse from JSONL, show in past conversations panel
5. **Alt+K `@mention` shortcut** — get active file from file tree, write `@path#L1-N` into active PTY

### Medium Priority (Worth building)
6. **Footer link badges** — regex-matched clickable badges from PTY output; great for PR/ticket workflows
7. **Reopen last closed session (`Cmd+Shift+T`)** — track last closed tab
8. **Worktree symlink directories** — add `node_modules` symlinking to `create_worktree`
9. **Worktree `baseRef: fresh/head`** setting in the New Worktree dialog
10. **Session mirror toggle** — pass `--session-mirror` flag; show returned URL in tab

### Low Priority / Informational
11. **`prUrlTemplate`** — custom PR URL format for git panel
12. **`footerLinksRegexes`** — power-user setting for link badges
13. **SSH configs** — `sshConfigs` array for remote environments (enterprise)
14. **Remote control `--rc`** — pass flag + show returned session URL
15. **`awaySummaryEnabled`** — already on by default, just don't suppress it

### Skip (Needs SDK or not applicable to PTY)
- Diff viewer (needs SDK IPC for real-time interception) — use git diff after turn instead
- MCP live status (needs SDK channel) — static settings edit is enough
- Jupyter/debugger MCP (VS Code-specific)
- Mobile push notification delivery (claude.ai account feature)
- Voice dictation into PTY (no chat input box to inject into)

---

## Summary Table

| Feature | SDK needed? | Burrow effort | Recommended? |
|---------|-------------|---------------|-------------|
| `fileCheckpointingEnabled` | No | Easy | ✅ Yes, default on |
| `terminalProgressBarEnabled` | No | Easy | ✅ Yes, default on |
| Turn duration in status dot | No | Easy | ✅ Yes |
| Session list + gitBranch | No | Medium | ✅ Yes |
| Alt+K `@mention` shortcut | No | Medium | ✅ Yes |
| Footer link badges | No | Medium | ✅ Yes |
| Cmd+Shift+T reopen session | No | Easy | ✅ Yes |
| Worktree symlink dirs | No | Medium | ✅ Yes |
| Session fork / `--resume-session-at` | CLI only | Medium | ⚠️ Partial |
| Session mirror `--session-mirror` | No | Easy | ✅ Yes |
| Remote control `--rc` | No | Medium | ⚠️ Maybe |
| Diff viewer (accept/reject) | Yes | Hard | ❌ Skip |
| MCP live status UI | Yes | Hard | ❌ Skip |
| Jupyter/debugger | Yes | Hard | ❌ Skip |
