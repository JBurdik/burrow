<!--
  MissionControl.vue — a tank-style task dashboard, in its own Tauri window.

  Mounted by main.ts when the window label is `mission`. Each "task" is a
  headless `claude` PTY (Burrow's native PTY, not tmux): we spawn it from a
  prompt, watch its status dot via the global hook server (pty-hook-{id}),
  attach a live xterm on demand, and read the final assistant reply straight
  from Claude's JSONL transcript when the Stop hook fires.

  POC scope: spawn + list + live terminal + result capture + sequential queue.
  Task metadata is mirrored to localStorage so reopening the window keeps the
  history; live attach only works while the daemon PTY is still alive.
-->
<template>
  <div class="mc" :class="{ blank: !activeWs }">
   <template v-if="activeWs">
    <!-- ── Left rail: task list grouped by project (cwd) ───────────────── -->
    <aside class="rail">
      <header class="rail-head">
        <PhCrosshair :size="17" weight="bold" class="brand-mark" />
        <h1>Mission Control</h1>
      </header>

      <!-- Scope = the active workspace (chosen in the sidebar). New tasks target it. -->
      <div class="ws-scope">
        <img v-if="wsIcon(activeWs.id)" class="ws-ico-img" :src="wsIcon(activeWs.id)" alt="" />
        <PhGitBranch v-else-if="activeWs.parent_id" :size="15" class="ws-ico" />
        <PhFolder v-else :size="15" weight="fill" class="ws-ico" />
        <span class="ws-meta">
          <span class="ws-name">{{ activeWs.name }}</span>
          <span class="ws-path">{{ shortCwd(activeWs.path) }}</span>
        </span>
      </div>

      <button class="btn primary new-btn" @click="openComposer"><PhPlus :size="14" weight="bold" /> New task</button>

      <div class="rail-summary">
        <span class="chip" :class="{ on: countBy('running') }"><em class="dot running" />{{ countBy('running') }}<small>running</small></span>
        <span class="chip" :class="{ on: countBy('waiting') }"><em class="dot waiting" />{{ countBy('waiting') }}<small>waiting</small></span>
        <span class="chip" :class="{ on: countBy('done') }"><em class="dot done" />{{ countBy('done') }}<small>done</small></span>
        <span class="spacer" />
        <button class="btn ghost xs" v-if="tasks.some(t => !t.alive)" @click="clearDead">clear finished</button>
      </div>

      <section class="queue" v-if="queue.length">
        <h2>Queue · {{ queue.length }}</h2>
        <ul>
          <li v-for="(q, i) in queue" :key="q.qid">
            <span class="qnum">{{ i + 1 }}</span>
            <span class="qtext">{{ q.prompt }}</span>
            <button class="x" title="remove" @click="queue.splice(i, 1)"><PhX :size="11" /></button>
          </li>
        </ul>
      </section>

      <div v-if="!tasks.length" class="rail-empty">
        No tasks yet.<br />Hit <strong>＋ New</strong> to spawn one.
      </div>

      <!-- Projects = tasks grouped by working dir -->
      <nav class="tasklist">
        <div v-for="proj in projects" :key="proj.key" class="proj">
          <div class="proj-head">{{ proj.label }}</div>
          <ul>
            <li
              v-for="t in proj.tasks"
              :key="t.id"
              class="task-row"
              :class="{ active: t.id === selectedId }"
              @click="selectedId = t.id"
            >
              <em class="dot" :class="t.status" />
              <span class="row-title">{{ t.title }}</span>
              <PhArrowSquareOut v-if="t.handedOff" class="row-handoff" :size="12" weight="bold" title="handed off to a terminal tab" />
              <span class="row-meta">{{ statusLabel(t) }}</span>
            </li>
          </ul>
        </div>
      </nav>
    </aside>

    <!-- ── Center: selected-task conversation + continue bar ───────────── -->
    <main class="detail">
      <template v-if="selected">
        <header class="detail-head">
          <em class="dot" :class="selected.status" :title="selected.status" />
          <h2>{{ selected.title }}</h2>
          <span class="model" v-if="selected.model && selected.model !== 'default'">{{ selected.model }}</span>
          <span class="cwd-hint">{{ shortCwd(selected.cwd) }}</span>
          <span class="status-text">· {{ statusLabel(selected) }}</span>
          <span class="handoff-badge" v-if="selected.handedOff" title="this task's live session was handed off to a terminal tab"><PhArrowSquareOut :size="11" weight="bold" /> in tab</span>
          <span class="spacer" />
          <button class="btn tiny" @click="openTerminal(selected)" :disabled="!selected.alive || selected.handedOff" title="attach live terminal"><PhTerminal :size="12" /> Terminal</button>
          <button v-if="selected.handedOff" class="btn tiny" @click="focusHandoff(selected)" title="jump to the terminal tab running this task"><PhArrowSquareOut :size="12" /> Focus tab</button>
          <button v-else class="btn tiny" @click="handoffToTab(selected)" :disabled="!selected.alive" title="hand this live session off to a real terminal tab (keeps tracking here)"><PhArrowRight :size="12" /> Hand off</button>
          <button class="btn tiny danger" @click="deleteTask(selected)" title="kill + remove"><PhTrash :size="12" /> Delete</button>
        </header>

        <div class="convo-full" ref="convoEl">
          <div v-for="(turn, i) in selected.turns" :key="i" class="turn" :class="turn.role">
            <span class="role"><PhCaretRight v-if="turn.role === 'user'" :size="13" weight="bold" /><PhRobot v-else :size="14" /></span>
            <div class="ttext">{{ turn.text }}</div>
          </div>
          <div v-if="selected.status === 'running'" class="working"><PhRobot :size="13" /> working…</div>
          <div v-if="selected.turns.length === 1 && selected.status !== 'running'" class="no-result">no result captured yet</div>
        </div>

        <!-- Handed off → a terminal tab owns input; lock the bar to avoid two writers. -->
        <div class="continue-bar handed" v-if="selected.handedOff">
          <PhArrowSquareOut :size="13" />
          <span>Handed off to a terminal tab — input lives there now.</span>
          <span class="spacer" />
          <button class="btn primary" @click="focusHandoff(selected)"><PhArrowSquareOut :size="13" /> Focus tab</button>
        </div>
        <!-- Persistent continue bar (tank's "send & continue") -->
        <div class="continue-bar" v-else-if="selected.alive">
          <textarea
            v-model="selected.followup"
            rows="2"
            placeholder="continue this conversation (Enter to send, Shift+Enter for newline)"
            :disabled="selected.status === 'running'"
            @keydown.enter.exact.prevent="sendFollowup(selected)"
          ></textarea>
          <div class="cb-actions">
            <button class="btn ghost" @click="stopGeneration(selected)" :disabled="selected.status !== 'running'">stop</button>
            <button class="btn primary" :disabled="selected.status === 'running' || !selected.followup.trim()" @click="sendFollowup(selected)">send &amp; continue</button>
          </div>
        </div>
        <div class="continue-bar dead" v-else-if="!selected.alive">
          <span>PTY finished — this task is read-only.</span>
          <span class="spacer" />
          <button class="btn primary" @click="resumeTask(selected)" title="respawn claude --resume and continue"><PhArrowClockwise :size="13" /> Resume</button>
        </div>
      </template>

      <div v-else class="empty-detail">
        <div class="empty-inner">
          <h2>No task selected</h2>
          <p>Pick a task on the left, or hit <strong>＋ New task</strong> to spawn one.</p>
        </div>
      </div>
    </main>
   </template>

    <!-- ── Blank state: no active workspace → nothing to scope a task to ──── -->
    <div v-else class="mc-blank">
      <div class="blank-inner">
        <PhCrosshair :size="34" weight="duotone" class="blank-mark" />
        <h2>Pick a workspace</h2>
        <p>Mission Control runs Claude tasks against a workspace. Select one in the sidebar on the left to spawn and track agent tasks here.</p>
        <span class="blank-hint"><PhArrowLeft :size="13" /> choose a workspace to begin</span>
      </div>
    </div>

    <!-- ── New-task composer (modal) ───────────────────────────────────── -->
    <div v-if="composerOpen" class="composer-modal" @click.self="composerOpen = false">
      <div class="composer-box" @drop.prevent="onComposerDrop" @dragover.prevent>
        <header class="cm-head">
          <PhCrosshair :size="14" weight="bold" class="cm-brand" />
          <span>New task</span>
          <span class="spacer" />
          <button class="btn tiny icon-btn" @click="composerOpen = false"><PhX :size="12" /></button>
        </header>

        <div class="fld prompt-fld">
          <div class="prompt-label-row">
            <span>Prompt</span>
            <span class="prompt-hint">Type <kbd>@</kbd> to reference a file · <kbd>⌘↵</kbd> to run</span>
          </div>
          <div class="prompt-wrap">
            <textarea
              ref="promptTextarea"
              v-model="draft.prompt"
              rows="5"
              placeholder="What should Claude do?"
              @keydown.meta.enter="runDraft"
              @paste="onComposerPaste"
              @input="onPromptInput"
              @keydown.escape="showFilePicker = false"
            ></textarea>
            <!-- @-trigger file picker dropdown -->
            <div v-if="showFilePicker" class="at-picker">
              <div class="at-header"><PhMagnifyingGlass :size="11" /> <span>{{ fileSearchQuery || "files in workspace" }}</span></div>
              <ul>
                <li v-for="p in fileSearchResults" :key="p" @click="selectFileFromPicker(p)" class="at-opt">
                  <PhFile :size="12" />
                  <span class="at-name">{{ fileBasename(p) }}</span>
                  <span class="at-path">{{ shortCwd(p) }}</span>
                </li>
              </ul>
            </div>
          </div>
        </div>

        <!-- Attachments: images + tagged files -->
        <div v-if="draft.images.length || draft.files.length" class="img-chips">
          <span v-for="(p, i) in draft.images" :key="`img-${i}`" class="img-chip">
            <PhImage :size="12" /> {{ p.split('/').pop() }}
            <button class="x" @click="removeImage(i)"><PhX :size="10" /></button>
          </span>
          <span v-for="(p, i) in draft.files" :key="`file-${i}`" class="img-chip file-chip">
            <PhFile :size="12" /> {{ fileBasename(p) }}
            <button class="x" @click="removeFile(i)"><PhX :size="10" /></button>
          </span>
        </div>

        <!-- Attach buttons -->
        <div class="attach-row">
          <button class="btn ghost xs attach-btn" @click="pickFiles" type="button" title="Attach context files (sent as @path references)">
            <PhPaperclip :size="13" /> Attach files
          </button>
          <span class="attach-hint">or drop images · type <kbd>@</kbd> for inline ref</span>
        </div>

        <div class="fld">
          <span>Model</span>
          <div class="dd">
            <button type="button" class="dd-btn" :class="{ open: modelMenuOpen }" @click="modelMenuOpen = !modelMenuOpen">
              <span class="dd-val">{{ modelLabel }}</span>
              <PhCaretDown :size="12" class="dd-chev" :class="{ open: modelMenuOpen }" />
            </button>
            <div v-if="modelMenuOpen" class="dd-backdrop" @click="modelMenuOpen = false" />
            <ul v-if="modelMenuOpen" class="dd-menu">
              <li
                v-for="m in MODELS"
                :key="m.value"
                class="dd-opt"
                :class="{ sel: m.value === draft.model }"
                @click="draft.model = m.value; modelMenuOpen = false"
              >
                <span>{{ m.label }}</span>
                <PhCheck v-if="m.value === draft.model" :size="13" weight="bold" />
              </li>
            </ul>
          </div>
        </div>
        <p class="cwd-line">
          <PhFolder :size="12" weight="fill" class="dir-ico" /> {{ activeWs?.name }}
          <span class="cwd-path">{{ shortCwd(draft.cwd) }}</span>
        </p>

        <!-- Feature 5 — isolate in a git worktree -->
        <label class="iso">
          <input type="checkbox" v-model="draft.isolate" />
          <span>Isolate in a git worktree <em>(off the active repo — parallel-safe)</em></span>
        </label>
        <div v-if="draft.isolate" class="branch-row">
          <PhGitBranch :size="13" />
          <span class="branch-prefix">mission/</span>
          <input v-model="draft.branch" class="branch-input" type="text" placeholder="m-a1b2c3 (auto)" spellcheck="false" />
        </div>
        <div v-if="worktreeError" class="wt-err">{{ worktreeError }}</div>

        <!-- Skip permission prompts — claude runs unattended (no interactive gate) -->
        <label class="iso danger-toggle">
          <input type="checkbox" v-model="draft.skipPerms" />
          <span><PhWarning :size="13" class="warn-ico" /> Skip permissions <em>(<code>--dangerously-skip-permissions</code> — no approval prompts; claude can run any tool)</em></span>
        </label>

        <div class="composer-actions">
          <button class="btn primary" :disabled="!canRun" @click="runDraft">Run now</button>
          <button class="btn" :disabled="!canRun" @click="enqueueDraft">Add to queue</button>
          <span class="spacer" />
          <label class="conc"><span>concurrent</span><input v-model.number="maxConcurrent" type="number" min="1" max="6" /></label>
        </div>
      </div>
    </div>

    <!-- ── Terminal modal (attach-on-demand) ───────────────────────────── -->
    <div v-if="termTaskId" class="term-modal" @click.self="closeTerminal">
      <div class="term-box">
        <header class="term-head">
          <em class="dot" :class="termTaskStatus" />
          <span>{{ termTaskTitle }}</span>
          <span class="spacer" />
          <button class="btn tiny" @click="sendCtrlC">^C</button>
          <button class="btn tiny" @click="closeTerminal">close</button>
        </header>
        <div ref="termHost" class="term-host"></div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive, computed, watch, inject, onMounted, onBeforeUnmount, nextTick } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  PhCrosshair, PhPlus, PhFolder, PhGitBranch, PhRobot, PhTerminal,
  PhArrowRight, PhArrowLeft, PhArrowClockwise, PhTrash, PhImage, PhX, PhWarning,
  PhCaretRight, PhCaretDown, PhCheck, PhArrowSquareOut, PhPaperclip, PhFile,
  PhMagnifyingGlass,
} from "@phosphor-icons/vue";
import { useWorkspaceStore } from "@/stores/workspace";
import { useUIStore } from "@/stores/ui";
import { useNotificationsStore } from "@/stores/notifications";
import { playSound } from "@/lib/sounds";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";

