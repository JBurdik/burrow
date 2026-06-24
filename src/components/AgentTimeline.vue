<template>
  <div class="tl-wrap" ref="wrap">
    <div v-if="!turns.length" class="tl-empty">No agent turns recorded yet.</div>
    <template v-else>
      <div class="tl-toolbar">
        <button class="tl-fit-btn" @click="autoFit" title="Fit all turns in view">Fit</button>
        <span class="tl-range-label">{{ rangeLabel }}</span>
      </div>
      <svg
        :width="svgW" :height="svgH"
        class="tl-svg"
        @mousedown="onMouseDown"
        @mousemove="onMouseMove"
        @mouseup="onMouseUp"
        @mouseleave="onMouseLeave"
        @wheel.prevent="onWheel"
      >
        <!-- clip path so bars don't spill over labels -->
        <defs>
          <clipPath id="tl-chart-clip">
            <rect :x="LABEL_W" y="0" :width="chartW" :height="svgH" />
          </clipPath>
        </defs>

        <!-- Axis ticks -->
        <g class="tl-axis" :transform="`translate(0, ${AXIS_H - 1})`">
          <line :x1="LABEL_W" y1="0" :x2="svgW" y2="0" class="tl-axis-line" />
          <g v-for="tick in ticks" :key="tick.ms">
            <line
              :x1="tick.x" y1="0" :x2="tick.x" :y2="svgH - AXIS_H + 2"
              class="tl-tick-line"
            />
            <text :x="tick.x" y="-4" class="tl-tick-label" text-anchor="middle">
              {{ tick.label }}
            </text>
          </g>
        </g>

        <!-- Turn rows -->
        <g clip-path="url(#tl-chart-clip)">
          <g
            v-for="(turn, i) in reversedTurns"
            :key="turn.id"
            :transform="`translate(0, ${AXIS_H + i * ROW_H})`"
          >
            <!-- row background track -->
            <rect
              :x="LABEL_W" y="3" :width="chartW" :height="ROW_H - 6"
              class="tl-track"
            />
            <!-- segments -->
            <rect
              v-for="seg in turn.segments"
              :key="seg.start"
              :x="Math.max(LABEL_W, tx(seg.start))"
              y="3"
              :width="Math.max(0, tx(seg.end ?? now) - Math.max(LABEL_W, tx(seg.start)))"
              :height="ROW_H - 6"
              :class="`tl-seg tl-seg-${seg.state}`"
              @mouseenter="showTip($event, turn, seg)"
              @mouseleave="hideTip"
            />
          </g>
        </g>

        <!-- Row labels (left side, outside clip) -->
        <g v-for="(turn, i) in reversedTurns" :key="`lbl-${turn.id}`">
          <text
            :x="LABEL_W - 6"
            :y="AXIS_H + i * ROW_H + ROW_H / 2 + 3"
            class="tl-row-label"
            text-anchor="end"
          >T{{ turn.id + 1 }}</text>
        </g>
      </svg>

      <!-- Tooltip overlay -->
      <div
        v-if="tip"
        class="tl-tip"
        :style="{ left: tip.x + 'px', top: tip.y + 'px' }"
      >
        <div class="tl-tip-state" :class="`tl-tip-${tip.state}`">{{ tip.state }}</div>
        <div class="tl-tip-dur">{{ tip.duration }}</div>
        <div v-if="tip.turnDur" class="tl-tip-turn">turn: {{ tip.turnDur }}</div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from "vue";
import { useAgentHistoryStore } from "@/stores/agentHistory";
import type { AgentTurn, TurnSegment } from "@/stores/agentHistory";

const props = defineProps<{ ptyId: number }>();
const store = useAgentHistoryStore();

const LABEL_W = 32;
const ROW_H = 14;
const AXIS_H = 18;
const PAD = 2;

const wrap = ref<HTMLElement>();
const svgW = ref(300);
const chartW = computed(() => Math.max(0, svgW.value - LABEL_W));

const turns = computed(() => store.getTimeline(props.ptyId));
const reversedTurns = computed(() => [...turns.value].reverse());
const svgH = computed(() => AXIS_H + reversedTurns.value.length * ROW_H + PAD);

