# ACP Chat Transport Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add ACP (Agent Client Protocol, Zed Industries) as an opt-in chat transport for Claude and as the primary transport for Gemini CLI, enabling any ACP-compatible agent to render through the same `ClaudeChat.vue` components without touching the existing stream-json path.

**Architecture:** A thin `AgentTransport` TS interface normalises both paths into identical `AgentEvent` types; `ClaudeChat.vue` switches on a `transport` prop. Rust adds a parallel `AcpState` (mirrors `ClaudeState`) with four commands: `acp_start`, `acp_send`, `acp_stop`, `acp_respond_permission`. The ACP handshake (`initialize` → `session/new`) runs synchronously in `spawn_blocking` before the reader thread is born; ongoing `session/update` notifications and blocking `session/request_permission` requests are forwarded as Tauri events; the existing Vue permission-banner components handle both.

**Tech Stack:** Vue 3 + Pinia + Tauri v2 (Rust), TypeScript, newline-delimited JSON-RPC 2.0 (ACP wire format), `@agentclientprotocol/claude-agent-acp` (npm, Node ≥ 22), `gemini --acp` CLI.

## Global Constraints

- Claude stream-json path (`claude_start` / `claude_send` / `claude_respond_control`) is **unchanged** — ACP is strictly additive.
- ACP uses **NDJSON over stdio** (one JSON-RPC 2.0 object per line, `\n`-terminated, no embedded newlines). **Not** LSP Content-Length headers.
- Claude ACP adapter: `npx @agentclientprotocol/claude-agent-acp` (Node ≥ 22). Uses Claude subscription OAuth (`claude auth login --claudeai`). **`ANTHROPIC_API_KEY` must be blank** so subscription auth flows through — same as current `claude_start`.
- `@zed-industries/claude-agent-acp` (a different, older package) requires an API key and is **NOT subscription-safe** — never use it here.
- Gemini invocation: `gemini --acp` (the `--experimental-acp` alias still works but is deprecated; use `--acp`).
- Phase 1 advertises `fs: false, terminal: false` in ACP `initialize` so no `fs/*` or `terminal/*` callbacks arrive — agents use their own file tools instead.
- No new npm dependencies in the Vue frontend — ACP parsing is pure TypeScript.

---

## ACP Wire Protocol Reference

Below is the exact data needed during implementation.

### Framing
Each JSON-RPC 2.0 message is one `\n`-terminated UTF-8 line on stdio. Client writes to agent stdin; agent writes to client stdout.

### initialize (client → agent, id=0)
```json
{
  "jsonrpc": "2.0", "id": 0, "method": "initialize",
  "params": {
    "protocolVersion": 1,
    "clientCapabilities": {
      "fs": { "readTextFile": false, "writeTextFile": false },
      "terminal": false
    },
    "clientInfo": { "name": "burrow", "title": "Burrow", "version": "2.16.0" }
  }
}
```
Response shape: `{ "protocolVersion": 1, "agentCapabilities": {...}, "authMethods": [...], "agentInfo": {...} }` — we only need to check for errors.

### session/new (client → agent, id=1)
```json
{
  "jsonrpc": "2.0", "id": 1, "method": "session/new",
  "params": { "cwd": "/path/to/project", "mcpServers": [] }
}
```
Response: `{ "sessionId": "sess_abc123" }` — store this.

### session/prompt (client → agent, id=N)
```json
{
  "jsonrpc": "2.0", "id": 2, "method": "session/prompt",
  "params": {
    "sessionId": "sess_abc123",
    "prompt": [{ "type": "text", "text": "fix the login bug" }]
  }
}
```
Response (when turn ends): `{ "stopReason": "end_turn" }`. `stopReason` values: `end_turn | max_tokens | max_turn_requests | refusal | cancelled`.

### session/update (agent → client, notification — no id)
Discriminated by `update.sessionUpdate`:

| `sessionUpdate` value | Meaning | Key fields |
|---|---|---|
| `agent_message_chunk` | Text streamed to user | `messageId`, `content.text` |
| `agent_thought_chunk` | Thinking/reasoning | `content.text` |
| `tool_call` | Tool invocation started | `toolCallId`, `title`, `kind`, `status: "pending"` |
| `tool_call_update` | Tool progress / result | `toolCallId`, `status: "in_progress"\|"completed"\|"failed"`, `content[]` |
| `usage_update` | Token usage | `used`, `size`, `cost.amount` |
| `plan` | Plan entries | `entries[]` |
| others | No-op for Phase 1 | — |

```json
// Text chunk example
{
  "jsonrpc": "2.0", "method": "session/update",
  "params": {
    "sessionId": "sess_abc123",
    "update": {
      "sessionUpdate": "agent_message_chunk",
      "messageId": "msg_001",
      "content": { "type": "text", "text": "Here's the fix: " }
    }
  }
}
```

