<template>
  <div class="xterm-host" ref="hostEl" />
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from "vue";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import "@xterm/xterm/css/xterm.css";

const props = defineProps<{ ptyId: number; cwd: string; initialCmd?: string }>();
const emit = defineEmits<{ title: [t: string] }>();

const hostEl = ref<HTMLElement>();
let term: Terminal;
let fitAddon: FitAddon;
let unlisten: UnlistenFn | null = null;
let resizeObserver: ResizeObserver;
let pollTimer: ReturnType<typeof setInterval>;

const CLAUDE_RE = /^claude$/i;
const SHELL_RE = /^(zsh|bash|sh|fish|csh|tcsh|dash)$/;

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
    fontFamily: '"JetBrains Mono", "Fira Code", "Cascadia Code", monospace',
    fontSize: 13,
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

  // Create PTY
  await invoke("create_pty", {
    id: props.ptyId,
    cwd: props.cwd,
    cols: term.cols,
    rows: term.rows,
  });

  // Stream output from Rust → xterm
  unlisten = await listen<number[]>(`pty-data-${props.ptyId}`, (event) => {
    term.write(new Uint8Array(event.payload));
  });

  // Send initial command after shell is ready
  if (props.initialCmd) {
    setTimeout(() => {
      const bytes = Array.from(new TextEncoder().encode(props.initialCmd + "\n"));
      invoke("write_pty", { id: props.ptyId, data: bytes });
    }, 600);
  }

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

    if (!proc || SHELL_RE.test(proc)) {
      emit("title", "");          // reset → Terminal N
    } else if (CLAUDE_RE.test(proc)) {
      emit("title", "🤖 Claude");
    } else {
      emit("title", proc);        // e.g. "vim", "python3", "node"
    }
  }, 2000);
});

onBeforeUnmount(async () => {
  clearInterval(pollTimer);
  resizeObserver?.disconnect();
  unlisten?.();
  await invoke("kill_pty", { id: props.ptyId });
  term?.dispose();
});

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
