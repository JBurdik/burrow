<template>
  <div class="bubble-root" :class="{ expanded }">
    <!-- ── Collapsed state: a thin bar (icon · title · status · close) ── -->
    <div v-if="!expanded" class="bubble-bar" data-tauri-drag-region @click="expand" :title="displayTitle">
      <PhRobot v-if="isAgentSession" :size="13" class="bar-icon" />
      <PhTerminal v-else :size="13" class="bar-icon" />
      <span class="bar-title">{{ displayTitle }}</span>
      <span v-if="status !== 'idle'" class="bar-status-dot" :class="`status-${status}`">
        {{ status === 'running' ? spinnerFrame : '' }}
      </span>
      <button class="bar-close" title="Close" @click.stop="closeWindow">
        <PhX :size="10" weight="bold" />
      </button>
    </div>

    <!-- ── Expanded state: terminal panel ── -->
    <template v-else>
      <div class="bubble-header" data-tauri-drag-region>
        <span v-if="status !== 'idle'" class="bubble-status-dot" :class="`status-${status}`">
          {{ status === 'running' ? spinnerFrame : '' }}
        </span>
        <span class="bubble-title" data-tauri-drag-region>{{ displayTitle }}</span>
        <div class="bubble-actions">
          <button class="bubble-btn" title="Focus in main window" @click="focusMain">
            <PhArrowSquareOut :size="11" />
          </button>
          <button class="bubble-btn" title="Collapse" @click="collapse">
            <PhMinus :size="11" weight="bold" />
          </button>
          <button class="bubble-btn bubble-btn-close" title="Close" @click="closeWindow">
            <PhX :size="11" weight="bold" />
          </button>
        </div>
      </div>
      <div class="bubble-term" ref="hostEl" />
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, watch, nextTick } from "vue";
import { Terminal } from "@xterm/xterm";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { invoke } from "@tauri-apps/api/core";
import { listen, emit, type UnlistenFn } from "@tauri-apps/api/event";
import { PhArrowSquareOut, PhMinus, PhRobot, PhTerminal, PhX } from "@phosphor-icons/vue";
import { useUIStore } from "@/stores/ui";
import { spinnerFrame } from "@/lib/spinner";
import "@xterm/xterm/css/xterm.css";

const props = defineProps<{ ptyId: number; wsId: number; initTitle: string }>();

const ui = useUIStore();
const hostEl = ref<HTMLElement>();
const displayTitle = ref(props.initTitle || `PTY ${props.ptyId}`);
const status = ref<"idle" | "running" | "waiting" | "done">("idle");
const isAgentSession = ref(false);
const expanded = ref(false);

let term: Terminal | null = null;
let unlistenData: UnlistenFn | null = null;
let unlistenSnap: UnlistenFn | null = null;
let unlistenHook: UnlistenFn | null = null;
let resizeObserver: ResizeObserver | null = null;
let doneTimer: ReturnType<typeof setTimeout> | null = null;
let snapTimer: ReturnType<typeof setTimeout> | null = null;
let termMounted = false;
// Snapshot handshake state. While "loading" we buffer live pty bytes into
// liveQueue instead of writing them, so the serialized snapshot (the main
// window's exact current screen) lands first, then the queued live bytes apply
// on top in order. "idle" = collapsed (drop bytes; resync via snapshot on next
// expand). "live" = write straight through.
let phase: "idle" | "loading" | "live" = "idle";
let liveQueue: Uint8Array[] = [];
let unlistenMoved: UnlistenFn | null = null;
let moveTimer: ReturnType<typeof setTimeout> | null = null;

async function focusMain() {
  await emit("float-focus-tab", { ptyId: props.ptyId, wsId: props.wsId });
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const main = await WebviewWindow.getByLabel("main");
  if (main) { await main.show(); await main.setFocus(); }
}

function closeWindow() {
  invoke("close_float_window", { label: `float-${props.ptyId}` }).catch(() => {});
}

function collapse() {
  expanded.value = false;
  // Stop mirroring: drop buffered/live bytes (status dots still come from the
  // independent pty-hook channel). Next expand re-syncs via a fresh snapshot.
  phase = "idle";
  liveQueue = [];
  if (snapTimer) { clearTimeout(snapTimer); snapTimer = null; }
  invoke("set_window_size", { label: `float-${props.ptyId}`, width: 240, height: 36 }).catch(() => {});
}

