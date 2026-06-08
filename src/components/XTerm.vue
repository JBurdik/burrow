<template>
  <div class="xterm-host" ref="hostEl" />
  <div
    v-if="ui.debugOverlay"
    style="position:absolute;top:2px;left:2px;z-index:9999;background:rgba(0,0,0,.8);color:#0f0;font:10px/1.3 monospace;padding:3px 5px;border:1px solid #0f0;white-space:pre;pointer-events:none"
  >{{ dbg.text }}</div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, watch } from "vue";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { SerializeAddon } from "@xterm/addon-serialize";
import { attachRenderer } from "@/lib/termRenderer";
import type { ITerminalAddon } from "@xterm/xterm";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useUIStore } from "@/stores/ui";
import "@xterm/xterm/css/xterm.css";

const props = defineProps<{ ptyId: number; cwd: string; initialCmd?: string; resultToken?: string }>();
const emit = defineEmits<{ title: [t: string]; busy: [b: boolean]; needsInput: [b: boolean]; spawn: [req: { cmd: string; token: string; cwd: string }]; agentState: [s: string]; agent: [b: boolean]; interrupt: [] }>();

const ui = useUIStore();

// The whole UI is magnified by CSS `zoom: s` on #app (ui.ts). That breaks mouse
// selection in xterm: getCoords does `clientX - getBoundingClientRect().left`
// (ZOOMED/visual px) ÷ cell size (the canvas measureText metric, UNZOOMED layout
// px) — under an ancestor zoom the two disagree by `s`, so the selection lands a
// row/col off. Fix: counter-zoom the host to net-zoom-1 (zoom: 1/s) so rect and
// cell metric share one space, and re-grow the box so it still fills the pane.
//
// Crucially the re-grow is in PX (parent.clientWidth * s), NOT `s*100%`. A `%`
// width resolves against a containing block already inflated by the #app-zoom
// chain and compounds — measured at scale 1.23 it overflowed the pane by ~23%
// (host 1575px vs pane 1280px) and spilled off the window. PX from the parent's
// own layout width lands the zoomed footprint back exactly on the pane.
// Verified in a real WebKit-zoom browser: px → rect==offset (net-zoom-1) and
// host==pane (no overflow); % → 295px horizontal overflow.
const scaledFontSize = () => ui.terminalFontSize * ui.effectiveScale;

const hostEl = ref<HTMLElement>();

// ── TEMP debug overlay ────────────────────────────────────────────────────────
// Visible readout to diagnose the prod-only blank terminal: does data arrive, does
// the xterm buffer fill, is the alt-screen active, is the host sized? Toggle with
// localStorage 'burrow-debug' = '1' (on by default here for the debug build).
let bytesRx = 0;
let writes = 0;
let txBack = 0;   // bytes xterm sent BACK to the pty (input + query responses)
let txBackN = 0;
// Raw capture of the first N bytes received, dumped to a file when the debug
// overlay is on — lets us inspect exactly what the agent emitted in a prod build
// (e.g. whether ?1049h alt-screen-enter arrives and how xterm reacted).
const dbg = ref({ text: "" });
function refreshDbg() {
  if (!ui.debugOverlay || !term) return;
  const host = hostEl.value;
  let bufLines = 0;
  let altType = "?";
  try {
    const b = term.buffer.active;
    altType = b.type;
    for (let i = 0; i < term.rows; i++) {
      const line = b.getLine(b.viewportY + i);
      if (line && line.translateToString(true).trim()) bufLines++;
    }
  } catch (e) { altType = "err:" + (e as Error).message; }
  dbg.value.text = [
    `pty ${props.ptyId}  ${term.cols}x${term.rows}`,
    `host ${host?.offsetWidth}x${host?.offsetHeight} scale ${ui.effectiveScale.toFixed(2)}`,
    `rx ${bytesRx}B writes ${writes}`,
    `txBack ${txBack}B / ${txBackN}`,
    `buf ${altType} lines ${bufLines}`,
    `age ${lastDataAt ? Math.round(performance.now() - lastDataAt) : "-"}ms`,
  ].join("\n");
}