// Live "now" ticks every second while a turn is in progress
const now = ref(Date.now());
let nowTimer: ReturnType<typeof setInterval> | undefined;
watch(
  () => store.getTimeline(props.ptyId).some((t) => !t.end),
  (live) => {
    clearInterval(nowTimer);
    if (live) nowTimer = setInterval(() => { now.value = Date.now(); }, 1000);
  },
  { immediate: true },
);

// ── Time window ─────────────────────────────────────────────────────────────
const viewStart = ref(0);
const viewDuration = ref(60_000);
let userInteracted = false;

function autoFit() {
  const t = turns.value;
  if (!t.length) return;
  const lo = t[0].start;
  const hi = Math.max(now.value, t[t.length - 1].end ?? now.value);
  const span = hi - lo || 5_000;
  viewStart.value = lo - span * 0.05;
  viewDuration.value = span * 1.1;
  userInteracted = false;
}

watch(turns, () => { if (!userInteracted) autoFit(); }, { immediate: true });

// time ms → svg x pixel
function tx(ms: number): number {
  return LABEL_W + ((ms - viewStart.value) / viewDuration.value) * chartW.value;
}

// ── Tick generation ──────────────────────────────────────────────────────────
const NICE_INTERVALS = [
  500, 1_000, 2_000, 5_000, 10_000, 30_000, 60_000,
  120_000, 300_000, 600_000, 1_800_000, 3_600_000,
];

function fmtMs(ms: number): string {
  if (ms < 1_000) return `${Math.round(ms)}ms`;
  if (ms < 60_000) return `${(ms / 1_000).toFixed(ms < 10_000 ? 1 : 0)}s`;
  return `${Math.floor(ms / 60_000)}m${Math.floor((ms % 60_000) / 1_000)}s`;
}

function fmtTick(ms: number, interval: number): string {
  const rel = ms - viewStart.value;
  if (interval < 60_000) return `+${(rel / 1_000).toFixed(rel < 10_000 ? 1 : 0)}s`;
  return `+${Math.round(rel / 60_000)}m`;
}

const ticks = computed(() => {
  const minGapPx = 60;
  const targetCount = chartW.value / minGapPx;
  const rawInterval = viewDuration.value / targetCount;
  const interval = NICE_INTERVALS.find((n) => n >= rawInterval) ?? NICE_INTERVALS[NICE_INTERVALS.length - 1];
  const viewEnd = viewStart.value + viewDuration.value;
  const first = Math.ceil(viewStart.value / interval) * interval;
  const result: { ms: number; x: number; label: string }[] = [];
  for (let ms = first; ms < viewEnd; ms += interval) {
    result.push({ ms, x: tx(ms), label: fmtTick(ms, interval) });
  }
  return result;
});

const rangeLabel = computed(() => fmtMs(viewDuration.value));

// ── Pan & zoom ───────────────────────────────────────────────────────────────
let dragging = false;
let dragStartX = 0;
let dragViewStart = 0;

function onMouseDown(e: MouseEvent) {
  dragging = true;
  dragStartX = e.clientX;
  dragViewStart = viewStart.value;
}

function onMouseMove(e: MouseEvent) {
  if (!dragging) return;
  userInteracted = true;
  const dx = e.clientX - dragStartX;
  const msPerPx = viewDuration.value / chartW.value;
  viewStart.value = dragViewStart - dx * msPerPx;
}

function onMouseUp() { dragging = false; }
function onMouseLeave() { dragging = false; hideTip(); }

function onWheel(e: WheelEvent) {
  userInteracted = true;
  const factor = e.deltaY > 0 ? 1.18 : 0.85;
  const rect = wrap.value?.getBoundingClientRect();
  const ratio = rect ? Math.max(0, (e.clientX - rect.left - LABEL_W) / chartW.value) : 0.5;
  const pivot = viewStart.value + ratio * viewDuration.value;
  viewDuration.value = Math.max(1_000, Math.min(86_400_000, viewDuration.value * factor));
  viewStart.value = pivot - ratio * viewDuration.value;
}

// ── Tooltip ──────────────────────────────────────────────────────────────────
const tip = ref<{ x: number; y: number; state: string; duration: string; turnDur?: string } | null>(null);