async function expand() {
  expanded.value = true;
  await invoke("set_window_size", { label: `float-${props.ptyId}`, width: 620, height: 420 }).catch(() => {});
  // Wait for the expanded block to render (v-if swap) + its ref to populate,
  // THEN mount/fit so xterm measures the real 460px box (not the 64px collapsed
  // one → wrong cols).
  await nextTick();
  await nextTick();
  if (!termMounted) mountTerm();
  else term?.reset();
  // Begin the snapshot handshake: buffer live bytes, ask the main XTerm for its
  // serialized current screen. The reply (float-snap-{id}) writes the snapshot
  // then drains the queue and flips to live. Falls back to live-only on timeout.
  phase = "loading";
  liveQueue = [];
  await invoke("request_float_snapshot", { ptyId: props.ptyId });
  if (snapTimer) clearTimeout(snapTimer);
  snapTimer = setTimeout(() => {
    if (phase !== "loading") return;
    while (liveQueue.length) term?.write(liveQueue.shift()!);
    phase = "live";
  }, 400);
  // Re-fit the font a few times as the async window resize lands + focus so
  // typing works immediately.
  for (const d of [60, 180, 400]) setTimeout(() => { fitFont(); term?.focus(); }, d);
}

// Keep the float's grid identical to the main terminal's (so the mirror renders
// 1:1) and scale the FONT to fill the window — never reflow cols/rows (that would
// diverge from the source). Monospace cell ≈ 0.6·fontSize wide, 1.4·fontSize tall.
function fitFont() {
  if (!term || !hostEl.value) return;
  const w = hostEl.value.clientWidth;
  const h = hostEl.value.clientHeight;
  if (w < 10 || h < 10) return;
  const fs = Math.max(
    4,
    Math.min(28, Math.floor(Math.min(w / (term.cols * 0.6), h / (term.rows * 1.4)))),
  );
  if (term.options.fontSize !== fs) term.options.fontSize = fs;
}

function mountTerm() {
  if (!hostEl.value || termMounted) return;
  termMounted = true;

  term = new Terminal({
    theme: ui.activeTheme.xterm,
    fontFamily: ui.terminalFont,
    fontSize: ui.terminalFontSize,
    lineHeight: 1.4,
    cursorBlink: true,
    cursorStyle: "bar",
    allowProposedApi: true,
    allowTransparency: true,
    scrollback: 2000,
  });
  term.loadAddon(new WebLinksAddon());
  term.open(hostEl.value);
  fitFont();
  term.focus();

  term.onData((data) => {
    invoke("write_pty", { id: props.ptyId, data: Array.from(new TextEncoder().encode(data)) });
  });

  // Click anywhere in the pane re-focuses, so typing routes to the PTY.
  hostEl.value.addEventListener("mousedown", () => term?.focus());

  resizeObserver = new ResizeObserver(() => fitFont());
  resizeObserver.observe(hostEl.value.parentElement ?? hostEl.value);
}

onMounted(async () => {
  // Start collapsed — a thin bar (title + status), always on top
  invoke("set_window_size", { label: `float-${props.ptyId}`, width: 240, height: 36 }).catch(() => {});

  // Live mirror: piggyback on the SAME pty-data-{id} event the main window
  // consumes (Tauri app.emit broadcasts to every window — no extra daemon
  // stream). Phase gates it: loading → buffer, live → write, idle → drop.
  unlistenData = await listen<number[]>(`pty-data-${props.ptyId}`, (event) => {
    const bytes = new Uint8Array(event.payload);
    if (phase === "live") term?.write(bytes);
    else if (phase === "loading") liveQueue.push(bytes);
    // idle (collapsed): drop — we resync via snapshot on next expand
  });

  // Snapshot reply from the main XTerm: its exact current screen (serialized) +
  // its grid dims. Match the grid so the screen renders identically, then write
  // the snapshot, drain buffered live bytes, go live.
  unlistenSnap = await listen<{ data: string; cols: number; rows: number }>(
    `float-snap-${props.ptyId}`,
    (event) => {
      if (phase !== "loading") return;
      if (snapTimer) { clearTimeout(snapTimer); snapTimer = null; }
      const { data, cols, rows } = event.payload;
      // Fix the float's grid to the main window's, then font-scale to fit.
      if (cols > 0 && rows > 0) { term?.resize(cols, rows); fitFont(); }
      term?.reset();
      term?.write(data);
      while (liveQueue.length) term?.write(liveQueue.shift()!);
      phase = "live";
      term?.focus();
    },
  );

  unlistenHook = await listen<string>(`pty-hook-${props.ptyId}`, (event) => {
    const s = event.payload;
    if (s === "running") { status.value = "running"; isAgentSession.value = true; }
    else if (s === "waiting") status.value = "waiting";
    else if (s === "done") {
      status.value = "done";
      if (doneTimer) clearTimeout(doneTimer);
      doneTimer = setTimeout(() => { if (status.value === "done") status.value = "idle"; }, 4000);
    }
  });

  // Corner-snapping: dragging fires move events continuously; 220ms after the
  // last one (drag settled) snap the window to its nearest corner + re-stack.
  // Snapping itself moves the window → guard against an infinite snap loop with
  // a short suppression window.
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  let snapping = false;
  unlistenMoved = await getCurrentWindow().onMoved(() => {
    if (snapping) return;
    if (moveTimer) clearTimeout(moveTimer);
    moveTimer = setTimeout(async () => {
      snapping = true;
      await invoke("snap_float_window", { label: `float-${props.ptyId}` }).catch(() => {});
      setTimeout(() => { snapping = false; }, 150);
    }, 220);
  });
});

