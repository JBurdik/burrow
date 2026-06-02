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
const emit = defineEmits<{ title: [t: string]; busy: [b: boolean]; needsInput: [b: boolean]; spawn: [req: { cmd: string; token: string; cwd: string }]; done: [] }>();

const ui = useUIStore();

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

let outputBuffer = "";
let hooksSettingsPath = "";
let unlistenHook: UnlistenFn | null = null;

onMounted(async () => {
  term = new Terminal({
    theme: {
      background: "#0a0a0a",
      foreground: "#e2e8f0",
      cursor: "#3b82f6",
      cursorAccent: "#0a0a0a",
      selectionBackground: "#1e3a5f",
      black: "#1e293b",
      red: "#ef4444",
      green: "#22c55e",
      yellow: "#eab308",
      blue: "#3b82f6",
      magenta: "#a855f7",
      cyan: "#06b6d4",
      white: "#cbd5e1",
      brightBlack: "#475569",
      brightRed: "#f87171",
      brightGreen: "#4ade80",
      brightYellow: "#fbbf24",
      brightBlue: "#60a5fa",
      brightMagenta: "#c084fc",
      brightCyan: "#22d3ee",
      brightWhite: "#f1f5f9",
    },
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
  fitAddon.fit();

  // OSC title sequences set by the shell or programs (e.g. vim, tmux)
  term.onTitleChange((title) => {
    if (title) emit("title", title);
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

  unlistenHook = await listen<string>(`pty-hook-${props.ptyId}`, (event) => {
    const state = event.payload;
    if (state === "running")      { emit("busy", true);  emit("needsInput", false); }
    else if (state === "waiting") { emit("busy", true);  emit("needsInput", true);  }
    else if (state === "done")    { emit("done");  emit("busy", false); }
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
  resizeObserver = new ResizeObserver(() => {
    fitAddon.fit();
    invoke("resize_pty", { id: props.ptyId, cols: term.cols, rows: term.rows });
  });
  resizeObserver.observe(hostEl.value!);

  // Poll foreground process every 2s → auto-title
  let lastProcess = "";
  pollTimer = setInterval(async () => {
    const proc = await invoke<string>("get_pty_foreground", { id: props.ptyId });
    if (proc === lastProcess) return;
    lastProcess = proc;

    const idle = !proc || SHELL_RE.test(proc);
    const isClaude = CLAUDE_RE.test(proc);
    const isAgent = isClaude || CODEX_RE.test(proc);

    if (idle) {
      // Back at the shell prompt → whatever ran has exited. Clear any running
      // state (also rescues a stuck dot if an agent was Ctrl+C'd with no done hook).
      emit("busy", false);
      emit("title", "");          // reset → Terminal N
    } else if (isAgent) {
      // An agent is the foreground process — but it stays foreground whether it's
      // THINKING or sitting idle at its own prompt. Presence is NOT "busy": the
      // poll must never fabricate a status here, or the spinner sticks forever.
      // Busy/running/waiting/done come ONLY from the agent's hooks (listener above).
      emit("title", isClaude ? "🤖 Claude" : proc);
    } else {
      // Plain foreground command (npm test, vim, python…): presence == busy.
      emit("busy", true);
      const stripped = outputBuffer.replace(ANSI_RE, "");
      emit("needsInput", NEEDS_INPUT_RE.test(stripped.slice(-200)));
      emit("title", proc);        // e.g. "vim", "python3", "node"
    }
  }, 2000);
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

// Live-apply terminal font preference changes.
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

defineExpose({ focus() { term?.focus(); } });
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