### session/request_permission (agent → client, HAS id — blocks until response)
```json
{
  "jsonrpc": "2.0", "id": 5, "method": "session/request_permission",
  "params": {
    "sessionId": "sess_abc123",
    "toolCall": { "toolCallId": "call_001" },
    "options": [
      { "optionId": "allow-once", "name": "Allow once", "kind": "allow_once" },
      { "optionId": "reject-once", "name": "Reject", "kind": "reject_once" }
    ]
  }
}
```
Client MUST respond (allow):
```json
{ "jsonrpc": "2.0", "id": 5, "result": { "outcome": { "outcome": "selected", "optionId": "allow-once" } } }
```
Client MUST respond (deny/cancel):
```json
{ "jsonrpc": "2.0", "id": 5, "result": { "outcome": { "outcome": "cancelled" } } }
```

### session/cancel (client → agent, notification — no id)
```json
{ "jsonrpc": "2.0", "method": "session/cancel", "params": { "sessionId": "sess_abc123" } }
```

---

## File Structure

**Create:**
- `src/lib/agentTransport.ts` — `AgentEvent` union + `PermissionOption` + `PermissionDecision` types (no runtime, pure TS)
- `src/lib/acpParser.ts` — `parseAcpUpdate(params): AgentEvent | null`

**Modify:**
- `src-tauri/src/lib.rs` — add `AcpProc`, `AcpState`, 4 Tauri commands; register state + commands in `setup`
- `src/stores/claudeChats.ts` — add `agentKind: 'claude' | 'gemini'` and `transport: 'stream-json' | 'acp'` to `ClaudeSession`; update `create()`, `persist()`, `sync()`
- `src/components/ClaudeChat.vue` — add `transport` + `agentKind` props; conditional listen/send/respond branches

---

## Task 1: AgentTransport types

**Files:**
- Create: `src/lib/agentTransport.ts`

**Interfaces:**
- Produces: `AgentEvent`, `PermissionOption`, `PermissionDecision` — consumed by `acpParser.ts` (Task 2) and `ClaudeChat.vue` (Task 4)

- [ ] **Step 1: Create `src/lib/agentTransport.ts`**

```typescript
// Normalised event types emitted by any agent transport.
// ClaudeChat.vue's onLine() produces these from stream-json text;
// acpParser.ts produces them from ACP session/update params.
export type AgentEvent =
  | { kind: 'text_chunk';         messageId: string; text: string }
  | { kind: 'thinking_chunk';     text: string }
  | { kind: 'tool_call';          toolCallId: string; title: string }
  | { kind: 'tool_output';        toolCallId: string; output: string; done: boolean }
  | { kind: 'permission_request'; requestId: string; toolCallId: string; options: PermissionOption[] }
  | { kind: 'turn_done';          stopReason: string; inputTokens?: number; outputTokens?: number; costUsd?: number }
  | { kind: 'session_id';         sessionId: string }

export interface PermissionOption {
  optionId: string
  name: string
  kind: string   // "allow_once" | "allow_always" | "reject_once" | "reject_always"
}

export type PermissionDecision =
  | { type: 'selected'; optionId: string }
  | { type: 'cancelled' }
```

- [ ] **Step 2: Verify type-check passes**

Run: `pnpm build`
Expected: no type errors related to agentTransport.ts

- [ ] **Step 3: Commit**

```bash
git add src/lib/agentTransport.ts
git commit -m "feat: add AgentEvent types for transport-agnostic chat"
```

---

## Task 2: ACP session/update parser

**Files:**
- Create: `src/lib/acpParser.ts`

**Interfaces:**
- Consumes: `AgentEvent` from `src/lib/agentTransport.ts`
- Produces: `parseAcpUpdate(params: unknown): AgentEvent | null` — consumed by ClaudeChat.vue (Task 4)
  Also produces: `parseAcpPermRequest(raw: unknown): { rpcId: number; requestId: string; toolCallId: string; options: PermissionOption[] } | null`

- [ ] **Step 1: Create `src/lib/acpParser.ts`**