onBeforeUnmount(() => {
  resizeObserver?.disconnect();
  unlistenData?.();
  unlistenSnap?.();
  unlistenHook?.();
  unlistenMoved?.();
  if (doneTimer) clearTimeout(doneTimer);
  if (snapTimer) clearTimeout(snapTimer);
  if (moveTimer) clearTimeout(moveTimer);
  term?.dispose();
});

watch(() => ui.activeTheme, (t) => { if (term) term.options.theme = t.xterm; });
// Font FAMILY follows the user pref; SIZE is auto-fit to the window (fitFont).
watch(() => ui.terminalFont, (font) => {
  if (!term) return;
  term.options.fontFamily = font;
  fitFont();
});
</script>

<style>
html, body, #app {
  width: 100%; height: 100%;
  margin: 0; padding: 0;
  overflow: hidden;
  background: transparent;
}
</style>

<style scoped>
.bubble-root {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  font-family: var(--font-ui, system-ui, sans-serif);
  /* transparent bg — OS clips shadow to border-radius */
  background: transparent;
}

/* ── Collapsed: a thin bar filling the window — icon · title · status · close ── */
.bubble-bar {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 0 8px;
  background: var(--bg-panel, #1c1c1e);
  border-radius: 8px;
  border: 1px solid rgba(255,255,255,0.14);
  cursor: pointer;
  color: var(--text-secondary, #aaa);
  transition: background 0.15s, color 0.15s;
  -webkit-app-region: drag;
}
.bubble-bar:hover {
  background: var(--bg-hover, #2a2a2c);
  color: var(--text-primary, #eee);
}

.bar-icon { flex-shrink: 0; color: var(--accent, #7aa2f7); }

.bar-title {
  flex: 1;
  font-size: 11px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.bar-status-dot {
  flex-shrink: 0;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  font-size: 7px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.bar-close {
  flex-shrink: 0;
  width: 18px;
  height: 18px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--text-muted, #666);
  cursor: pointer;
  padding: 0;
  opacity: 0;
  transition: opacity 0.12s, background 0.12s, color 0.12s;
  -webkit-app-region: no-drag;
}
.bubble-bar:hover .bar-close { opacity: 0.7; }
.bar-close:hover { opacity: 1 !important; background: rgba(239,68,68,0.25); color: var(--red, #ef4444); }

.bubble-btn-close:hover {
  background: rgba(239,68,68,0.25);
  color: var(--red, #ef4444);
}

/* ── Expanded: fills the whole window edge-to-edge. ── */
.bubble-root.expanded {
  background: var(--bg-base, #0d0d0d);
  border: 1px solid rgba(255,255,255,0.1);
}

.bubble-header {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 10px;
  height: 32px;
  flex-shrink: 0;
  background: var(--bg-panel, #111);
  border-bottom: 1px solid rgba(255,255,255,0.06);
  user-select: none;
  -webkit-app-region: drag;
}

.bubble-title {
  flex: 1;
  font-size: 11px;
  color: var(--text-secondary, #888);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  -webkit-app-region: drag;
}

.bubble-actions {
  display: flex;
  gap: 2px;
  -webkit-app-region: no-drag;
}

.bubble-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  border: none;
  background: transparent;
  color: var(--text-muted, #666);
  border-radius: 3px;
  cursor: pointer;
  padding: 0;
  transition: background 0.1s, color 0.1s;
}
.bubble-btn:hover {
  background: rgba(255,255,255,0.08);
  color: var(--text-primary, #e0e0e0);
}

.bubble-term {
  flex: 1;
  overflow: hidden;
  padding: 4px;
}
.bubble-term :deep(.xterm) { height: 100%; }
.bubble-term :deep(.xterm-viewport) { background: transparent !important; }

.bubble-status-dot {
  position: absolute;
  bottom: 6px;
  right: 6px;
  width: 9px;
  height: 9px;
  border-radius: 50%;
  border: 1.5px solid rgba(0,0,0,0.4);
}
.bubble-header .bubble-status-dot {
  position: static;
  width: 7px;
  height: 7px;
  border: none;
}
.status-running { background: #f59e0b; }
.status-waiting { background: #3b82f6; }
.status-done    { background: #84cc16; }
</style>