const wsStore = useWorkspaceStore();
const ui = useUIStore();
const notifStore = useNotificationsStore();
// App.vue provides the active workspace's Terminal component (for "send to tab").
const activeTerm = inject<() => {
  spawnAgent: (cmd: string) => void;
  adoptPty: (opts: { ptyId: number; cwd: string; title: string; sessionId?: string }) => void;
  focusLeaf: (ptyId: number) => void;
} | undefined>("activeTerm", () => undefined);

type Status = "running" | "waiting" | "done" | "error" | "idle";
type Role = "user" | "assistant";
interface Turn { role: Role; text: string }

interface Task {
  id: string;        // crypto.randomUUID() — also Claude's --session-id
  ptyId: number;     // headless PTY id (offset id-space, see PTY_BASE)
  workspaceId: number | null; // the Burrow workspace this task belongs to
  title: string;
  prompt: string;    // the first prompt (kept for the title/seed)
  cwd: string;
  model: string;
  status: Status;
  turns: Turn[];     // full conversation: user prompts + assistant replies
  followup: string;  // draft text for the in-card follow-up input
  expanded: boolean; // show all turns vs last 4
  alive: boolean;
  handedOff: boolean; // PTY adopted by a real terminal tab — that tab owns input,
                      // MC keeps tracking status read-only (no double-input).
  createdAt: number;
}

// Row shape exchanged with the Rust mission_tasks table (snake_case = serde).
interface TaskRow {
  id: string;
  workspace_id: number | null;
  pty_id: number | null;
  title: string | null;
  cwd: string | null;
  model: string | null;
  status: string | null;
  turns: string | null;       // JSON-encoded Turn[]
  created_at: number;
  handed_off: number | null;  // 1 = handed off to a terminal tab
}

const MODELS = [
  { value: "default", label: "Default" },
  { value: "fable", label: "Fable" },
  { value: "opus", label: "Opus" },
  { value: "sonnet", label: "Sonnet" },
  { value: "haiku", label: "Haiku" },
  { value: "opusplan", label: "Opus Plan" },
];

// Mission's PTY ids live in a high range so they never collide with the main
// window's per-window counter (both share the daemon's global PTY map). The
// counter is seeded each mount from the source of truth — the persisted tasks in
// SQLite (max stored pty_id) plus the daemon's live sessions — so it never reuses
// an id whose PTY is still running. Reusing one would attach a "new task" to an
// old, still-running claude (typing the launch command into its REPL instead of a
// fresh shell). No localStorage: the DB + daemon already know every taken id.
const PTY_BASE = 2_000_000;
let ptyCounter = 0;