```typescript
import type { AgentEvent, PermissionOption } from './agentTransport'

// Maps ACP session/update notification params → normalised AgentEvent.
// Returns null for updates that don't correspond to a chat message
// (usage_update, plan, available_commands_update, current_mode_update, etc.).
export function parseAcpUpdate(params: unknown): AgentEvent | null {
  const p = params as Record<string, unknown>
  const u = p?.update as Record<string, unknown> | undefined
  if (!u) return null
  const disc = u.sessionUpdate as string

  switch (disc) {
    case 'agent_message_chunk': {
      const c = u.content as Record<string, unknown> | undefined
      return {
        kind: 'text_chunk',
        messageId: (u.messageId as string) ?? 'msg',
        text: (c?.text as string) ?? ''
      }
    }
    case 'agent_thought_chunk': {
      const c = u.content as Record<string, unknown> | undefined
      return { kind: 'thinking_chunk', text: (c?.text as string) ?? '' }
    }
    case 'tool_call':
      return {
        kind: 'tool_call',
        toolCallId: u.toolCallId as string,
        title: (u.title as string) ?? 'Tool'
      }
    case 'tool_call_update': {
      const status = u.status as string
      if (status !== 'completed' && status !== 'failed') return null
      const blocks = (u.content as Array<Record<string, unknown>>) ?? []
      const text = blocks
        .map(b => {
          const inner = b.content as Record<string, unknown> | undefined
          return inner?.type === 'text' ? String(inner.text ?? '') : ''
        })
        .filter(Boolean)
        .join('\n')
      return {
        kind: 'tool_output',
        toolCallId: u.toolCallId as string,
        output: text,
        done: status === 'completed'
      }
    }
    default:
      return null
  }
}

// Parses a raw ACP session/request_permission JSON-RPC request line.
// The agent sends this as a *request* (has id) that blocks until we respond.
export function parseAcpPermRequest(raw: unknown): {
  rpcId: number
  sessionId: string
  toolCallId: string
  options: PermissionOption[]
} | null {
  const msg = raw as Record<string, unknown>
  if (msg.method !== 'session/request_permission') return null
  const rpcId = msg.id as number
  const p = msg.params as Record<string, unknown>
  const toolCall = p?.toolCall as Record<string, unknown> | undefined
  const options = ((p?.options ?? []) as Array<Record<string, unknown>>).map(o => ({
    optionId: o.optionId as string,
    name: o.name as string,
    kind: o.kind as string
  }))
  return {
    rpcId,
    sessionId: p?.sessionId as string,
    toolCallId: toolCall?.toolCallId as string,
    options
  }
}
```

- [ ] **Step 2: Verify type-check**

Run: `pnpm build`
Expected: passes with no errors

- [ ] **Step 3: Commit**

```bash
git add src/lib/acpParser.ts
git commit -m "feat: add ACP session/update and request_permission parsers"
```

---

## Task 3: Rust ACP process management

**Files:**
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces Tauri commands: `acp_start`, `acp_send`, `acp_stop`, `acp_respond_permission`
- Emits Tauri events: `acp-data-{id}` (raw JSON line: session/update notifications + turn-done responses), `acp-req-{id}` (raw JSON line: session/request_permission)

- [ ] **Step 1: Add `AcpProc` and `AcpState` structs**

Insert directly below the `ClaudeState` struct (~line 2385 in lib.rs):

```rust
struct AcpProc {
    stdin: Arc<Mutex<std::process::ChildStdin>>,
    child: std::process::Child,
    // Monotonically increasing id counter for outgoing JSON-RPC requests
    next_id: Arc<std::sync::atomic::AtomicU64>,
    session_id: String,
}

#[derive(Default)]
struct AcpState {
    procs: Mutex<std::collections::HashMap<u32, AcpProc>>,
}
```

Add `use std::sync::{Arc, Mutex};` at the top of lib.rs if not already present (check — `Mutex` is already used in `ClaudeState`).

- [ ] **Step 2: Add `acp_start` command**

Insert after `claude_respond_control` function (~line 2626):

