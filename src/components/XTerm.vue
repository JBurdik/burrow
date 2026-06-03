<template>
  <div class="xterm-host" ref="hostEl" />
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, watch } from "vue";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useUIStore } from "@/stores/ui";
import "@xterm/xterm/css/xterm.css";

const props = defineProps<{ ptyId: number; cwd: string; initialCmd?: string; resultToken?: string }>();
const emit = defineEmits<{ title: [t: string]; busy: [b: boolean]; needsInput: [b: boolean]; spawn: [req: { cmd: string; token: string; cwd: string }]; agentState: [s: string]; agent: [b: boolean] }>();

const ui = useUIStore();

// The whole UI is magnified by CSS `zoom` on #app (see ui.ts); the terminal rides
// that zoom like every other panel. (An earlier attempt to counter-zoom the host
// to net-zoom-1 and re-grow it to `scale*100%` — to fix a selection-row offset —
// made the host overflow the pane/window under WebKit's zoom layout model, so the
// terminal is back to a plain flex child at its natural font size.)
const hostEl = ref<HTMLElement>();
let term: Terminal;
let fitAddon: FitAddon;
let unlisten: UnlistenFn | null = null;
let resizeObserver: ResizeObserver;
let pollTimer: ReturnType<typeof setInterval>;

const CLAUDE_RE = /^claude$/i;
const CODEX_RE = /^codex$/i;
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
let hooksSettingsPath = "";
let unlistenHook: UnlistenFn | null = null;

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
}
function deferredFit() {
  requestAnimationFrame(() => requestAnimationFrame(safeFit));
  document.fonts?.ready.then(safeFit).catch(() => {});
}

onMounted(async () => {
  term = new Terminal({
    theme: ui.activeTheme.xterm,
    fontFamily: ui.terminalFont,
    fontSize: ui.terminalFontSize,
    lineHeight: 1.4,
    cursorBlink: true,
    cursorStyle: "bar",
    allowProposedApi: true,
    scrollback: 5000,
  });

  fitAddon = new FitAddon();
  term.loadAddon(fitAddon);
  term.loadAddon(new WebLinksAddon());
  term.open(hostEl.value!);
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
    if (CLAUDE_RE.test(foreground) || CODEX_RE.test(foreground)) agentTitled = true;
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
  });

  // Send initial command after shell is ready (inject --settings for claude)
  if (props.initialCmd) {
    setTimeout(() => {
      const cmd = launchArgs
        ? `${props.initialCmd} ${launchArgs}`
        : props.initialCmd!;
      const bytes = Array.from(new TextEncoder().encode(cmd + "\n"));
      invoke("write_pty", { id: props.ptyId, data: bytes });
    }, 600);
  }

  // Shift+Enter → send CSI u escape (kitty protocol) so Claude Code inserts a newline
  term.attachCustomKeyEventHandler((e: KeyboardEvent) => {
    if (e.key === "Enter" && e.shiftKey && !e.ctrlKey && !e.altKey && !e.metaKey) {
      if (e.type === "keydown") {
        const bytes = Array.from(new TextEncoder().encode("\x1b[13;2u"));
        invoke("write_pty", { id: props.ptyId, data: bytes });
      }
      return false; // prevent xterm from also sending \r
    }
    return true;
  });

  // Send input from xterm → Rust PTY
  term.onData((data) => {
    const bytes = Array.from(new TextEncoder().encode(data));
    invoke("write_pty", { id: props.ptyId, data: bytes });
  });

  // Resize
  resizeObserver = new ResizeObserver(() => safeFit());
  resizeObserver.observe(hostEl.value!);

  // Poll foreground process → auto-title. Runs once immediately (so tabs get a
  // correct name right after reload instead of waiting 2s) then every 2s.
  let lastProcess = "";
  // Sticky across polls: once an agent is seen foreground, the session stays
  // "agent" until the shell returns. Child processes the agent spawns (a pager,
  // git, node) then can't steal the tab name mid-conversation (the rename bug).
  let isAgentSession = false;
  const poll = async () => {
    const proc = await invoke<string>("get_pty_foreground", { id: props.ptyId });
    // Empty = an unknown/race reading from the daemon, NOT proof the shell is
    // idle. Treating it as idle reset the tab to "Terminal N" mid-conversation.
    // Skip it entirely — keep the last known title/state.
    if (!proc) return;
    foreground = proc;
    if (proc === lastProcess) return;
    lastProcess = proc;

    const isClaude = CLAUDE_RE.test(proc);
    const isAgent = isClaude || CODEX_RE.test(proc);

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
});

onBeforeUnmount(async () => {
  clearInterval(pollTimer);
  resizeObserver?.disconnect();
  unlisten?.();
  unlistenHook?.();
  // detach_pty closes the data stream but leaves the PTY alive in the daemon,
  // so it can be reattached after app restart.
  await invoke("detach_pty", { id: props.ptyId });
  term?.dispose();
});

// Live-apply terminal font preference changes. UI scale is handled by the #app
// zoom (ui.ts), so only the explicit font family/size matter here; refit after.
watch(
  () => [ui.terminalFont, ui.terminalFontSize],
  ([font, size]) => {
    if (!term) return;
    term.options.fontFamily = font as string;
    term.options.fontSize = size as number;
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

defineExpose({ focus() { term?.focus(); }, refit() { safeFit(); deferredFit(); } });
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