function applyCounterZoom() {
  const el = hostEl.value;
  const parent = el?.parentElement;
  if (!el || !parent) return;
  const s = ui.effectiveScale;
  if (s === 1) {
    el.style.zoom = "";
    el.style.width = "";
    el.style.height = "";
    el.style.flex = "";
    return;
  }
  // Basis is the host's OWN natural flex slot, NOT parent.clientHeight: the parent
  // pane also holds a 26px titlebar when split, so parent.clientHeight overcounts
  // and the host spills past the pane bottom. Reset to flex first, read the laid-out
  // slot (flexbox already excludes the titlebar), then grow that by `s`.
  el.style.flex = "";
  el.style.zoom = "";
  el.style.width = "";
  el.style.height = "";
  const w = el.clientWidth;
  const h = el.clientHeight;
  el.style.flex = "none";
  el.style.zoom = String(1 / s);
  el.style.width = `${w * s}px`;
  el.style.height = `${h * s}px`;
}
let term: Terminal;
let fitAddon: FitAddon;
let serializeAddon: SerializeAddon;
let renderAddon: ITerminalAddon | null = null;
let unlisten: UnlistenFn | null = null;
let unlistenSnapReq: UnlistenFn | null = null;
let resizeObserver: ResizeObserver;
let pollTimer: ReturnType<typeof setInterval>;
let dbgTimer: ReturnType<typeof setInterval>;