function allocPtyId(): number {
  ptyCounter++;
  return PTY_BASE + ptyCounter;
}

// Push the sequence past an id we know is taken (restored task or live daemon
// session), so the next allocation can't collide with it.
function bumpPtySeqPast(ptyId: number) {
  if (ptyId < PTY_BASE) return;
  const seq = ptyId - PTY_BASE;
  if (seq > ptyCounter) ptyCounter = seq;
}

const tasks = ref<Task[]>([]);
const queue = ref<{ qid: string; prompt: string; cwd: string; model: string; isolate: boolean; branch: string; images: string[]; files: string[]; skipPerms: boolean }[]>([]);
const maxConcurrent = ref(1);

const maxConcurrentClamped = computed(() => Math.max(1, maxConcurrent.value));
const selectedId = ref<string | null>(null);
const composerOpen = ref(false);
const modelMenuOpen = ref(false);
const convoEl = ref<HTMLElement | null>(null);
const modelLabel = computed(() => MODELS.find((m) => m.value === draft.model)?.label ?? "Default");

// ── Workspace scope: the active workspace, chosen in Burrow's sidebar (no picker
// of our own — the sidebar already owns selection). Every new task targets it; with
// no active workspace the whole view falls back to a blank "pick a workspace" state.
const activeWs = computed(() => wsStore.active);
function wsIcon(id: number): string | undefined {
  return wsStore.icons[id];
}
// Switching workspace in the sidebar re-targets the composer at the new cwd.
watch(activeWs, (w) => { if (w) draft.cwd = w.path; });

const draft = reactive({
  prompt: "",
  cwd: "",
  model: "default",
  isolate: false,            // spawn in a fresh git worktree off the active repo
  branch: "",                // optional worktree branch name (else auto mission/m-XXXXXX)
  images: [] as string[],    // temp image paths attached to the first prompt
  files: [] as string[],     // file paths tagged with @file syntax
  skipPerms: false,          // launch claude with --dangerously-skip-permissions
});

// File search state for the @-trigger picker
const fileSearchQuery = ref("");
const fileSearchResults = ref<string[]>([]);
const showFilePicker = ref(false);
const filePickerAnchor = ref<{ top: number; left: number }>({ top: 0, left: 0 });
const promptTextarea = ref<HTMLTextAreaElement | null>(null);

const canRun = computed(() => draft.prompt.trim().length > 0 && draft.cwd.trim().length > 0);
const orderedTasks = computed(() => [...tasks.value].sort((a, b) => b.createdAt - a.createdAt));
const selected = computed(() => tasks.value.find((t) => t.id === selectedId.value) || null);

// A project = a Burrow workspace. Tasks group under their workspace; the label
// is the workspace name, falling back to the cwd basename for tasks whose
// workspace was deleted. Newest task first within each group.
function projectLabel(wsId: number | null, cwd: string): string {
  const w = wsId != null ? wsStore.workspaces.find((x) => x.id === wsId) : null;
  if (w) return w.name;
  return cwd.split("/").filter(Boolean).pop() || cwd;
}
// Tasks are scoped to the active workspace: only its own tasks + tasks living in
// a worktree off it (isolate spawns own workspace rows whose parent is the repo).
// Climb to the repo root first so the scope is the same whether you're sitting on
// the repo or one of its worktrees.
const scopeWsIds = computed<Set<number>>(() => {
  const a = activeWs.value;
  if (!a) return new Set();
  const root = a.parent_id != null ? (wsStore.workspaces.find((w) => w.id === a.parent_id) ?? a) : a;
  const ids = new Set<number>([root.id]);
  for (const w of wsStore.workspaces) if (w.parent_id === root.id) ids.add(w.id);
  return ids;
});
const projects = computed(() => {
  const scope = scopeWsIds.value;
  const groups = new Map<string, { label: string; tasks: Task[] }>();
  for (const t of orderedTasks.value) {
    if (t.workspaceId == null || !scope.has(t.workspaceId)) continue; // other workspace → hide
    const key = `ws:${t.workspaceId}`;
    if (!groups.has(key)) groups.set(key, { label: projectLabel(t.workspaceId, t.cwd), tasks: [] });
    groups.get(key)!.tasks.push(t);
  }
  return [...groups.entries()].map(([key, g]) => ({ key, label: g.label, tasks: g.tasks }));
});

// Resolve the workspace a cwd belongs to (exact path match), else the active
// workspace — so a task spawned in the active project nests under it.
function workspaceIdForCwd(cwd: string): number | null {
  const exact = wsStore.workspaces.find((w) => w.path === cwd);
  if (exact) return exact.id;
  return wsStore.active?.id ?? null;
}

function openComposer() {
  if (!activeWs.value) return;  // no active workspace → blank state covers the view
  draft.cwd = activeWs.value.path;
  composerOpen.value = true;
}

// ── Per-PTY plumbing: raw byte buffer (for terminal replay) + listeners ──────
const buffers = new Map<number, string>();      // ptyId → accumulated decoded output
const unlisteners = new Map<number, UnlistenFn[]>();

function countBy(s: Status) {
  const scope = scopeWsIds.value;
  return tasks.value.filter((t) => t.status === s && t.workspaceId != null && scope.has(t.workspaceId)).length;
}

function statusLabel(t: Task): string {
  switch (t.status) {
    case "running": return "working…";
    case "waiting": return "waiting for input";
    case "done": return "finished";
    case "error": return "error";
    default: return t.alive ? "idle" : "stopped";
  }
}

function shortCwd(p: string) {
  const home = "/Users/";
  return p.replace(home + (p.split("/")[2] || ""), "~");
}