```rust
#[tauri::command]
async fn acp_start(
    app: AppHandle,
    state: State<'_, AcpState>,
    id: u32,
    kind: String,  // "claude" | "gemini"
    cwd: String,
) -> Result<(), String> {
    if state.procs.lock().unwrap().contains_key(&id) {
        return Ok(());
    }

    let (bin_name, extra_args): (&str, Vec<String>) = match kind.as_str() {
        "gemini" => ("gemini", vec!["--acp".into()]),
        _        => ("npx",    vec!["@agentclientprotocol/claude-agent-acp".into()]),
    };

    let bin = resolve_lsp_bin(bin_name, &cwd)
        .ok_or_else(|| format!("{bin_name} not found on PATH"))?;

    let mut env_map = std::collections::HashMap::new();
    for key in ["HOME", "USER", "TMPDIR", "LANG"] {
        if let Ok(v) = std::env::var(key) { env_map.insert(key.to_string(), v); }
    }
    env_map.insert("PATH".to_string(), augmented_path(&cwd));
    // Blank API key → subscription auth flows through (same as claude_start)
    env_map.insert("ANTHROPIC_API_KEY".to_string(), String::new());

    let mut child = std::process::Command::new(&bin)
        .args(&extra_args)
        .current_dir(&cwd)
        .envs(&env_map)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn {bin_name} failed: {e}"))?;

    let mut raw_stdin = child.stdin.take().ok_or("no stdin")?;
    let raw_stdout = child.stdout.take().ok_or("no stdout")?;

    // ACP handshake — synchronous I/O, done before the reader thread starts
    let cwd_clone = cwd.clone();
    let (stdin_back, stdout_back, session_id) = tokio::task::spawn_blocking(
        move || -> Result<(std::process::ChildStdin, std::process::ChildStdout, String), String> {
            use std::io::{BufRead, Write};

            let write_rpc = |stdin: &mut std::process::ChildStdin, msg: serde_json::Value| -> Result<(), String> {
                let line = msg.to_string() + "\n";
                stdin.write_all(line.as_bytes()).and_then(|_| stdin.flush()).map_err(|e| e.to_string())
            };

            // Read lines until a response with the given id appears.
            // Discards notifications and other messages (handshake is silent).
            let read_response = |reader: &mut std::io::BufReader<std::process::ChildStdout>, target_id: u64| -> Result<serde_json::Value, String> {
                let mut line = String::new();
                loop {
                    line.clear();
                    reader.read_line(&mut line).map_err(|e| e.to_string())?;
                    let t = line.trim();
                    if t.is_empty() { continue; }
                    let v: serde_json::Value = serde_json::from_str(t).map_err(|e| e.to_string())?;
                    if v.get("id").and_then(|i| i.as_u64()) == Some(target_id) {
                        if let Some(err) = v.get("error") { return Err(format!("ACP error: {err}")); }
                        return Ok(v.get("result").cloned().unwrap_or(serde_json::Value::Null));
                    }
                    // Discard notifications during handshake
                }
            };

            let mut reader = std::io::BufReader::new(raw_stdout);

            // Step 1: initialize
            write_rpc(&mut raw_stdin, serde_json::json!({
                "jsonrpc": "2.0", "id": 0, "method": "initialize",
                "params": {
                    "protocolVersion": 1,
                    "clientCapabilities": {
                        "fs": { "readTextFile": false, "writeTextFile": false },
                        "terminal": false
                    },
                    "clientInfo": { "name": "burrow", "title": "Burrow", "version": "2.16.0" }
                }
            }))?;
            read_response(&mut reader, 0)?;

            // Step 2: session/new
            write_rpc(&mut raw_stdin, serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": "session/new",
                "params": { "cwd": cwd_clone, "mcpServers": [] }
            }))?;
            let new_result = read_response(&mut reader, 1)?;
            let session_id = new_result
                .get("sessionId").and_then(|s| s.as_str())
                .ok_or("session/new: missing sessionId")?
                .to_string();

            Ok((raw_stdin, reader.into_inner(), session_id))
        }
    )
    .await
    .map_err(|e| format!("handshake thread panicked: {e}"))??;

    let stdin_arc = Arc::new(Mutex::new(stdin_back));
    let stdin_for_thread = stdin_arc.clone();

    // Reader thread: routes ongoing ACP stream to Tauri events
    let app2 = app.clone();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(stdout_back);
        for line in reader.lines() {
            let Ok(raw) = line else { break };
            let raw = raw.trim().to_string();
            if raw.is_empty() { continue; }
            let Ok(msg) = serde_json::from_str::<serde_json::Value>(&raw) else { continue };

            let method = msg.get("method").and_then(|m| m.as_str()).map(|s| s.to_string());
            let has_id  = msg.get("id").is_some();
            let has_err = msg.get("error").is_some();

            match method.as_deref() {
                Some("session/update") => {
                    // Notification (no id) — text chunks, tool calls, etc.
                    let _ = app2.emit(&format!("acp-data-{id}"), &raw);
                }
                Some("session/request_permission") if has_id => {
                    // Agent blocks on this — emit as acp-req-{id} for the frontend
                    let _ = app2.emit(&format!("acp-req-{id}"), &raw);
                }
                None if has_id || has_err => {
                    // Response to our outgoing request (session/prompt turn-done or error)
                    let _ = app2.emit(&format!("acp-data-{id}"), &raw);
                }
                _ => {} // Unknown method — ignore
            }
        }
        // Signal EOF to frontend
        let _ = app2.emit(&format!("acp-data-{id}"), r#"{"_burrow":"exit"}"#);
    });

    state.procs.lock().unwrap().insert(id, AcpProc {
        stdin: stdin_arc,
        child,
        next_id: Arc::new(std::sync::atomic::AtomicU64::new(2)),
        session_id,
    });
    Ok(())
}
```

- [ ] **Step 3: Add `acp_send` command**