const CLAUDE_RE = /^claude$/i;
const CODEX_RE = /^codex$/i;
const COPILOT_RE = /^copilot$/i;
const SHELL_RE = /^(zsh|bash|sh|fish|csh|tcsh|dash)$/;
// Legacy pattern-match fallback (used when hooks aren't active)
const NEEDS_INPUT_RE = /[›❯]|(\(y\/n\)|\[y\/n\]|\(Y\/n\)|\[Y\/n\])/i;
const ANSI_RE = /\x1b(?:\[[0-9;?]*[A-Za-z]|[^[])/g;
// OSC 9999 from the `burrow` CLI: \x1b]9999;spawn;<b64cmd>;<b64token>;<b64cwd>\x07
const SPAWN_RE = /\x1b\]9999;spawn;([A-Za-z0-9+/=]*);([A-Za-z0-9+/=]*);([A-Za-z0-9+/=]*)\x07/g;
const b64decode = (s: string) =>
  s ? new TextDecoder().decode(Uint8Array.from(atob(s), (c) => c.charCodeAt(0))) : "";

// Last known foreground process name (from the poll) — gates OSC titles.
let foreground = "";
// True once the foreground agent has set its OWN title via OSC. After that the
// poll stops seeding "Claude" over it, so Claude Code's descriptive title sticks
// (and the tab tells you what that session was doing). Reset when the shell
// returns (agent gone).
let agentTitled = false;

// Strip control/non-printable chars (mid-OSC replay garbage), trim, cap length.
function sanitizeTitle(s: string): string {
  // eslint-disable-next-line no-control-regex
  return s.replace(/[\x00-\x1f\x7f]/g, "").trim().slice(0, 80);
}

let outputBuffer = "";
// Timestamp of the last PTY output chunk — used to detect when the shell has
// finished its startup (sourcing .zprofile/.zshrc, printing the first prompt)
// and gone quiet, so we can inject the launch command without racing the init.
let lastDataAt = 0;
let hooksSettingsPath = "";
let unlistenHook: UnlistenFn | null = null;
let unlistenDrop: UnlistenFn | null = null;

// Image files an agent can read. Drag-dropped paths and clipboard image mimes are
// matched against this before we inject anything into the PTY.
const IMG_EXT_RE = /\.(png|jpe?g|gif|webp|bmp|svg|heic|heif|tiff?|avif)$/i;

// Type a filesystem path into THIS pty so the foreground agent (Claude Code,
// Copilot, …) picks it up as an image attachment — same as dragging a file into
// the agent's own terminal. Space-bearing paths are single-quoted so the shell /
// agent input parses them as one token; a trailing space ends the token.
function injectImagePath(path: string) {
  const quoted = /\s/.test(path) ? `'${path.replace(/'/g, "'\\''")}'` : path;
  const bytes = Array.from(new TextEncoder().encode(quoted + " "));
  invoke("write_pty", { id: props.ptyId, data: bytes });
}

// Cmd+V of an image: the DOM paste event carries the bitmap as a File. Persist it
// to a temp file via Rust, then inject the path. Text paste falls through to
// xterm's native handler untouched (no preventDefault on the non-image path).
async function onPaste(e: ClipboardEvent) {
  const items = e.clipboardData?.items;
  if (!items) return;
  for (const it of items) {
    if (it.kind !== "file" || !it.type.startsWith("image/")) continue;
    const file = it.getAsFile();
    if (!file) continue;
    e.preventDefault();
    // Claude Code reads the macOS clipboard itself on Ctrl+V (\x16) and inserts
    // an `[Image #N]` reference — far better than typing a temp path. The image
    // is still on the OS clipboard, so just forward \x16 and let Claude grab it.
    // Other agents (Copilot/Aider) lack clipboard-image support → temp-path path.
    if (CLAUDE_RE.test(foreground)) {
      invoke("write_pty", { id: props.ptyId, data: [0x16] });
      return;
    }
    const buf = new Uint8Array(await file.arrayBuffer());
    let bin = "";
    for (let i = 0; i < buf.length; i++) bin += String.fromCharCode(buf[i]);
    const b64 = btoa(bin);
    const ext = it.type.split("/")[1] || "png";
    try {
      const path = await invoke<string>("save_temp_image", { b64, ext });
      injectImagePath(path);
    } catch { /* clipboard write failed — leave the prompt alone */ }
    return;
  }
}

// Fit, then re-fit after layout and web-fonts settle. On restart the first fit
// can run before the surrounding panels are laid out or the mono web-font has
// loaded — xterm then measures the wrong cell width and picks too many cols/rows,
// so the terminal overflows the panel. The container size never changes again, so
// the ResizeObserver never corrects it. Re-fitting on the next frames + after
// fonts.ready re-measures with the real metrics and resizes the PTY to match.
function safeFit() {
  if (!term || !fitAddon || !hostEl.value) return;
  if (hostEl.value.offsetWidth === 0 || hostEl.value.offsetHeight === 0) return;
  fitAddon.fit();
  invoke("resize_pty", { id: props.ptyId, cols: term.cols, rows: term.rows });
  notifyFloatGrid();
}

// Tell any floating mirror of this pty that the grid changed, so it can match
// cols/rows (the shared PTY's SIGWINCH already makes the agent repaint; the
// float just needs the new dims to render that repaint correctly).
let lastGridCols = 0;
let lastGridRows = 0;
function notifyFloatGrid() {
  if (!term) return;
  if (term.cols === lastGridCols && term.rows === lastGridRows) return;
  lastGridCols = term.cols;
  lastGridRows = term.rows;
  invoke("notify_float_grid", { ptyId: props.ptyId, cols: term.cols, rows: term.rows }).catch(() => {});
}
function deferredFit() {
  requestAnimationFrame(() => requestAnimationFrame(safeFit));
  document.fonts?.ready.then(safeFit).catch(() => {});
}

onMounted(async () => {
  term = new Terminal({
    theme: ui.activeTheme.xterm,
    fontFamily: ui.terminalFont,
    fontSize: scaledFontSize(),
    lineHeight: 1.4,
    cursorBlink: true,
    cursorStyle: "bar",
    allowProposedApi: true,
    // Lets themes use rgba/transparent xterm backgrounds so the window
    // wallpaper / OS vibrancy shows through the terminal (meme + lime themes).
    allowTransparency: true,
    scrollback: 5000,
  });

  fitAddon = new FitAddon();
  term.loadAddon(fitAddon);
  term.loadAddon(new WebLinksAddon());
  // SerializeAddon lets a floating-bubble window request a snapshot of THIS
  // terminal's current screen (incl. alt-screen TUIs) to reconstruct it exactly
  // on expand — the daemon ring-buffer replay can't rebuild an alt-screen.
  serializeAddon = new SerializeAddon();
  term.loadAddon(serializeAddon);
  term.open(hostEl.value!);
  // GPU renderer (WebGL → Canvas → DOM). Must follow open(). Default DOM
  // renderer is the slowest; this is the big win for agent output floods.
  renderAddon = attachRenderer(term);

  applyCounterZoom();
  safeFit();
  deferredFit();

  // OSC title sequences set by the shell or programs (e.g. vim, tmux, Claude).
  // The interactive shell (zsh/bash) sets the OSC title to the cwd or last
  // command as cosmetics — junk for a tab name — so those are ignored. But an
  // AGENT's own title IS wanted: Claude Code sets a descriptive title (the task
  // it's on), which is exactly what tells you what each tab was doing. We accept
  // it and flag `agentTitled` so the poll stops re-seeding "Claude" over it.
  // (Truncation risk: on reattach the daemon ring buffer can replay a snapshot
  // starting mid-OSC; sanitizeTitle strips the control garbage.)
  term.onTitleChange((raw) => {
    const title = sanitizeTitle(raw);
    if (!title) return;
    if (!foreground) return;
    if (SHELL_RE.test(foreground)) return;   // shell prompt cwd/cmd junk
    if (CLAUDE_RE.test(foreground) || CODEX_RE.test(foreground) || COPILOT_RE.test(foreground)) agentTitled = true;
    emit("title", title);
  });

  // Agent status (running/waiting/done) is driven by hooks installed GLOBALLY in
  // each agent's config (~/.claude/settings.json, ~/.codex/hooks.json) by Rust at
  // startup. Those hooks run `burrow hook`, which POSTs to the local hook HTTP
  // server; Rust re-emits a `pty-hook-{id}` Tauri event that the listener below
  // turns into busy/needsInput/done. Because they're global + env-driven, they fire
  // for EVERY agent session in this PTY — launched-by-button, typed by hand, or
  // reattached after restart. The poll never fabricates "busy" for an agent
  // process, so these events are the sole source of truth (no stuck orange dot).
  const baseCmd = props.initialCmd?.trim().split(/\s+/)[0] ?? "";
  let launchArgs = "";

  // The ONE thing that can't live in global config is per-tab result capture for
  // `burrow wait <token>` (the token is unique to this spawned sub-agent). Inject a
  // tiny per-launch --settings carrying just that Stop hook when a token is present.
  if (baseCmd === "claude" && props.resultToken) {
    hooksSettingsPath = `/tmp/agentic-ide-hooks-${props.ptyId}.json`;
    const hooksJson = JSON.stringify({
      hooks: {
        Stop: [{ hooks: [{ type: "command", command: `burrow capture ${props.resultToken}` }] }],
      },
    });
    await invoke("write_text_file", { path: hooksSettingsPath, content: hooksJson });
    launchArgs = `--settings ${hooksSettingsPath}`;
  }

  // Forward the agent's hook state straight through as ONE semantic event. The
  // old path emitted busy+needsInput as two separate signals, whose ordering let
  // a trailing "waiting" clobber a fresh "done" (done → blue bug). A single
  // running|waiting|done event has no ordering hazard; Terminal.vue owns the
  // transition. The 2s poll never fabricates agent status, so these hooks are the
  // sole source of truth for an agent's running/waiting/done.
  unlistenHook = await listen<string>(`pty-hook-${props.ptyId}`, (event) => {
    const state = event.payload;
    if (state === "running" || state === "waiting" || state === "done")
      emit("agentState", state);
  });

  // Float-bubble snapshot responder: a floating window mirroring THIS pty asks
  // for the current screen on expand. SerializeAddon rebuilds the exact visible
  // state (alt-screen + modes) which the float writes into a fresh xterm. We
  // never tear this down per-tab visibility — main XTerms stay mounted (v-show),
  // so a hidden tab still answers. (tauriEmit, not the Vue `emit` above.)
  unlistenSnapReq = await listen(`float-snap-req-${props.ptyId}`, async () => {
    try {
      // Send the grid dims too: the float must use the SAME cols/rows so the
      // serialized screen (and subsequent live bytes, laid out for this grid)
      // render identically — it font-scales to fit instead of reflowing.
      await invoke("send_float_snapshot", {
        ptyId: props.ptyId,
        data: serializeAddon.serialize(),
        cols: term.cols,
        rows: term.rows,
      });
    } catch { /* float falls back to live-only after its timeout */ }
  });

  // Create PTY
  await invoke("create_pty", {
    id: props.ptyId,
    cwd: props.cwd,
    cols: term.cols,
    rows: term.rows,
  });

  // Stream output from Rust → xterm
  unlisten = await listen<number[]>(`pty-data-${props.ptyId}`, (event) => {
    const bytes = new Uint8Array(event.payload);
    term.write(bytes);
    bytesRx += bytes.length;
    writes++;
    const text = new TextDecoder().decode(bytes);

    // `burrow spawn` requests: decode base64 fields → open a new tab.
    // Loop, since one chunk may carry several.
    SPAWN_RE.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = SPAWN_RE.exec(text)) !== null) {
      try {
        const cmd = b64decode(m[1]).trim();
        if (cmd) emit("spawn", { cmd, token: b64decode(m[2]).trim(), cwd: b64decode(m[3]).trim() });
      } catch { /* ignore malformed payload */ }
    }

    outputBuffer = (outputBuffer + text).slice(-500);
    lastDataAt = performance.now();
  });

  // Send initial command once the shell is actually ready (inject --settings for
  // claude). A fixed timeout raced slow startups (a login shell sourcing
  // .zprofile/.zshrc, an .zshrc that errors): the command was typed before the
  // prompt existed, so the newline got eaten and the command sat unrun — or input
  // interleaved with the prompt and zsh parsed the prompt text as a command.
  // Instead wait for the PTY to emit output and then fall quiet (prompt printed,
  // init done), with a hard cap so we never hang if the shell stays silent.
  if (props.initialCmd) {
    const cmd = launchArgs
      ? `${props.initialCmd} ${launchArgs}`
      : props.initialCmd!;
    const QUIET_MS = 250;   // silence that signals "prompt is ready"
    const MAX_WAIT_MS = 5000;
    const startedAt = performance.now();
    const trySend = () => {
      const now = performance.now();
      const ready = lastDataAt > 0 && now - lastDataAt >= QUIET_MS;
      if (ready || now - startedAt >= MAX_WAIT_MS) {
        const bytes = Array.from(new TextEncoder().encode(cmd + "\n"));
        invoke("write_pty", { id: props.ptyId, data: bytes });
      } else {
        setTimeout(trySend, 100);
      }
    };
    setTimeout(trySend, 100);
  }

  // Custom key handling on top of xterm's defaults.
  term.attachCustomKeyEventHandler((e: KeyboardEvent) => {
    // Cmd+K → clear the terminal (iTerm-style: wipe scrollback + viewport, keep
    // the current prompt line). xterm's clear() drops every line above the cursor
    // row; we swallow the key so it never reaches the PTY.
    if (e.metaKey && !e.ctrlKey && !e.altKey && (e.key === "k" || e.key === "K")) {
      if (e.type === "keydown") term.clear();
      return false;
    }
    // Shift+Enter → CSI u escape (kitty protocol) so Claude Code inserts a newline.
    if (e.key === "Enter" && e.shiftKey && !e.ctrlKey && !e.altKey && !e.metaKey) {
      if (e.type === "keydown") send("\x1b[13;2u");
      return false; // prevent xterm from also sending \r
    }
    // Option/Alt + ←/→ → word-wise cursor movement. macOS terminals map Option
    // to readline's word-left/right (ESC b / ESC f); xterm.js doesn't do this on
    // its own, so emit the sequences ourselves and swallow the key.
    if (e.altKey && !e.ctrlKey && !e.metaKey && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
      if (e.type === "keydown") send(e.key === "ArrowLeft" ? "\x1bb" : "\x1bf");
      return false;
    }
    // Option/Alt + Backspace → delete the previous word (readline: ESC DEL).
    if (e.altKey && !e.ctrlKey && !e.metaKey && (e.key === "Backspace" || e.key === "Delete")) {
      if (e.type === "keydown") send("\x1b\x7f");
      return false;
    }
    return true;
  });

  function send(s: string) {
    invoke("write_pty", { id: props.ptyId, data: Array.from(new TextEncoder().encode(s)) });
  }

  // Send input from xterm → Rust PTY
  term.onData((data) => {
    // Interrupt detection: a bare ESC (single 0x1b — NOT an escape sequence like
    // arrows "\x1b[A") or Ctrl+C (0x03) cancels an agent's running turn. Agents
    // fire NO Stop hook on interrupt, and the foreground poll never clears an
    // agent's "running" (claude stays foreground at its prompt) → the dot would
    // stick orange forever. Forward as a semantic interrupt so Terminal can
    // settle the leaf back to idle. Generic = works for every agent (claude,
    // codex, aider…). No-op if nothing was running.
    if (data === "\x1b" || data === "\x03") emit("interrupt");
    const bytes = Array.from(new TextEncoder().encode(data));
    txBack += bytes.length;
    txBackN++;
    invoke("write_pty", { id: props.ptyId, data: bytes });
  });

  // Resize. Observe the PARENT, not the host: under a non-1 scale the host's own
  // size is driven by applyCounterZoom (explicit px), so watching the host would
  // miss pane/window resizes and risk a feedback loop. The parent reflects the
  // real available space — recompute the counter-zoom box, then refit.
  resizeObserver = new ResizeObserver(() => { applyCounterZoom(); safeFit(); });
  resizeObserver.observe(hostEl.value!.parentElement ?? hostEl.value!);

  // Cmd+V image paste → temp file → path into the PTY (text paste untouched).
  term.textarea?.addEventListener("paste", onPaste);

  // Drag-and-drop image files. Tauri intercepts OS drops (dragDrop default-on), so
  // the bitmap never reaches the DOM — we listen to the window-level drop event
  // instead. It fires for ALL panes, so route by hit-testing the drop point
  // against THIS host's rect: only the pane under the cursor injects the path.
  // Dropped files already have a real path, so no temp copy is needed.
  unlistenDrop = await getCurrentWebview().onDragDropEvent((event) => {
    if (event.payload.type !== "drop") return;
    const rect = hostEl.value?.getBoundingClientRect();
    if (!rect) return;
    const dpr = window.devicePixelRatio || 1;
    const cx = event.payload.position.x / dpr;
    const cy = event.payload.position.y / dpr;
    if (cx < rect.left || cx > rect.right || cy < rect.top || cy > rect.bottom) return;
    for (const p of event.payload.paths) {
      if (IMG_EXT_RE.test(p)) injectImagePath(p);
    }
  });

  // Poll foreground process → auto-title. Runs once immediately (so tabs get a
  // correct name right after reload instead of waiting 2s) then every 2s.
  let lastProcess = "";
  // Sticky across polls: once an agent is seen foreground, the session stays
  // "agent" until the shell returns. Child processes the agent spawns (a pager,
  // git, node) then can't steal the tab name mid-conversation (the rename bug).
  let isAgentSession = false;
  const poll = async () => {
    const proc = await invoke<string>("get_pty_foreground", { id: props.ptyId });
    // Empty foreground = no non-shell process in the group: either a daemon
    // race/mid-conversation read (must NOT reset an agent's title/state) OR a
    // plain command just exited and only the shell remains. For an agent
    // session, skip — keep last known title/state (the "Terminal N" reset bug).
    // For a plain terminal that was busy, empty means the command finished and
    // we're back at the prompt → clear busy, else the orange dot sticks forever
    // (foreground_name returns "" for a bare shell, so SHELL_RE never fires).
    if (!proc) {
      if (!isAgentSession && lastProcess && !SHELL_RE.test(lastProcess)) {
        lastProcess = "";
        emit("busy", false);
        emit("title", "");
      }
      return;
    }
    foreground = proc;
    if (proc === lastProcess) return;
    lastProcess = proc;

    const isClaude = CLAUDE_RE.test(proc);
    const isAgent = isClaude || CODEX_RE.test(proc) || COPILOT_RE.test(proc);

    if (SHELL_RE.test(proc)) {
      // Back at the shell prompt → whatever ran (agent or command) has exited.
      // Clear running state (rescues a stuck dot if an agent was interrupted with
      // no done hook) and reset the tab name.
      isAgentSession = false;
      agentTitled = false;
      emit("agent", false);
      emit("busy", false);
      emit("title", "");          // reset → Terminal N
    } else if (isAgent) {
      // An agent is the foreground process — but it stays foreground whether it's
      // THINKING or sitting idle at its own prompt. Presence is NOT "busy": the
      // poll must never fabricate a status here, or the spinner sticks forever.
      // running/waiting/done come ONLY from the agent's hooks (listener above).
      isAgentSession = true;
      emit("agent", true);        // mark the tab as an agent (robot icon)
      // Only SEED a name until the agent sets its own OSC title; after that don't
      // override it (that was the "title keeps reverting to Claude" bug).
      if (!agentTitled) emit("title", isClaude ? "Claude" : proc);
    } else if (isAgentSession) {
      // A non-shell child process INSIDE a live agent session (the agent opened a
      // pager, ran git, spawned node…). Keep the agent's title and don't flip to
      // a plain-command "busy" — the agent's hooks remain the status source.
    } else {
      // Plain foreground command (npm test, vim, python…): presence == busy.
      emit("agent", false);
      emit("busy", true);
      const stripped = outputBuffer.replace(ANSI_RE, "");
      emit("needsInput", NEEDS_INPUT_RE.test(stripped.slice(-200)));
      emit("title", proc);        // e.g. "vim", "python3", "node"
    }
  };
  poll();
  pollTimer = setInterval(poll, 2000);
  dbgTimer = setInterval(refreshDbg, 500);
});