// ── Spawn ────────────────────────────────────────────────────────────────────
function shquote(s: string) {
  return "'" + s.replace(/'/g, "'\\''") + "'";
}

async function spawnTask(prompt: string, cwd: string, model: string, images: string[] = [], skipPerms = false, files: string[] = []): Promise<Task> {
  const id = crypto.randomUUID();
  const ptyId = allocPtyId();
  // Prepend @file refs if the user tagged files via the picker (not already in prompt)
  const fileRefs = files.filter((f) => !prompt.includes(`@${f}`)).map((f) => `@${f}`).join("\n");
  const fullPrompt = fileRefs ? `${fileRefs}\n\n${prompt.trim()}` : prompt.trim();
  const task: Task = {
    id, ptyId,
    workspaceId: workspaceIdForCwd(cwd.trim()),
    title: prompt.trim().split("\n")[0].slice(0, 48) || "task",
    prompt: fullPrompt,
    cwd: cwd.trim(),
    model,
    status: "running",
    turns: [{ role: "user", text: prompt.trim() + (images.length ? `\n📎 ${images.length} image${images.length === 1 ? "" : "s"}` : "") + (files.length ? `\n📄 ${files.length} file${files.length === 1 ? "" : "s"} tagged` : "") }],
    followup: "",
    expanded: false,
    alive: true,
    handedOff: false,
    createdAt: Date.now(),
  };
  tasks.value.push(task);
  saveTask(task);

  buffers.set(ptyId, "");
  await wireTask(task);

  // Headless PTY = a shell; then we type the claude command into it.
  await invoke("create_pty", { id: ptyId, cwd: task.cwd, cols: 120, rows: 34 });

  const modelFlag = model && model !== "default" ? ` --model ${model}` : "";
  const permFlag = skipPerms ? " --dangerously-skip-permissions" : "";
  const imageFlags = images.map((p) => ` ${shquote(p)}`).join("");  // claude reads image paths as positional args
  const cmd = `claude --session-id ${id}${modelFlag}${permFlag} ${shquote(fullPrompt)}${imageFlags}\n`;
  // Small delay so the shell rc has finished and won't swallow the line.
  setTimeout(() => {
    invoke("write_pty", { id: ptyId, data: Array.from(new TextEncoder().encode(cmd)) }).catch(() => {});
  }, 450);

  return task;
}

async function wireTask(task: Task) {
  const offs: UnlistenFn[] = [];

  // PTY output → replay buffer (+ live terminal if attached).
  offs.push(await listen<number[]>(`pty-data-${task.ptyId}`, (ev) => {
    const bytes = new Uint8Array(ev.payload);
    const text = new TextDecoder().decode(bytes);
    let buf = (buffers.get(task.ptyId) || "") + text;
    if (buf.length > 262_144) buf = buf.slice(-262_144); // cap replay scrollback
    buffers.set(task.ptyId, buf);
    if (termTaskId.value === task.id && termInstance) termInstance.write(bytes);
  }));

  // Status dot via the global hook server (same channel Burrow tabs use).
  offs.push(await listen<string>(`pty-hook-${task.ptyId}`, (ev) => {
    const state = String(ev.payload);
    const t = tasks.value.find((x) => x.id === task.id);
    if (!t) return;
    const prev = t.status;
    if (state === "running") t.status = "running";
    else if (state === "waiting") t.status = "waiting";
    else if (state === "done") {
      t.status = "done";
      // Transcript flushes a beat after Stop — read the result shortly after.
      setTimeout(() => captureResult(t), 600);
      pumpQueue();   // free slot → start the next queued prompt
    }
    // Notify only on a real transition INTO done/waiting, and only when the user
    // isn't already looking at this task (Superset-style "finished while away").
    if (t.status !== prev) {
      if (t.status === "done") notifyTask(t, "done");
      else if (t.status === "waiting") notifyTask(t, "waiting");
    }
    saveTask(t);
  }));

  unlisteners.set(task.ptyId, offs);
}

// Feature #9 — notifications. The user is "watching" a task only when the Mission
// Control view is up, that task is selected, and the window is focused. If so, the
// transition is already visible → stay quiet. Otherwise: toast + sound, plus a
// system notification when the window isn't focused (mirrors Terminal.vue).
function isWatching(t: Task): boolean {
  return ui.mode === "mission" && selectedId.value === t.id && document.hasFocus();
}

async function notifyTask(t: Task, kind: "done" | "waiting") {
  if (isWatching(t)) return;
  const title = kind === "done" ? "Task complete" : "Task needs input";
  const body = t.title || (kind === "done" ? "Agent finished" : "Claude is waiting");
  notifStore.push({ type: kind === "done" ? "done" : "info", title, body, workspaceId: t.workspaceId ?? undefined });
  playSound(kind);
  if (!document.hasFocus()) {
    try {
      let granted = await isPermissionGranted();
      if (!granted) granted = (await requestPermission()) === "granted";
      if (granted) sendNotification({ title: "Burrow", body: `${kind === "done" ? "✓" : "⏳"} ${body}` });
    } catch { /* notifications optional */ }
  }
}

async function captureResult(t: Task) {
  try {
    const out = await invoke<{ text: string; error: { status: number | null; message: string } | null }>(
      "read_claude_outcome", { cwd: t.cwd, sessionId: t.id },
    );
    // Feature C — surface API errors (429/529/…) as a distinct `error` state
    // instead of a false `done`. tank does the same via isApiErrorMessage.
    if (out.error) {
      t.status = "error";
      const reason = apiErrorReason(out.error);
      const last = t.turns[t.turns.length - 1];
      if (!last || last.text !== reason) {
        t.turns.push({ role: "assistant", text: reason });
        if (t.id === selectedId.value) scrollConvo();
      }
      if (!isWatching(t)) {
        notifStore.push({ type: "error", title: "Task error", body: `${t.title} — ${reason.split("\n")[0]}`, workspaceId: t.workspaceId ?? undefined });
        playSound("waiting");
      }
      saveTask(t);
      return;
    }
    if (!out.text) return;
    // Append as an assistant turn — but only if it's new (each `done` reads the
    // transcript's *latest* assistant message; a follow-up produces a fresh one).
    const lastAssistant = [...t.turns].reverse().find((x) => x.role === "assistant");
    if (!lastAssistant || lastAssistant.text !== out.text) {
      t.turns.push({ role: "assistant", text: out.text });
      if (t.id === selectedId.value) scrollConvo();
      saveTask(t);
    }
  } catch { /* best-effort */ }
}

function apiErrorReason(e: { status: number | null; message: string }): string {
  const code = e.status ? ` (HTTP ${e.status})` : "";
  const hint = e.status === 429 ? " — rate limited"
    : e.status === 529 ? " — Anthropic overloaded"
    : "";
  const msg = (e.message || "").trim();
  return `⚠️ API error${code}${hint}${msg ? `\n${msg}` : ""}`;
}

// ── Follow-up: claude is still alive at its prompt in the same PTY, so we just
// type the next message into it (no --resume needed — that's tank's trick for a
// killed session; ours never dies). UserPromptSubmit hook flips status back to
// running automatically.
function sendFollowup(t: Task) {
  const text = t.followup.trim();
  if (!text || t.status === "running" || !t.alive) return;
  t.turns.push({ role: "user", text });
  t.followup = "";
  t.status = "running";
  // Collapse newlines: claude's REPL submits on Enter, so a raw \n would split
  // the message. Send the text then a carriage return to submit.
  const line = text.replace(/\r?\n/g, " ") + "\r";
  invoke("write_pty", { id: t.ptyId, data: Array.from(new TextEncoder().encode(line)) }).catch(() => {});
  scrollConvo();
  saveTask(t);
}

// Keep the conversation pinned to the newest turn when it's the open task.
function scrollConvo() {
  nextTick(() => {
    if (convoEl.value) convoEl.value.scrollTop = convoEl.value.scrollHeight;
  });
}

// ── Composer + queue ─────────────────────────────────────────────────────────
// Feature 5 — optionally spawn the task in a fresh git worktree off the workspace
// owning `cwd`, so parallel tasks never clobber each other's working tree. Mirrors
// the New-worktree dialog's path convention: <worktreesDir>/<repo>/<branch>.
async function resolveCwd(branchName: string, cwd: string, isolate: boolean): Promise<string> {
  if (!isolate) return cwd.trim();
  const wid = workspaceIdForCwd(cwd.trim());
  const repo = wsStore.workspaces.find((w) => w.id === wid) ?? wsStore.active;
  if (!repo || repo.parent_id) {  // need a top-level repo (no worktree-of-worktree)
    worktreeError.value = "Isolate needs a git-repo workspace (not a worktree).";
    return "";
  }
  const repoName = repo.path.split("/").filter(Boolean).pop() || "repo";
  // A typed name is slugified; otherwise a clean auto name — never the prompt text.
  const name = branchName.trim() ? slugify(branchName) : `m-${crypto.randomUUID().slice(0, 6)}`;
  const branch = `mission/${name}`;
  const path = `${ui.worktreesDir}/${repoName}/${branch}`;
  try {
    const wt = await wsStore.createWorktree(repo.id, branch, "HEAD", path);
    return wt.path;
  } catch (e) {
    worktreeError.value = `Worktree failed: ${e}`;
    return "";
  }
}

function slugify(s: string): string {
  return s.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 32) || "task";
}

const worktreeError = ref("");

async function runDraft() {
  if (!canRun.value) return;
  worktreeError.value = "";
  const cwd = await resolveCwd(draft.branch, draft.cwd, draft.isolate);
  if (!cwd) return;  // worktree creation failed — error shown, keep the modal open
  const images = [...draft.images];
  const files = [...draft.files];
  const t = await spawnTask(draft.prompt, cwd, draft.model, images, draft.skipPerms, files);
  selectedId.value = t.id;        // jump straight into the new task's detail
  resetDraft();
  composerOpen.value = false;
}

function enqueueDraft() {
  if (!canRun.value) return;
  // Queued tasks resolve their cwd at spawn time (so worktrees aren't created
  // until they actually run); the isolate flag rides along.
  queue.value.push({ qid: crypto.randomUUID(), prompt: draft.prompt.trim(), cwd: draft.cwd.trim(), model: draft.model, isolate: draft.isolate, branch: draft.branch.trim(), images: [...draft.images], files: [...draft.files], skipPerms: draft.skipPerms });
  resetDraft();
  composerOpen.value = false;
  pumpQueue();
}

function resetDraft() {
  draft.prompt = "";
  draft.images = [];
  draft.files = [];
  draft.isolate = false;
  draft.branch = "";
  draft.skipPerms = false;
  worktreeError.value = "";
  showFilePicker.value = false;
}

// Feature 3 — image attachments. Paste or drop images into the composer; each is
// saved to a temp file (save_temp_image) and its path is passed to claude as a
// positional arg on spawn (claude reads image paths from argv).
async function addImageFiles(files: FileList | File[]) {
  for (const f of Array.from(files)) {
    if (!f.type.startsWith("image/")) continue;
    try {
      const b64 = await fileToBase64(f);
      const ext = (f.type.split("/")[1] || "png").replace("jpeg", "jpg");
      const path = await invoke<string>("save_temp_image", { b64, ext });
      draft.images.push(path);
    } catch { /* skip unreadable image */ }
  }
}