```rust
#[tauri::command]
fn acp_send(state: State<AcpState>, id: u32, text: String) -> Result<(), String> {
    use std::io::Write;
    let guard = state.procs.lock().unwrap();
    let proc = guard.get(&id).ok_or("ACP agent not running")?;
    let rpc_id = proc.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let session_id = proc.session_id.clone();
    let msg = serde_json::json!({
        "jsonrpc": "2.0", "id": rpc_id, "method": "session/prompt",
        "params": {
            "sessionId": session_id,
            "prompt": [{ "type": "text", "text": text }]
        }
    });
    let line = msg.to_string() + "\n";
    proc.stdin.lock().unwrap()
        .write_all(line.as_bytes())
        .and_then(|_| proc.stdin.lock().unwrap().flush())
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 4: Add `acp_stop` command**

```rust
#[tauri::command]
fn acp_stop(state: State<AcpState>, id: u32) {
    if let Some(mut proc) = state.procs.lock().unwrap().remove(&id) {
        let _ = proc.child.kill();
    }
}
```

- [ ] **Step 5: Add `acp_respond_permission` command**

```rust
// Responds to a blocking session/request_permission that the agent sent.
// rpc_id is the JSON-RPC id from the agent's request.
// option_id is the optionId the user chose, or "" to cancel.
#[tauri::command]
fn acp_respond_permission(
    state: State<AcpState>,
    id: u32,
    rpc_id: u64,
    option_id: String,
) -> Result<(), String> {
    use std::io::Write;
    let guard = state.procs.lock().unwrap();
    let proc = guard.get(&id).ok_or("ACP agent not running")?;
    let outcome = if option_id.is_empty() {
        serde_json::json!({ "outcome": "cancelled" })
    } else {
        serde_json::json!({ "outcome": "selected", "optionId": option_id })
    };
    let msg = serde_json::json!({
        "jsonrpc": "2.0", "id": rpc_id,
        "result": { "outcome": outcome }
    });
    let line = msg.to_string() + "\n";
    proc.stdin.lock().unwrap()
        .write_all(line.as_bytes())
        .and_then(|_| proc.stdin.lock().unwrap().flush())
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 6: Register state and commands in `setup`**

Find the `.manage(ClaudeState::default())` line in the Tauri builder and add alongside it:

```rust
.manage(AcpState::default())
```

Find the `generate_handler![...]` macro and add the four new commands:

```rust
acp_start,
acp_send,
acp_stop,
acp_respond_permission,
```

- [ ] **Step 7: Cargo check**

Run: `cd src-tauri && cargo check`
Expected: zero errors. Common issues: `Arc` import (`use std::sync::Arc;` at top), `AtomicU64` import (`use std::sync::atomic::AtomicU64;`).

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: add Rust ACP process management (acp_start/send/stop/respond_permission)"
```

---

## Task 4: ClaudeChat.vue ACP transport branch

**Files:**
- Modify: `src/components/ClaudeChat.vue`

**Interfaces:**
- Consumes: `parseAcpUpdate`, `parseAcpPermRequest` from `src/lib/acpParser.ts`
- Consumes Tauri commands: `acp_start`, `acp_send`, `acp_stop`, `acp_respond_permission`
- Listens to Tauri events: `acp-data-{chatId}`, `acp-req-{chatId}`

The design keeps a single `onLine` function for stream-json and adds a parallel `onAcpData` / `onAcpReq` pair for ACP. The same `messages` array and Vue template render both.

- [ ] **Step 1: Add `transport` and `agentKind` props**

Find the `defineProps<{...}>()` call (~line 528). Add two optional props:

```typescript
// "stream-json" = current Claude CLI path (default).
// "acp" = ACP transport (claude-code-acp or gemini --acp).
transport?: 'stream-json' | 'acp'
// Which agent to run. "gemini" forces transport="acp".
agentKind?: 'claude' | 'gemini'
```

- [ ] **Step 2: Import ACP parser and acpParser types**

At the top of the `<script setup>` block, add:

```typescript
import { parseAcpUpdate, parseAcpPermRequest } from "@/lib/acpParser";
import type { PermissionDecision } from "@/lib/agentTransport";
```

- [ ] **Step 3: Add ACP-specific permission state**

Near the existing `pendingPermission` declarations (~line 926) add:

```typescript
// ACP permission: we need to track the JSON-RPC id to reply to the agent's blocking request
const acpPermRpcId = ref<number | null>(null);
const acpPermToolCallId = ref<string>('');
```

- [ ] **Step 4: Add `onAcpData` handler**

Insert a new function after `onLine` (~line 1266):

```typescript
// Handles lines from the acp-data-{chatId} Tauri event.
// Covers both session/update notifications and session/prompt responses (turn done).
function onAcpData(raw: string) {
  let msg: Record<string, unknown>;
  try { msg = JSON.parse(raw); } catch { return; }

  // Turn done — response to our session/prompt (has id, no method)
  if ('id' in msg && !('method' in msg)) {
    const result = msg.result as Record<string, unknown> | undefined;
    const stopReason = (result?.stopReason as string) ?? 'end_turn';
    busy.value = false;
    for (const m of messages.value) { if (m.partial) m.partial = false; }
    saveMessages(props.chatId, messages.value);
    syncStore();
    scrollToBottom();
    refreshChanges();
    if (!suppressNextDone.value) {
      chats.sendStatusEvent(props.chatId, { type: 'STOP', watching: document.hasFocus() });
      notifyDone();
    }
    suppressNextDone.value = false;
    if (messageQueue.value.length > 0) {
      const next = messageQueue.value.shift()!;
      const qIdx = messages.value.findIndex((m) => m.role === 'queued' && m.text === next);
      if (qIdx !== -1) messages.value.splice(qIdx, 1);
      nextTick(() => sendMessageAcp(next));
    }
    return;
  }

  // EOF from Rust reader thread
  if ((msg as Record<string, unknown>)._burrow === 'exit') {
    if (busy.value) {
      busy.value = false;
      for (const m of messages.value) { if (m.partial) m.partial = false; }
      syncStore();
    }
    return;
  }

  // session/update notification
  if (msg.method !== 'session/update') return;
  const event = parseAcpUpdate(msg.params);
  if (!event) return;

  switch (event.kind) {
    case 'text_chunk': {
      const last = messages.value[messages.value.length - 1];
      if (last?.role === 'assistant' && last.partial && last.id === acpMessageId(event.messageId)) {
        last.text += event.text;
      } else {
        messages.value.push({ id: nextMsgId++, role: 'assistant', text: event.text, partial: true, _acpMsgId: event.messageId } as ChatMessage & { _acpMsgId?: string });
      }
      scrollToBottom();
      break;
    }
    case 'thinking_chunk': {
      const last = messages.value[messages.value.length - 1];
      if (last?.role === 'thinking' && last.partial) {
        last.text += event.text;
      } else {
        messages.value.push({ id: nextMsgId++, role: 'thinking', text: event.text, partial: true });
      }
      scrollToBottom();
      break;
    }
    case 'tool_call':
      messages.value.push({
        id: nextMsgId++, role: 'tool', text: event.title,
        toolInput: { toolCallId: event.toolCallId }, toolUseId: event.toolCallId, toolExpanded: false
      });
      scrollToBottom();
      break;
    case 'tool_output': {
      const toolMsg = [...messages.value].reverse().find(
        (m) => m.role === 'tool' && m.toolUseId === event.toolCallId
      );
      if (toolMsg && event.output) toolMsg.toolOutput = event.output.slice(0, 2000);
      break;
    }
  }
}

// Helper: track which message bubble an ACP messageId maps to (for incremental append).
const acpMsgMap = new Map<string, number>(); // acpMessageId → messages array index
function acpMessageId(msgId: string): number {
  return acpMsgMap.get(msgId) ?? -1;
}
```

Note: The `_acpMsgId` field is used only for identity tracking. Extend `ChatMessage` interface at line 674 to add `_acpMsgId?: string`.

- [ ] **Step 5: Add `onAcpReq` handler**

Insert after `onAcpData`:

```typescript
// Handles lines from the acp-req-{chatId} Tauri event.
// These are blocking session/request_permission requests — agent waits for our response.
function onAcpReq(raw: string) {
  let msg: Record<string, unknown>;
  try { msg = JSON.parse(raw); } catch { return; }

  const perm = parseAcpPermRequest(msg);
  if (!perm) return;

  acpPermRpcId.value = perm.rpcId;
  acpPermToolCallId.value = perm.toolCallId;

  // Reuse the existing pendingPermission banner.
  // Map ACP options to a synthetic CanUseToolReq so no template changes are needed.
  pendingPermission.value = {
    requestId: String(perm.rpcId),          // used only to identify the request
    toolName: `Tool (${perm.toolCallId})`,  // overridden by the tool card title if found
    input: { toolCallId: perm.toolCallId },
    suggestions: perm.options.map(o => ({ label: o.name, optionId: o.optionId, kind: o.kind })),
  } as CanUseToolReq;

  const pmMid = nextMsgId++;
  pendingPermissionMsgId.value = pmMid;
  messages.value.push({ id: pmMid, role: 'system-info', text: `⚡ Permission requested` });
  chats.sendStatusEvent(props.chatId, { type: 'PERMISSION_REQUEST' });
  notifyPermission(pendingPermission.value);
  syncStore();
  scrollToBottom();
}
```

- [ ] **Step 6: Override `respondPermission` to dispatch to ACP when active**

Find `respondPermission` (~line 1348) and modify the `respondControl` call at the end:

```typescript
// Replace the final respondControl(...) call in respondPermission with:
if (effectiveTransport.value === 'acp' && acpPermRpcId.value !== null) {
  const optionId = allow
    ? (opts?.always ? 'allow-always' : 'allow-once')
    : ''  // empty = cancelled
  invoke('acp_respond_permission', {
    id: props.chatId,
    rpcId: acpPermRpcId.value,
    optionId,
  }).catch((e) => {
    messages.value.push({ id: nextMsgId++, role: 'assistant', text: `Permission response failed: ${e}` });
  });
  chats.sendStatusEvent(props.chatId, { type: 'RESUME' });
  syncStore();
  acpPermRpcId.value = null;
} else {
  respondControl(cr.requestId, allow
    ? { behavior: 'allow', updatedInput: opts?.updatedInput ?? cr.input }
    : { behavior: 'deny', message: opts?.message || 'User denied this action.' }
  );
}
```

- [ ] **Step 7: Add `sendMessageAcp` and `effectiveTransport` computed**

Near the top of the `<script setup>` block, add:

```typescript
// Which transport to actually use: "acp" if agentKind=gemini, or if transport prop says "acp".
const effectiveTransport = computed(() =>
  props.agentKind === 'gemini' || props.transport === 'acp' ? 'acp' : 'stream-json'
);
```

Add `sendMessageAcp` (thin wrapper that calls `acp_send`):

```typescript
async function sendMessageAcp(text: string) {
  try {
    await invoke('acp_send', { id: props.chatId, text });
  } catch (e) {
    messages.value.push({ id: nextMsgId++, role: 'assistant', text: `Error: ${e}` });
    busy.value = false;
    chats.sendStatusEvent(props.chatId, { type: 'INTERRUPT' });
    syncStore();
  }
}
```

Modify `sendMessage` to dispatch based on `effectiveTransport`:

```typescript
// In sendMessage(), after pushing the user message and setting busy.value = true:
if (effectiveTransport.value === 'acp') {
  await sendMessageAcp(text);
} else {
  // existing: await invoke("claude_send", { ... })
}
```

- [ ] **Step 8: Wire ACP event listeners in `onMounted`**

Find `onMounted` (~line 1650+). After the existing `unlisten = await listen(...)` block:

```typescript
let acpDataUnlisten: UnlistenFn | null = null;
let acpReqUnlisten: UnlistenFn | null = null;

if (effectiveTransport.value === 'acp') {
  acpDataUnlisten = await listen<string>(`acp-data-${props.chatId}`, (e) => onAcpData(e.payload));
  acpReqUnlisten  = await listen<string>(`acp-req-${props.chatId}`,  (e) => onAcpReq(e.payload));
  await invoke('acp_start', {
    id: props.chatId,
    kind: props.agentKind === 'gemini' ? 'gemini' : 'claude',
    cwd: props.cwd,
  }).catch((e) => { console.error('acp_start failed:', e); });
} else {
  // existing: await invoke("claude_start", { ... })
}
```

In `onBeforeUnmount`, add cleanup:

```typescript
acpDataUnlisten?.();
acpReqUnlisten?.();
if (effectiveTransport.value === 'acp') {
  await invoke('acp_stop', { id: props.chatId }).catch(() => {});
} else {
  // existing: await invoke("claude_stop", { ... })
}
```

- [ ] **Step 9: Wire `abortTurn` and `clearChat` for ACP**

In `abortTurn`, add ACP branch before the existing `invoke("claude_stop")`:

```typescript
if (effectiveTransport.value === 'acp') {
  suppressNextDone.value = true;
  // ACP cancel: send session/cancel notification then restart
  await invoke('acp_stop', { id: props.chatId }).catch(() => {});
  await invoke('acp_start', { id: props.chatId, kind: props.agentKind === 'gemini' ? 'gemini' : 'claude', cwd: props.cwd }).catch(() => {});
  busy.value = false;
  messageQueue.value = [];
  messages.value = messages.value.filter((m) => m.role !== 'queued');
  chats.sendStatusEvent(props.chatId, { type: 'INTERRUPT' });
  syncStore();
  return;
}
// ... existing stream-json abort below
```

- [ ] **Step 10: Build and verify**

Run: `pnpm build`
Expected: zero type errors

- [ ] **Step 11: Commit**

```bash
git add src/components/ClaudeChat.vue src/lib/acpParser.ts
git commit -m "feat: wire ACP transport into ClaudeChat.vue (onAcpData, onAcpReq, respondPermission)"
```

---

## Task 5: claudeChats store + Gemini agent selector

**Files:**
- Modify: `src/stores/claudeChats.ts`
- Modify: `src/components/ClaudeChat.vue` (header agent switcher)

**Interfaces:**
- `ClaudeSession` gains `agentKind: 'claude' | 'gemini'` and `transport: 'stream-json' | 'acp'`
- `create(workspaceId, opts?)` accepts optional `{ agentKind }` param

- [ ] **Step 1: Extend `ClaudeSession` in `claudeChats.ts`**

Find the `ClaudeSession` interface (~line 9) and add:

```typescript
agentKind?: 'claude' | 'gemini'    // default: 'claude'
transport?: 'stream-json' | 'acp'  // default: 'stream-json'
```

- [ ] **Step 2: Update `create()` to accept options**

```typescript
function create(workspaceId: number, opts?: { agentKind?: 'claude' | 'gemini' }): ClaudeSession {
  const id = nextId++;
  const agentKind = opts?.agentKind ?? 'claude';
  const transport: 'stream-json' | 'acp' = agentKind === 'gemini' ? 'acp' : 'stream-json';
  const session: ClaudeSession = {
    id,
    workspaceId,
    claudeSessionId: '',
    title: `Chat ${sessionsForWs(workspaceId).length + 1}`,
    busy: false,
    messageCount: 0,
    agentKind,
    transport,
  };
  // ... rest of existing create() body unchanged
```

- [ ] **Step 3: Add agent switcher button in ClaudeChat.vue header**

Find the `.chat-header` div (~line 4). Add after the existing `<PhArrowCounterClockwise>` button:

```html
<button
  class="chat-header-btn"
  :title="`Switch agent (current: ${agentKind})`"
  @click="cycleAgent"
>
  <component :is="agentKind === 'gemini' ? PhBrainCircuit : ClaudeIcon" :size="13" />
</button>
```

In `<script setup>` add:

```typescript
import { PhBrainCircuit } from "@phosphor-icons/vue";

const agentKind = ref<'claude' | 'gemini'>(
  chats.sessions.find(s => s.id === props.chatId)?.agentKind ?? 'claude'
);

async function cycleAgent() {
  const next: 'claude' | 'gemini' = agentKind.value === 'claude' ? 'gemini' : 'claude';
  agentKind.value = next;
  chats.sync(props.chatId, {
    agentKind: next,
    transport: next === 'gemini' ? 'acp' : 'stream-json'
  });
  suppressNextDone.value = true;
  // Restart with new agent
  if (effectiveTransport.value === 'acp') {
    await invoke('acp_stop', { id: props.chatId }).catch(() => {});
  } else {
    await invoke('claude_stop', { id: props.chatId }).catch(() => {});
  }
  // effectiveTransport is now stale; force a page-level remount instead
  // by clearing the chat (simplest restart path)
  messages.value = [];
  busy.value = false;
  await clearChat();
}
```

Note: `cycleAgent` triggers `clearChat()` which restarts the agent via `claude_start` or `acp_start` depending on `effectiveTransport`. Make sure `clearChat` and `abortTurn` check `effectiveTransport` (done in Task 4 Step 9).

- [ ] **Step 4: Update `sync()` to accept `agentKind` and `transport`**

Find `sync` in `claudeChats.ts` (~line 202). Add the new fields to the `Pick` type:

```typescript
function sync(id: number, patch: Partial<Pick<ClaudeSession,
  "busy" | "messageCount" | "claudeSessionId" | "title" | "status" | "control" | "agentKind" | "transport"
>>) {
```

- [ ] **Step 5: Build check**

Run: `pnpm build`
Expected: passes

- [ ] **Step 6: Commit**

```bash
git add src/stores/claudeChats.ts src/components/ClaudeChat.vue
git commit -m "feat: add agentKind/transport to ClaudeSession, Gemini switcher button"
```

---

## Task 6: Manual end-to-end verification

No automated test suite exists. Perform these checks in `pnpm tauri:dev`:

- [ ] **Claude stream-json (regression):** Open any workspace chat. Send a message. Verify:
  - Text streams in progressively (partial bubble)
  - Tool calls show as collapsed pills; expand to see args and output
  - Permission banner appears for Bash/Edit; Y/N keys work
  - Status dot cycles idle → running → done

- [ ] **Claude ACP (opt-in):** Click the agent switcher button to cycle to "gemini" (will use `acp_start`). Observe in `tauri:dev` console:
  - `acp_start` log shows no error (or check with `cargo run -- --debug`)
  - Sending a message produces streaming text via `acp-data-{id}` events
  - Tool calls render as pills
  - Esc key aborts (triggers `acp_stop` + `acp_start`)

- [ ] **Gemini (if installed):** With `gemini` on PATH and `gemini --acp` available, cycle agent switcher to Gemini. Verify text streams in from Gemini's response.

- [ ] **Permission gate (ACP):** Put Claude in ACP mode with default permissions. Ask it to run a Bash command. Verify the amber permission banner appears, Y allows (responds to agent's blocking request), N cancels.

- [ ] **Commit verification step result** in git notes or a brief comment in the PR.

---

## Subscription Safety Note

**`@agentclientprotocol/claude-agent-acp`** (the correct package, Node ≥ 22) wraps the `claude` CLI binary using the same OAuth subscription auth that Burrow's stream-json path uses (`claude auth login --claudeai`). It does **not** use `ANTHROPIC_API_KEY`. Burrow's `acp_start` explicitly blanks `ANTHROPIC_API_KEY` in the child env (same as `claude_start`) — subscription auth flows through the CLI's keychain credential store exactly as today.

`@zed-industries/claude-agent-acp` (different package) requires `ANTHROPIC_API_KEY=sk-ant-api03-...` and must **never** be used here.

---

## 10-Line Summary

1. **Wire format** — NDJSON (one JSON-RPC 2.0 line per `\n`). Not LSP headers.
2. **Spawn (Claude ACP)** — `npx @agentclientprotocol/claude-agent-acp` — wraps Claude CLI subscription auth, no API key.
3. **Spawn (Gemini)** — `gemini --acp` — same NDJSON framing, same protocol version 1.
4. **Handshake** — `initialize` (id=0) then `session/new` (id=1) synchronously in Rust `spawn_blocking`; session_id stored in `AcpProc`.
5. **Session/update** — discriminated by `sessionUpdate` field (`agent_message_chunk`, `tool_call`, `tool_call_update`, etc.) — maps to existing `ChatMessage` roles via `acpParser.ts`.
6. **Permission model** — ACP's `session/request_permission` is an agent→client JSON-RPC **request** (has id); Burrow responds with `acp_respond_permission` writing `{"outcome":"selected","optionId":"allow-once"}`.
7. **Transport seam** — `effectiveTransport` computed in `ClaudeChat.vue` switches between `claude_send`/`claude_start`/`claude_respond_control` (stream-json) and `acp_send`/`acp_start`/`acp_respond_permission` (ACP) — same Vue template renders both.
8. **Subscription safety** — `ANTHROPIC_API_KEY=""` in child env (same as `claude_start`); OAuth via keychain.
9. **No new npm deps** — `agentTransport.ts` + `acpParser.ts` are plain TypeScript; no ACP SDK installed in the frontend.
10. **Rollout** — Claude stream-json stays the default; ACP activates only when `transport="acp"` prop is passed or user clicks agent switcher; Gemini forces `transport="acp"` automatically.
