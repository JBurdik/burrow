# Claude Code message proxy → rich UI (PoC plan)

> **SUPERSEDED (2026-06-18).** The reverse-proxy idea below was never shipped. The live
> implementation (`ClaudeChat.vue` + `claude_*` in `lib.rs`) drives `claude` directly in
> **bidirectional stream-json** mode — `--input-format stream-json --output-format
> stream-json` with NO `-p` and NO Agent SDK package, so the subscription is preserved
> without a proxy. Interactive control (permission allow/deny + always, ExitPlanMode plan
> approval, AskUserQuestion multi-choice, Edit/Write diff accept-reject) flows over the SDK
> **control protocol** on stdio: the hidden flag `--permission-prompt-tool stdio` routes
> every blocking decision to us as a `can_use_tool` control_request, answered via
> `claude_respond_control` (`{behavior:"allow",updatedInput}` / `{behavior:"deny",message}`;
> AskUserQuestion answers ride in `updatedInput.answers`). Editor context (@-mention file
> completion, share-selection) added too. Kept for history only.

**Date:** 2026-06-09
**Goal:** Render Claude Code's conversation (user/assistant/tool/thinking blocks + token
usage) in a native Burrow UI instead of (or beside) the raw xterm, **while keeping the
user's Claude subscription** — not the API-key `claude -p` path.

## Why a proxy (and not `-p` / SDK / stream-json)

- `claude -p` / Agent SDK / `--output-format stream-json` give clean structured events
  but require an **API key** and are non-interactive. They cannot use a Max/Pro
  **subscription** (OAuth). Dealbreaker — the whole point is subscription + UI.
- A **transparent reverse proxy** keeps interactive `claude` on the subscription. Claude
  Code sends its OAuth bearer to `api.anthropic.com`; we just relay.

## Core idea

Set `ANTHROPIC_BASE_URL=http://127.0.0.1:<port>` for the PTY. Claude Code then POSTs all
`/v1/messages` traffic to our local proxy. Proxy:

1. Forwards the request **verbatim** to `https://api.anthropic.com` — all headers
   (`authorization`, `anthropic-version`, `anthropic-beta`, etc.) passed through
   untouched. Auth unchanged → subscription preserved.
2. **Streams** the SSE response back chunk-by-chunk (`text/event-stream`, no buffering —
   Claude Code needs real-time tokens or it breaks).
3. **Taps** the stream as it passes: parse SSE events, accumulate the assistant message,
   tool_use blocks, thinking blocks, and `message_delta.usage` token counts.
4. Emits the parsed semantic events to the frontend.

```
claude (PTY)  ──http──▶  Burrow proxy  ──https──▶  api.anthropic.com
                              │  (tap, non-blocking)
                              ▼
                     Tauri event  pty-messages-{id}
                              ▼
                     Vue MessagesPane.vue  (chat-style render)
```

## Why it fits Burrow

- Already have a local HTTP server pattern (`tiny_http` hook server in `lib.rs`).
- Already inject `BURROW_*` env into every PTY — add `ANTHROPIC_BASE_URL` next to them.
- Already have the Tauri-event → XTerm.vue listener pattern (`pty-hook-{id}`) to copy.

## SSE events to parse (Anthropic Messages streaming)

- `message_start` → new turn, model id, input token usage
- `content_block_start` → block opens: `text` | `tool_use` | `thinking`
- `content_block_delta` → `text_delta` / `input_json_delta` / `thinking_delta`
- `content_block_stop`, `message_delta` (carries `usage.output_tokens`, `stop_reason`),
  `message_stop`

## Implementation sketch (Rust)

- Use `axum` + `hyper` (or extend the existing `tiny_http`, but streaming bodies are
  easier with hyper/reqwest). `reqwest` with streaming response (`bytes_stream`).
- One proxy server, shared across PTYs. **Map request → PTY id**: inject a unique header
  per PTY (e.g. `x-burrow-pty: <id>`) via env? Claude Code won't add custom headers.
  → Alternative: **one proxy port per PTY**, baked into that PTY's `ANTHROPIC_BASE_URL`.
  Simpler routing, no header hack. Port table in `lib.rs` like the hook port.
- Endpoint to handle: `POST /v1/messages` (+ passthrough any other path verbatim:
  `/v1/messages/count_tokens`, model list, etc.).

## Open questions / risks

- **Per-PTY routing**: per-port (clean) vs single-port + correlation. Lean per-port.
- **gzip/br**: forward `accept-encoding` as-is and tap the *decoded* stream, or strip
  `accept-encoding` on the upstream request so we always read plaintext SSE (simpler;
  tiny bandwidth cost, localhost anyway). Lean strip.
- **TLS**: none needed locally — PTY→proxy is plain HTTP on loopback; proxy→Anthropic is
  HTTPS via reqwest. No cert MITM.
- **Robustness**: if proxy dies, Claude Code breaks. Must be rock-solid + fail-open
  (relay even if the tap parser throws). Tap must never block the relay.
- **Non-`/v1/messages` traffic** (OAuth refresh, telemetry): blind passthrough.
- **ToS**: intercepting your own traffic for your own UI. Fine. Don't log/store tokens.

## PoC milestones

1. **Relay only** — axum proxy, strip `accept-encoding`, stream passthrough. Verify
   `ANTHROPIC_BASE_URL=http://127.0.0.1:PORT claude` works end-to-end interactively on
   subscription (no UI yet). ~80 LOC.
2. **Tap + log** — parse SSE, print assistant text + tool calls + usage to stdout.
3. **Wire to Burrow** — per-PTY port, inject env on spawn, Tauri event, minimal
   `MessagesPane.vue` rendering user/assistant/tool/thinking + token counter.
4. **Polish** — toggle pane vs xterm, persist transcript, copy-message, cost estimate.