function fileToBase64(f: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result).split(",")[1] || "");
    r.onerror = reject;
    r.readAsDataURL(f);
  });
}

function onComposerPaste(e: ClipboardEvent) {
  const items = e.clipboardData?.items;
  if (!items) return;
  const imgs = Array.from(items).filter((i) => i.type.startsWith("image/")).map((i) => i.getAsFile()).filter(Boolean) as File[];
  if (imgs.length) { e.preventDefault(); addImageFiles(imgs); }
}

function onComposerDrop(e: DragEvent) {
  const files = e.dataTransfer?.files;
  if (files?.length) { e.preventDefault(); addImageFiles(files); }
}

function removeImage(i: number) {
  draft.images.splice(i, 1);
}

// ── File tagging (@ references) ──────────────────────────────────────────────
// Files are included as @/absolute/path in the prompt — Claude Code reads these
// as file-content injections. Images stay separate (positional argv); plain files
// go into the prompt string so they're visible to the user in the conversation.

async function pickFiles() {
  try {
    const result = await openDialog({
      multiple: true,
      directory: false,
      defaultPath: draft.cwd || undefined,
    });
    if (!result) return;
    const paths = Array.isArray(result) ? result : [result];
    for (const p of paths) {
      if (!draft.files.includes(p)) draft.files.push(p);
    }
  } catch { /* dialog cancelled */ }
}

function removeFile(i: number) {
  draft.files.splice(i, 1);
}

function fileBasename(p: string) {
  return p.split("/").pop() || p;
}

// @-trigger in textarea: when user types @ we run a fuzzy search against the
// workspace dir and show a small inline picker.
async function onPromptInput(e: Event) {
  const ta = e.target as HTMLTextAreaElement;
  const val = ta.value;
  const pos = ta.selectionStart;
  // Find last @ before cursor that isn't preceded by a non-space char
  const before = val.slice(0, pos);
  const atIdx = before.lastIndexOf("@");
  if (atIdx === -1 || (atIdx > 0 && !/\s/.test(before[atIdx - 1]))) {
    showFilePicker.value = false;
    return;
  }
  const query = before.slice(atIdx + 1);
  if (query.includes(" ") || query.includes("\n")) {
    showFilePicker.value = false;
    return;
  }
  fileSearchQuery.value = query;
  // Search the workspace dir
  try {
    const entries = await invoke<{ name: string; path: string; is_dir: boolean }[]>(
      "read_dir_shallow", { path: draft.cwd }
    );
    const q = query.toLowerCase();
    fileSearchResults.value = entries
      .filter((e) => !e.is_dir && e.name.toLowerCase().includes(q))
      .map((e) => e.path)
      .slice(0, 8);
    showFilePicker.value = fileSearchResults.value.length > 0;
    // Position the picker near the textarea cursor (best-effort)
    if (showFilePicker.value && ta) {
      const rect = ta.getBoundingClientRect();
      filePickerAnchor.value = { top: rect.bottom + 4, left: rect.left };
    }
  } catch {
    showFilePicker.value = false;
  }
}

function selectFileFromPicker(path: string) {
  // Replace the @<query> fragment in the textarea with @path
  const ta = promptTextarea.value;
  if (!ta) { draft.files.push(path); showFilePicker.value = false; return; }
  const val = ta.value;
  const pos = ta.selectionStart;
  const before = val.slice(0, pos);
  const atIdx = before.lastIndexOf("@");
  const after = val.slice(pos);
  draft.prompt = before.slice(0, atIdx) + `@${path}` + after;
  showFilePicker.value = false;
  nextTick(() => { ta.focus(); const np = atIdx + path.length + 1; ta.setSelectionRange(np, np); });
}

// Feature 4 — hand the task off to a real Burrow terminal tab WITHOUT killing it.
// The same daemon PTY is *adopted* by a terminal tab (create_pty reattaches the live
// session — no `--resume`, no new process). The terminal tab now owns input; Mission
// Control flips `handedOff` so its follow-up bar + embedded-terminal attach lock out
// (no two writers on one PTY), but keeps its status listeners — the dot stays live,
// since `pty-hook-{id}` broadcasts to every listener. Re-handing-off just focuses the
// existing tab. The flag persists (DB), so input stays locked across an app restart.
function handoffToTab(t: Task) {
  if (!t.alive) return;
  // Drop the in-MC embedded terminal if it's attached to this task — that's another
  // input path into the same PTY, and the tab is taking over.
  if (termTaskId.value === t.id) closeTerminal();
  t.handedOff = true;
  saveTask(t);
  ui.setMode("terminal");
  const wsRow = wsStore.workspaces.find((w) => w.id === t.workspaceId) || wsStore.active;
  if (wsRow) wsStore.open(wsRow);
  // Defer until the workspace's Terminal is mounted/active, then adopt the live PTY.
  setTimeout(() => activeTerm()?.adoptPty({ ptyId: t.ptyId, cwd: t.cwd, title: t.title, sessionId: t.id }), 80);
}

// Jump back to the terminal tab that owns a handed-off task's PTY.
function focusHandoff(t: Task) {
  ui.setMode("terminal");
  const wsRow = wsStore.workspaces.find((w) => w.id === t.workspaceId) || wsStore.active;
  if (wsRow) wsStore.open(wsRow);
  setTimeout(() => activeTerm()?.adoptPty({ ptyId: t.ptyId, cwd: t.cwd, title: t.title, sessionId: t.id }), 80);
}

// Feature A — resume a dead (read-only) task in place. Spawn a fresh mission PTY
// running `claude --resume <session-id>`: the conversation reloads from the
// transcript and the task goes live again (follow-up works), without leaving
// Mission Control. Complements reconcileLive (which only re-binds PTYs the daemon
// still holds); this revives a task whose PTY is truly gone.
async function resumeTask(t: Task) {
  if (t.alive) return;
  const ptyId = allocPtyId();
  t.ptyId = ptyId;
  t.alive = true;
  t.status = "running";
  buffers.set(ptyId, "");
  await wireTask(t);
  await invoke("create_pty", { id: ptyId, cwd: t.cwd, cols: 120, rows: 34 });
  const cmd = `claude --resume ${t.id}\n`;
  setTimeout(() => {
    invoke("write_pty", { id: ptyId, data: Array.from(new TextEncoder().encode(cmd)) }).catch(() => {});
    // No turn runs on a bare resume — claude just reloads and waits at its prompt.
    setTimeout(() => { if (t.status === "running") t.status = "waiting"; }, 1500);
  }, 450);
  saveTask(t);
}

// Sequential runner: keep activeCount up to maxConcurrent.
function activeCount() {
  return tasks.value.filter((t) => t.alive && (t.status === "running" || t.status === "waiting")).length;
}
async function pumpQueue() {
  while (queue.value.length && activeCount() < maxConcurrentClamped.value) {
    const next = queue.value.shift()!;
    const cwd = await resolveCwd(next.branch, next.cwd, next.isolate);
    if (!cwd) continue;  // worktree failed — drop this item, keep draining
    spawnTask(next.prompt, cwd, next.model, next.images, next.skipPerms, next.files ?? []);
  }
}

// ── Stop / cleanup ───────────────────────────────────────────────────────────
// Interrupt the current turn (Ctrl+C) without killing the session — tank's
// "stop" button. claude returns to its prompt; follow-up still works.
function stopGeneration(t: Task) {
  if (!t.alive) return;
  invoke("write_pty", { id: t.ptyId, data: [0x03] }).catch(() => {});
}

// Kill the PTY and drop the task entirely (header "Delete").
function deleteTask(t: Task) {
  invoke("kill_pty", { id: t.ptyId }).catch(() => {});
  teardown(t.ptyId);
  buffers.delete(t.ptyId);
  tasks.value = tasks.value.filter((x) => x.id !== t.id);
  if (selectedId.value === t.id) selectedId.value = tasks.value[0]?.id ?? null;
  invoke("delete_mission_task", { id: t.id }).catch(() => {});
  pumpQueue();
}

function teardown(ptyId: number) {
  unlisteners.get(ptyId)?.forEach((u) => u());
  unlisteners.delete(ptyId);
}