onBeforeUnmount(async () => {
  clearInterval(pollTimer);
  clearInterval(dbgTimer);
  resizeObserver?.disconnect();
  unlisten?.();
  unlistenHook?.();
  unlistenSnapReq?.();
  unlistenDrop?.();
  term?.textarea?.removeEventListener("paste", onPaste);
  // detach_pty closes the data stream but leaves the PTY alive in the daemon,
  // so it can be reattached after app restart.
  await invoke("detach_pty", { id: props.ptyId });
  renderAddon?.dispose();
  term?.dispose();
});

// Live-apply terminal font + UI-scale changes. The host counter-zooms to net-1,
// so the visual size comes from xterm's own fontSize (scaled by effectiveScale);
// a scale change also resizes the counter-zoom box, so recompute it then refit.
watch(
  () => [ui.terminalFont, ui.terminalFontSize, ui.effectiveScale],
  ([font]) => {
    if (!term) return;
    term.options.fontFamily = font as string;
    term.options.fontSize = scaledFontSize();
    applyCounterZoom();
    fitAddon?.fit();
    invoke("resize_pty", { id: props.ptyId, cols: term.cols, rows: term.rows });
  },
);

// Live-apply theme changes to the running terminal.
watch(
  () => ui.activeTheme,
  (t) => {
    if (term) term.options.theme = t.xterm;
  },
);

defineExpose({
  focus() { term?.focus(); },
  refit() { safeFit(); deferredFit(); },
  // Inject text into the PTY (no trailing newline — user reviews then hits Enter).
  sendText(text: string) {
    const bytes = Array.from(new TextEncoder().encode(text));
    invoke("write_pty", { id: props.ptyId, data: bytes });
    term?.focus();
  },
});
</script>

<style scoped>
.xterm-host {
  flex: 1;
  overflow: hidden;
  padding: 8px;
}

.xterm-host :deep(.xterm) { height: 100%; }
.xterm-host :deep(.xterm-viewport) { background: transparent !important; }
</style>