function showTip(e: MouseEvent, turn: AgentTurn, seg: TurnSegment) {
  const rect = wrap.value?.getBoundingClientRect();
  const segDur = (seg.end ?? now.value) - seg.start;
  const turnDur = turn.end ? turn.end - turn.start : undefined;
  tip.value = {
    x: (e.clientX - (rect?.left ?? 0)) + 12,
    y: (e.clientY - (rect?.top ?? 0)) - 8,
    state: seg.state,
    duration: fmtMs(segDur),
    turnDur: turnDur ? fmtMs(turnDur) : undefined,
  };
}
function hideTip() { tip.value = null; }

// ── Resize ───────────────────────────────────────────────────────────────────
let ro: ResizeObserver;
onMounted(() => {
  ro = new ResizeObserver(([entry]) => {
    svgW.value = entry.contentRect.width;
  });
  if (wrap.value) ro.observe(wrap.value);
});
onUnmounted(() => {
  ro?.disconnect();
  clearInterval(nowTimer);
});
</script>

<style scoped>
.tl-wrap {
  width: 100%;
  position: relative;
  user-select: none;
}

.tl-empty {
  font-size: 11px;
  color: var(--text-muted);
  padding: 16px;
  text-align: center;
}

.tl-toolbar {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 2px 6px;
  border-bottom: 1px solid var(--border);
}

.tl-fit-btn {
  font-size: 10px;
  font-weight: 500;
  padding: 2px 7px;
  border-radius: 3px;
  border: 1px solid var(--border);
  background: none;
  color: var(--text-muted);
  cursor: pointer;
  font-family: var(--font-ui);
  transition: background 0.1s, color 0.1s;
}
.tl-fit-btn:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.tl-range-label {
  font-size: 10px;
  color: var(--text-muted);
  font-family: var(--font-mono);
}

.tl-svg {
  display: block;
  width: 100%;
  cursor: grab;
  overflow: visible;
}
.tl-svg:active { cursor: grabbing; }

/* SVG element styles via deep scoped */
.tl-axis-line {
  stroke: var(--border);
  stroke-width: 1;
}

.tl-tick-line {
  stroke: var(--border);
  stroke-width: 1;
  stroke-dasharray: 3 3;
  opacity: 0.5;
}

.tl-tick-label {
  font-size: 9px;
  fill: var(--text-muted);
  font-family: var(--font-mono, monospace);
}

.tl-track {
  fill: color-mix(in srgb, var(--border) 30%, transparent);
  rx: 3;
}

.tl-row-label {
  font-size: 9px;
  fill: var(--text-muted);
  font-family: var(--font-mono, monospace);
}

.tl-seg {
  rx: 2;
  opacity: 0.85;
  transition: opacity 0.1s;
}
.tl-seg:hover { opacity: 1; }

.tl-seg-running   { fill: var(--status-running,    #fb923c); }
.tl-seg-waiting   { fill: var(--status-waiting,    #3b82f6); }
.tl-seg-permission{ fill: var(--status-permission, #f59e0b); }
.tl-seg-error     { fill: var(--status-error,      #ef4444); }

/* Tooltip */
.tl-tip {
  position: absolute;
  pointer-events: none;
  background: color-mix(in srgb, var(--bg-panel) 95%, black);
  border: 1px solid var(--border);
  border-radius: 5px;
  padding: 5px 8px;
  font-size: 11px;
  line-height: 1.5;
  color: var(--text-secondary);
  white-space: nowrap;
  z-index: 10;
  box-shadow: 0 4px 12px rgba(0,0,0,0.4);
}
.tl-tip-state {
  font-weight: 600;
  font-size: 11px;
}
.tl-tip-dur  { font-size: 10px; color: var(--text-muted); }
.tl-tip-turn { font-size: 10px; color: var(--text-muted); }

.tl-tip-running    { color: var(--status-running,    #fb923c); }
.tl-tip-waiting    { color: var(--status-waiting,    #3b82f6); }
.tl-tip-permission { color: var(--status-permission, #f59e0b); }
.tl-tip-error      { color: var(--status-error,      #ef4444); }
</style>