function clearDead() {
  const removed = tasks.value.filter((t) => !t.alive);
  removed.forEach((t) => {
    teardown(t.ptyId);
    buffers.delete(t.ptyId);
    invoke("delete_mission_task", { id: t.id }).catch(() => {});
  });
  tasks.value = tasks.value.filter((t) => t.alive);
  if (selectedId.value && !tasks.value.some((t) => t.id === selectedId.value)) {
    selectedId.value = tasks.value[0]?.id ?? null;
  }
}

// ── Terminal modal ───────────────────────────────────────────────────────────
const termHost = ref<HTMLElement | null>(null);
const termTaskId = ref<string | null>(null);
let termInstance: Terminal | null = null;
let termInputOff: (() => void) | null = null;

const termTaskTitle = computed(() => tasks.value.find((t) => t.id === termTaskId.value)?.title ?? "");
const termTaskStatus = computed(() => tasks.value.find((t) => t.id === termTaskId.value)?.status ?? "idle");

async function openTerminal(t: Task) {
  termTaskId.value = t.id;
  await nextTick();
  const css = getComputedStyle(document.documentElement);
  const cssVar = (n: string, fb: string) => css.getPropertyValue(n).trim() || fb;
  // Render at the PTY's EXACT native geometry (every mission PTY is created at
  // 120×34 — see create_pty calls). The replay buffer and the live agent both
  // emit absolute cursor positioning (`[24G` etc.) for that 120-col grid, so a
  // FitAddon that picks a different width misaligns every redraw → garbled
  // overlap. Fixing the grid to 120×34 makes replay + live align exactly; no
  // reflow, no SIGWINCH disruption to the running agent. The dialog is sized to
  // fit this grid (term-host scrolls if the window is too small).
  termInstance = new Terminal({
    cols: 120,
    rows: 34,
    cursorBlink: true,
    fontFamily: cssVar("--font-mono", "ui-monospace, SFMono-Regular, Menlo, monospace"),
    fontSize: 12,
    theme: {
      background: cssVar("--terminal-bg", "#0a0a0a"),
      foreground: cssVar("--text-primary", "#e6edf3"),
    },
    scrollback: 5000,
  });
  termInstance.open(termHost.value!);
  termInstance.write(buffers.get(t.ptyId) || "");

  const onData = termInstance.onData((data) => {
    invoke("write_pty", { id: t.ptyId, data: Array.from(new TextEncoder().encode(data)) }).catch(() => {});
  });
  termInputOff = () => onData.dispose();

  // The raw replay is a *stack* of every past TUI frame, ending on a partial one.
  // Claude repaints its box IN PLACE (relative cursor moves), so a bare SIGWINCH
  // redraws on top of those stale rows → spinner/status cells interleave with the
  // leftovers → permanent garble (the bug). Fix: blank the VIEWPORT first (ED2 +
  // home, written locally into xterm — scrollback above stays intact), THEN nudge
  // a SIGWINCH so the agent's full repaint lands on a clean screen. The same-size
  // resize is a kernel no-op, so toggle 119→120 (back to native geometry) to
  // guarantee delivery. The PTY is owned solely by this modal (no other xterm
  // mounts it), so resizing it is safe — no geometry fight.
  if (t.alive) {
    termInstance.write("\x1b[2J\x1b[H");
    invoke("resize_pty", { id: t.ptyId, cols: 119, rows: 34 }).catch(() => {});
    setTimeout(() => invoke("resize_pty", { id: t.ptyId, cols: 120, rows: 34 }).catch(() => {}), 60);
  }
}

function sendCtrlC() {
  const t = tasks.value.find((x) => x.id === termTaskId.value);
  if (t) invoke("write_pty", { id: t.ptyId, data: [0x03] }).catch(() => {});
}

function closeTerminal() {
  termInputOff?.(); termInputOff = null;
  termInstance?.dispose(); termInstance = null;
  termTaskId.value = null;
}

// ── Persistence — shared workspaces.db (mission_tasks table), like every other
// Burrow feature. Metadata only; live PTYs aren't serializable, so a restored
// task is read-only (its daemon PTY died with the previous app session).
function saveTask(t: Task) {
  invoke("upsert_mission_task", {
    task: {
      id: t.id,
      workspace_id: t.workspaceId,
      pty_id: t.ptyId,
      title: t.title,
      cwd: t.cwd,
      model: t.model,
      status: t.status,
      turns: JSON.stringify(t.turns),
      created_at: t.createdAt,
      handed_off: t.handedOff ? 1 : 0,
    },
  }).catch(() => {});
}

async function loadTasks() {
  let rows: TaskRow[] = [];
  try { rows = await invoke<TaskRow[]>("list_mission_tasks"); } catch { return; }
  tasks.value = rows.map((r) => ({
    id: r.id,
    ptyId: r.pty_id ?? 0,
    workspaceId: r.workspace_id ?? null,
    title: r.title ?? "task",
    prompt: "",
    cwd: r.cwd ?? "",
    model: r.model ?? "default",
    status: (r.status as Status) ?? "done",
    turns: parseTurns(r.turns),
    followup: "",
    expanded: false,
    alive: false,   // restored: PTY gone → read-only
    handedOff: r.handed_off === 1,
    createdAt: r.created_at,
  }));
  // Keep new PTY ids above any restored one (they share the daemon's id map).
  for (const t of tasks.value) bumpPtySeqPast(t.ptyId);
}

// Feature 2 — live PTY reconciliation. The Burrow daemon keeps PTYs alive across
// app reloads, so a restored task whose PTY is still running can be re-attached:
// re-stream it (same as XTerm's restore path) and re-wire listeners, making live
// attach + follow-up work again. Also bumps the seq past every daemon PTY id so a
// new task never reuses a live one (the bug that typed `claude …` into a running
// claude's REPL). This is the authoritative guard — the DB max alone can miss a
// PTY whose task row was deleted but whose process lingers.
async function reconcileLive() {
  let sessions: { pty_id: number; alive: boolean }[] = [];
  try { sessions = await invoke("list_pty_sessions"); } catch { return; }
  const live = new Map(sessions.map((s) => [s.pty_id, s]));
  for (const s of sessions) bumpPtySeqPast(s.pty_id);
  for (const t of tasks.value) {
    if (t.alive || !live.get(t.ptyId)?.alive) continue;
    try {
      // Re-open the daemon stream for this existing session, then listen again.
      await invoke("create_pty", { id: t.ptyId, cwd: t.cwd, cols: 120, rows: 34 });
      buffers.set(t.ptyId, buffers.get(t.ptyId) || "");
      await wireTask(t);
      t.alive = true;
      if (t.status === "done") t.status = "waiting"; // idle at its prompt, ready for follow-up
      saveTask(t);
    } catch { /* leave it read-only */ }
  }
}

function parseTurns(s: string | null): Turn[] {
  if (!s) return [];
  try { const v = JSON.parse(s); return Array.isArray(v) ? v : []; } catch { return []; }
}

// Selecting a task jumps the conversation to its newest turn.
watch(selectedId, () => scrollConvo());

// Keep the activity bar badge in sync with active task count.
watch(
  tasks,
  (ts) => { ui.missionActiveCount = ts.filter((t) => t.alive && (t.status === "running" || t.status === "waiting")).length; },
  { deep: true },
);

onMounted(async () => {
  await loadTasks();
  await reconcileLive();   // re-attach still-running PTYs + never reuse a live id
  // Composer cwd defaults to the active workspace (the scope chosen in the sidebar).
  draft.cwd = activeWs.value?.path || tasks.value[0]?.cwd || "";
  selectedId.value = orderedTasks.value[0]?.id ?? null;
  if (!tasks.value.length) composerOpen.value = true;  // empty → invite a new task
});

onBeforeUnmount(() => {
  closeTerminal();
  for (const offs of unlisteners.values()) offs.forEach((u) => u());
});
</script>

<style scoped>
.mc {
  display: grid;
  grid-template-columns: 300px 1fr;
  height: 100%;
  width: 100%;
  min-height: 0;
  background: var(--bg-base);
  color: var(--text-primary);
  font-family: var(--font-ui);
  font-size: 14px;
}

/* ── Rail ── */
.rail {
  border-right: 1px solid var(--border);
  background: var(--bg-panel);
  backdrop-filter: var(--blur-content, none);
  -webkit-backdrop-filter: var(--blur-content, none);
  display: flex;
  flex-direction: column;
  overflow-y: auto;
}
.rail-head { padding: 16px 16px 10px; display: flex; align-items: center; gap: 9px; }
.rail-head .brand-mark { color: var(--accent); flex-shrink: 0; }
.rail-head h1 { font-size: 14px; margin: 0; font-weight: 650; letter-spacing: 0.01em; flex: 1; }

/* ── Workspace scope: read-only mirror of the sidebar's active workspace ── */
.ws-scope {
  margin: 0 12px 10px; display: flex; align-items: center; gap: 10px;
  background: var(--terminal-bg); border: 1px solid var(--border); border-radius: 10px; padding: 9px 11px;
}
.ws-ico { flex-shrink: 0; color: var(--text-secondary); }
.ws-ico-img { width: 20px; height: 20px; border-radius: 5px; object-fit: cover; flex-shrink: 0; }
.ws-meta { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
.ws-name { font-size: 13px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.ws-path { font-size: 10px; color: var(--text-muted); font-family: var(--font-mono); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

.new-btn { margin: 0 12px 12px; display: flex; align-items: center; justify-content: center; gap: 7px; padding: 9px 12px; font-size: 13px; }

/* ── Blank state: no active workspace ── */
.mc.blank { display: flex; align-items: center; justify-content: center; }
.mc-blank { display: flex; align-items: center; justify-content: center; width: 100%; padding: 40px; }
.blank-inner { max-width: 380px; text-align: center; display: flex; flex-direction: column; align-items: center; gap: 10px; }
.blank-mark { color: var(--accent); opacity: 0.9; margin-bottom: 4px; }
.blank-inner h2 { font-size: 18px; font-weight: 650; margin: 0; color: var(--text-primary); }
.blank-inner p { font-size: 13px; line-height: 1.6; color: var(--text-secondary); margin: 0; }
.blank-hint { display: inline-flex; align-items: center; gap: 6px; margin-top: 6px; font-size: 11px; color: var(--accent); font-family: var(--font-mono); }

.rail-summary { display: flex; align-items: center; gap: 6px; padding: 0 14px 12px; font-size: 12px; color: var(--text-secondary); border-bottom: 1px solid var(--border); }
.chip { display: inline-flex; align-items: center; gap: 5px; padding: 3px 8px; border-radius: 999px; background: var(--terminal-bg); border: 1px solid var(--border); font-size: 11px; font-variant-numeric: tabular-nums; opacity: 0.55; }
.chip.on { opacity: 1; }
.chip small { color: var(--text-muted); font-size: 10px; }
.btn.xs { padding: 2px 6px; font-size: 10px; }
.rail-empty { padding: 30px 16px; text-align: center; font-size: 12px; color: var(--text-muted); line-height: 1.7; }

/* ── Task list (projects → tasks) ── */
.tasklist { flex: 1; overflow-y: auto; padding: 8px 0; }
.proj { margin-bottom: 6px; }
.proj-head { font-size: 10px; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-muted); padding: 6px 14px 4px; font-family: var(--font-mono); }
.proj ul { list-style: none; margin: 0; padding: 0; }
.task-row { display: flex; align-items: center; gap: 8px; padding: 8px 14px; cursor: pointer; border-left: 2px solid transparent; }
.task-row:hover { background: var(--bg-hover); }
.task-row.active { background: var(--bg-selected); border-left-color: var(--accent); }
.row-title { flex: 1; font-size: 13px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.row-meta { font-size: 10px; color: var(--text-muted); white-space: nowrap; }

.cwd-line { margin: -4px 0 0; font-size: 11.5px; color: var(--text-secondary); display: flex; align-items: center; gap: 6px; white-space: nowrap; overflow: hidden; }
.cwd-line .dir-ico { color: var(--accent); flex-shrink: 0; }
.cwd-line .cwd-path { color: var(--text-muted); font-family: var(--font-mono); font-size: 10.5px; overflow: hidden; text-overflow: ellipsis; }
.warn-ico { color: var(--yellow); vertical-align: -2px; }
.fld { display: flex; flex-direction: column; gap: 4px; }
.fld > span { font-size: 11px; color: var(--text-secondary); text-transform: uppercase; letter-spacing: 0.04em; }
.fld textarea, .fld input, .fld select {
  background: var(--terminal-bg); border: 1px solid var(--border); border-radius: 8px;
  color: var(--text-primary); padding: 8px 10px; font-size: 13px; font-family: inherit; resize: vertical;
}
.fld textarea { font-family: var(--font-mono); }
.fld input:focus, .fld textarea:focus, .fld select:focus { outline: none; border-color: var(--accent); }

/* ── Custom dropdown (matches the app, native <select> can't be themed) ── */
.dd { position: relative; }
.dd-btn {
  width: 100%; display: flex; align-items: center; gap: 8px;
  background: var(--terminal-bg); border: 1px solid var(--border); border-radius: 8px;
  color: var(--text-primary); padding: 8px 10px; font-size: 13px; font-family: inherit; cursor: pointer;
  transition: border-color 0.16s cubic-bezier(0.22,1,0.36,1);
}
.dd-btn:hover { border-color: color-mix(in srgb, var(--accent) 35%, var(--border)); }
.dd-btn.open { border-color: var(--accent); }
.dd-val { flex: 1; text-align: left; }
.dd-chev { color: var(--text-muted); transition: transform 0.2s cubic-bezier(0.22,1,0.36,1); }
.dd-chev.open { transform: rotate(180deg); }
.dd-backdrop { position: fixed; inset: 0; z-index: 100; }
.dd-menu {
  position: absolute; z-index: 101; left: 0; right: 0; top: calc(100% + 5px);
  list-style: none; margin: 0; padding: 5px; max-height: 260px; overflow-y: auto;
  background: var(--bg-panel); border: 1px solid var(--border); border-radius: 9px;
  box-shadow: 0 14px 34px -10px #000d;
}
.dd-opt {
  display: flex; align-items: center; justify-content: space-between; gap: 8px;
  padding: 8px 9px; border-radius: 6px; font-size: 13px; color: var(--text-secondary); cursor: pointer;
}
.dd-opt:hover { background: var(--bg-hover); color: var(--text-primary); }
.dd-opt.sel { color: var(--accent); }
.dd-opt.sel svg { color: var(--accent); }

.composer-actions { display: flex; gap: 8px; }
.conc { display: flex; align-items: center; gap: 8px; font-size: 11px; color: var(--text-secondary); }
.conc input { width: 52px; background: var(--terminal-bg); border: 1px solid var(--border); border-radius: 6px; color: var(--text-primary); padding: 4px 6px; }

.btn {
  display: inline-flex; align-items: center; justify-content: center; gap: 6px;
  background: var(--bg-hover); border: 1px solid var(--border); color: var(--text-primary);
  border-radius: 8px; padding: 8px 12px; font-size: 13px; cursor: pointer; font-family: inherit;
}
.btn svg { flex-shrink: 0; }
.btn:hover { background: color-mix(in srgb, var(--text-primary) 8%, var(--bg-hover)); }
.btn:disabled { opacity: 0.4; cursor: not-allowed; }
.btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; font-weight: 600; }
.btn.primary:hover { background: var(--accent-dim); }
.btn.ghost { background: transparent; }
.btn.tiny { padding: 4px 8px; font-size: 11px; }
.btn.danger { color: var(--red); border-color: color-mix(in srgb, var(--red) 30%, var(--border)); }

.queue { padding: 8px 16px 16px; }
.queue h2 { font-size: 11px; color: var(--text-secondary); text-transform: uppercase; letter-spacing: 0.04em; }
.queue ul { list-style: none; margin: 8px 0 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
.queue li { display: flex; align-items: center; gap: 8px; background: var(--terminal-bg); border: 1px solid var(--border); border-radius: 8px; padding: 6px 8px; }
.qnum { font-size: 10px; color: var(--text-muted); width: 14px; }
.qtext { flex: 1; font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.queue .x { background: none; border: none; color: var(--text-muted); cursor: pointer; }

/* ── Detail (selected task) ── */
.detail { display: flex; flex-direction: column; height: 100%; min-height: 0; overflow: hidden; }
.detail-head { display: flex; align-items: center; gap: 10px; padding: 14px 20px; border-bottom: 1px solid var(--border); }
.detail-head h2 { margin: 0; font-size: 15px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 40vw; }
.detail-head .status-text { font-size: 12px; color: var(--text-muted); }
.handoff-badge { display: inline-flex; align-items: center; gap: 4px; font-size: 10px; font-weight: 600; color: var(--accent); background: color-mix(in srgb, var(--accent) 14%, transparent); border-radius: 5px; padding: 2px 7px; }
.row-handoff { color: var(--accent); flex-shrink: 0; }
.continue-bar.handed { flex-direction: row; align-items: center; gap: 8px; color: var(--accent); font-size: 12px; }
.cwd-hint { font-size: 11px; color: var(--text-muted); font-family: var(--font-mono); }
.model { font-size: 10px; background: color-mix(in srgb, var(--accent) 18%, transparent); border-radius: 5px; padding: 2px 6px; color: var(--accent); }
.spacer { flex: 1; }

.convo-full { flex: 1; overflow-y: auto; padding: 20px; display: flex; flex-direction: column; gap: 16px; }
.turn { display: flex; gap: 10px; font-size: 13px; line-height: 1.55; }
.turn .role { flex-shrink: 0; width: 18px; display: flex; justify-content: center; padding-top: 2px; color: var(--text-muted); }
.turn .ttext { white-space: pre-wrap; }
.turn.user .ttext { color: var(--text-primary); font-family: var(--font-mono); background: var(--bg-hover); border: 1px solid var(--border); border-radius: 8px; padding: 8px 12px; flex: 1; }
.turn.assistant .ttext { color: var(--text-secondary); background: var(--terminal-bg); border: 1px solid var(--border); border-radius: 8px; padding: 10px 12px; flex: 1; }
.working { color: var(--yellow); font-size: 12px; padding-left: 28px; display: flex; align-items: center; gap: 6px; animation: pulse 1.4s infinite; }
.no-result { color: var(--text-muted); font-size: 12px; font-style: italic; padding-left: 28px; }

/* ── Continue bar (send & continue) ── */
.continue-bar { border-top: 1px solid var(--border); padding: 12px 20px 16px; display: flex; flex-direction: column; gap: 8px; background: var(--bg-panel); }
.continue-bar textarea {
  background: var(--terminal-bg); border: 1px solid var(--border); border-radius: 10px;
  color: var(--text-primary); padding: 10px 12px; font-size: 13px; font-family: var(--font-mono);
  resize: vertical; min-height: 48px; max-height: 200px;
}
.continue-bar textarea:focus { outline: none; border-color: var(--accent); }
.continue-bar textarea:disabled { opacity: 0.5; }
.cb-actions { display: flex; justify-content: flex-end; gap: 8px; }
.continue-bar.dead { color: var(--text-muted); font-size: 12px; flex-direction: row; align-items: center; }

.empty-detail { display: flex; align-items: center; justify-content: center; height: 100%; }
.empty-inner { text-align: center; color: var(--text-muted); }
.empty-inner h2 { font-size: 16px; margin-bottom: 6px; color: var(--text-secondary); }
.empty-inner p { font-size: 13px; }

/* ── Composer modal ── */
.composer-modal { position: fixed; inset: 0; background: #000b; display: flex; align-items: center; justify-content: center; z-index: 90; }
.composer-box { width: 520px; max-width: 92vw; background: var(--bg-panel); border: 1px solid var(--border); border-radius: 14px; padding: 18px; display: flex; flex-direction: column; gap: 12px; backdrop-filter: var(--blur-overlay, none); -webkit-backdrop-filter: var(--blur-overlay, none); }
.cm-head { display: flex; align-items: center; font-size: 14px; font-weight: 600; }
.cm-head span { flex: 1; }
.composer-actions { display: flex; gap: 8px; align-items: center; }

/* image attachments + worktree toggle */
.img-chips { display: flex; flex-wrap: wrap; gap: 6px; }
.img-chip { display: inline-flex; align-items: center; gap: 6px; font-size: 11px; background: var(--bg-hover); border: 1px solid var(--border); border-radius: 6px; padding: 3px 8px; color: var(--text-secondary); }
.img-chip .x { background: none; border: none; color: var(--text-muted); cursor: pointer; padding: 0; }
.iso { display: flex; align-items: flex-start; gap: 8px; font-size: 12px; color: var(--text-secondary); cursor: pointer; }
.iso em { color: var(--text-muted); font-style: normal; }
.iso code { font-size: 11px; }
.danger-toggle em { color: color-mix(in srgb, var(--red) 70%, var(--text-muted)); }
.wt-err { font-size: 12px; color: var(--red); }
.branch-row { display: flex; align-items: center; gap: 6px; margin-left: 24px; color: var(--text-muted); font-size: 12px; }
.branch-prefix { font-family: var(--font-mono); opacity: 0.7; }
.branch-input { flex: 1; min-width: 0; background: var(--terminal-bg); border: 1px solid var(--border); border-radius: 6px; color: var(--text-primary); font-family: var(--font-mono); font-size: 12px; padding: 4px 6px; }
.branch-input:focus { outline: none; border-color: color-mix(in srgb, var(--accent, #6ab) 60%, var(--border)); }

/* ── Status dots ── */
.dot { width: 8px; height: 8px; border-radius: 50%; display: inline-block; background: var(--text-muted); flex-shrink: 0; }
.dot.running { background: var(--yellow); box-shadow: 0 0 8px color-mix(in srgb, var(--yellow) 53%, transparent); animation: pulse 1.4s infinite; }
.dot.waiting { background: var(--accent); box-shadow: 0 0 8px color-mix(in srgb, var(--accent) 53%, transparent); }
.dot.done { background: var(--green); box-shadow: 0 0 8px color-mix(in srgb, var(--green) 53%, transparent); }
.dot.error { background: var(--red); }
@keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.4; } }

/* ── Terminal modal ── */
.term-modal { position: fixed; inset: 0; background: #000a; display: flex; align-items: center; justify-content: center; z-index: 100; }
.term-box { width: auto; max-width: 94vw; max-height: 90vh; background: var(--terminal-bg); border: 1px solid var(--border); border-radius: 12px; display: flex; flex-direction: column; overflow: hidden; backdrop-filter: var(--blur-overlay, none); -webkit-backdrop-filter: var(--blur-overlay, none); }
.term-head { display: flex; align-items: center; gap: 8px; padding: 8px 12px; background: var(--bg-panel); border-bottom: 1px solid var(--border); font-size: 13px; }
.term-host { flex: 1; padding: 8px; overflow: auto; }

/* ── Composer improvements ── */
.cm-head { display: flex; align-items: center; font-size: 14px; font-weight: 600; gap: 8px; }
.cm-brand { color: var(--accent); flex-shrink: 0; }
.icon-btn { padding: 4px 6px; }

.prompt-fld { position: relative; }
.prompt-label-row { display: flex; align-items: baseline; justify-content: space-between; margin-bottom: 4px; }
.prompt-hint { font-size: 10px; color: var(--text-muted); }
.prompt-hint kbd { font-family: var(--font-mono); background: var(--bg-hover); border: 1px solid var(--border); border-radius: 3px; padding: 1px 4px; font-size: 10px; color: var(--text-secondary); }
.prompt-wrap { position: relative; }
.prompt-wrap textarea { width: 100%; box-sizing: border-box; }

/* @-trigger inline file picker */
.at-picker {
  position: absolute; top: calc(100% + 4px); left: 0; right: 0; z-index: 200;
  background: var(--bg-panel); border: 1px solid var(--border); border-radius: 9px;
  box-shadow: 0 12px 32px -8px #000c; overflow: hidden;
}
.at-header { display: flex; align-items: center; gap: 6px; padding: 6px 10px; font-size: 10px; color: var(--text-muted); border-bottom: 1px solid var(--border); font-family: var(--font-mono); }
.at-picker ul { list-style: none; margin: 0; padding: 4px; }
.at-opt { display: flex; align-items: center; gap: 8px; padding: 7px 9px; border-radius: 6px; cursor: pointer; font-size: 12px; }
.at-opt:hover { background: var(--bg-hover); }
.at-name { font-weight: 500; color: var(--text-primary); white-space: nowrap; }
.at-path { font-family: var(--font-mono); font-size: 10px; color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

/* file chip variant */
.file-chip { color: color-mix(in srgb, var(--accent) 90%, var(--text-secondary)); border-color: color-mix(in srgb, var(--accent) 25%, var(--border)); }
.file-chip svg { color: var(--accent); }

/* attach row */
.attach-row { display: flex; align-items: center; gap: 10px; }
.attach-btn { display: flex; align-items: center; gap: 5px; }
.attach-hint { font-size: 10.5px; color: var(--text-muted); }
.attach-hint kbd { font-family: var(--font-mono); background: var(--bg-hover); border: 1px solid var(--border); border-radius: 3px; padding: 1px 4px; font-size: 10px; color: var(--text-secondary); }
</style>
