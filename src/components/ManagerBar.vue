<template>
  <div class="mb-root" :class="{ 'mb-open': expanded }">
    <!-- Drag handle (top border) — resize the expanded panel height -->
    <div
      v-show="expanded"
      class="mb-resize"
      @mousedown="startResize"
    />

    <!-- Expanded panel: animated height wrapper keeps the chat mounted while it
         slides open/closed. Inner panel is fixed-height so content doesn't
         squish mid-animation. Only PAST MESSAGES live here — the composer is in
         the strip below. -->
    <div
      v-if="started"
      class="mb-panel-wrap"
      :style="{ height: (expanded ? panelHeight : 0) + 'px' }"
    >
      <div class="mb-panel" :style="{ height: panelHeight + 'px' }">
        <div class="mb-chat">
          <ClaudeChat
            v-if="controlChatId !== null"
            ref="chatRef"
            :key="controlChatId"
            compact
            hide-composer
            :chat-id="controlChatId"
            :workspace-id="rootId"
            :cwd="rootCwd"
            :append-system-prompt="managerPrimer"
          />
        </div>
      </div>
    </div>

    <!-- Always-visible bottom strip — holds the one Manager composer -->
    <div class="mb-strip">
      <PhSparkle :size="15" weight="fill" class="mb-strip-icon" />
      <span class="mb-strip-title">Manager</span>
      <span class="mb-status-dot" :class="`mb-dot-${dotKind}`" :title="dotTitle" />
      <span class="mb-strip-sub" :title="rootCwd">{{ rootName }}</span>

      <!-- Quick single-line input straight into the Manager (always present) -->
      <input
        ref="quickEl"
        v-model="quickText"
        class="mb-quick"
        type="text"
        :placeholder="busy ? 'Manager is working — queue a message…' : 'Message Manager — orchestrate worktrees, agents & PRs'"
        @focus="ensureStarted"
        @keydown.enter.prevent="quickSend"
      />

      <!-- Spawn-target picker: clear labeled dropdown (replaces the cryptic
           icon toggle). Tells you where the Manager puts new agents. -->
      <div class="mb-wt">
        <button
          class="mb-wt-btn"
          :title="'Where the Manager spawns new agents'"
          @click="wtMenuOpen = !wtMenuOpen"
        >
          <PhTree v-if="worktreeMode" :size="13" weight="bold" />
          <PhGitBranch v-else :size="13" weight="bold" />
          <span class="mb-wt-label">{{ worktreeMode ? 'New worktree' : 'Current branch' }}</span>
          <PhCaretUp :size="9" weight="bold" class="mb-wt-caret" />
        </button>
        <div v-if="wtMenuOpen" class="mb-wt-menu">
          <div class="mb-wt-menu-head">Spawn agents in…</div>
          <button
            class="mb-wt-item"
            :class="{ 'mb-wt-item-on': !worktreeMode }"
            @click="selectWorktreeMode(false)"
          >
            <PhGitBranch :size="14" weight="bold" />
            <div class="mb-wt-item-text">
              <span class="mb-wt-item-title">Current branch</span>
              <span class="mb-wt-item-sub">Shared working tree — fast, agents see each other's edits</span>
            </div>
            <PhCheck v-if="!worktreeMode" :size="13" weight="bold" class="mb-wt-item-check" />
          </button>
          <button
            class="mb-wt-item"
            :class="{ 'mb-wt-item-on': worktreeMode }"
            @click="selectWorktreeMode(true)"
          >
            <PhTree :size="14" weight="bold" />
            <div class="mb-wt-item-text">
              <span class="mb-wt-item-title">New worktree each</span>
              <span class="mb-wt-item-sub">Isolated branch per agent — safe for parallel work</span>
            </div>
            <PhCheck v-if="worktreeMode" :size="13" weight="bold" class="mb-wt-item-check" />
          </button>
        </div>
      </div>
      <button
        class="mb-btn"
        :title="expanded ? 'Collapse Manager (⌘J)' : 'Expand Manager (⌘J)'"
        @click="toggleExpanded"
      >
        <PhCaretDown v-if="expanded" :size="15" weight="bold" />
        <PhCaretUp v-else :size="15" weight="bold" />
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount, nextTick } from "vue";
import { PhSparkle, PhGitBranch, PhTree, PhCaretDown, PhCaretUp, PhCheck } from "@phosphor-icons/vue";
import ClaudeChat from "./ClaudeChat.vue";
import { useUIStore } from "@/stores/ui";
import { useClaudeChatsStore } from "@/stores/claudeChats";
import { useWorkspaceStore } from "@/stores/workspace";

const props = defineProps<{ cwd: string; wsId: number }>();

const ui = useUIStore();
const chats = useClaudeChatsStore();
const wsStore = useWorkspaceStore();

const chatRef = ref<InstanceType<typeof ClaudeChat> | null>(null);
const quickEl = ref<HTMLInputElement | null>(null);
const quickText = ref("");

// Expanded state is shared with the existing ui pref (floatChatOpen) so ⌘J and the
// persisted preference keep working unchanged. `started` gates the first claude
// spawn: we don't launch a Manager process until the user first opens or types.
const expanded = computed(() => ui.floatChatOpen);
const started = ref(false);

function ensureStarted() {
  if (!started.value) started.value = true;
  if (typeof rootId.value === "number") ensureControlSession(rootId.value);
}
function toggleExpanded() {
  ui.toggleFloatChat();
}

// ── Active workspace anchoring (same model as the old FloatChat) ──
// Only re-anchor while the Manager is idle, so switching workspaces mid-turn
// doesn't kill the running claude process.
const activeWsId = ref<number>(props.wsId);
const activeCwd = ref<string>(props.cwd);

// The Manager is anchored to the ROOT repo, not a worktree. Worktrees are their
// own workspace rows (parent_id set); keying by root keeps one session alive
// across worktree switches instead of an empty one per worktree.
const root = computed(() => {
  const w = wsStore.workspaces.find((x) => x.id === activeWsId.value);
  if (w?.parent_id) {
    const parent = wsStore.workspaces.find((x) => x.id === w.parent_id);
    if (parent) return parent;
  }
  return w ?? null;
});
const rootId = computed(() => root.value?.id ?? activeWsId.value);
const rootCwd = computed(() => root.value?.path ?? activeCwd.value);
const rootName = computed(() => root.value?.name ?? "this repo");

// One persistent Manager session per ROOT repo, reused across open/collapse,
// worktree switches, and app restarts. Keyed by root repo id in localStorage.
const MAP_KEY = "burrow.floatchat.sessions";
function loadMap(): Record<number, number> {
  try { return JSON.parse(localStorage.getItem(MAP_KEY) || "{}"); } catch { return {}; }
}
function saveMap(m: Record<number, number>) {
  localStorage.setItem(MAP_KEY, JSON.stringify(m));
}

const controlChatId = ref<number | null>(null);

// Worktree spawn preference (persisted globally) — reflected in the primer.
const WT_KEY = "burrow.floatchat.worktreeMode";
const worktreeMode = ref<boolean>(localStorage.getItem(WT_KEY) === "1");
watch(worktreeMode, (v) => localStorage.setItem(WT_KEY, v ? "1" : "0"));
const wtMenuOpen = ref(false);
function selectWorktreeMode(v: boolean) {
  worktreeMode.value = v;
  wtMenuOpen.value = false;
}

// Adopt the active workspace only when the Manager is idle.
watch(
  () => [props.wsId, props.cwd] as const,
  ([wsId, cwd]) => {
    const isBusy = controlChatId.value
      ? chats.sessions.find((s) => s.id === controlChatId.value)?.busy
      : false;
    if (!isBusy) {
      activeWsId.value = wsId;
      activeCwd.value = cwd;
    }
  },
);

function ensureControlSession(repoId: number) {
  const map = loadMap();
  const existing = map[repoId];
  if (existing && chats.sessions.find((s) => s.id === existing)) {
    controlChatId.value = existing;
    return;
  }
  // create() flips the workspace's active chat; restore it so the in-tab Claude
  // pane isn't yanked to this hidden Manager session.
  const prevActive = chats.activeByWs[repoId];
  const sess = chats.create(repoId);
  chats.sync(sess.id, { title: "Manager", control: true });
  if (prevActive) chats.setActive(repoId, prevActive);
  map[repoId] = sess.id;
  saveMap(map);
  controlChatId.value = sess.id;
}

// Start lazily the first time the bar is expanded (or input focused).
watch(
  () => [expanded.value, rootId.value] as const,
  ([isOpen, repoId]) => {
    if (isOpen && typeof repoId === "number") {
      started.value = true;
      ensureControlSession(repoId);
    } else if (started.value && typeof repoId === "number") {
      // Already running: keep the session pointer current as we switch repos.
      ensureControlSession(repoId);
    }
  },
  { immediate: true },
);

// The live Manager session row (status/busy mirror the in-tab chat model).
const session = computed(() =>
  controlChatId.value === null
    ? null
    : chats.sessions.find((s) => s.id === controlChatId.value) ?? null,
);
const busy = computed(() => !!session.value?.busy);

// Latch a turn that finished while collapsed so the strip dot flags "done".
const finishedWhileCollapsed = ref(false);
watch(
  () => session.value?.busy,
  (now, prev) => {
    if (prev && !now && !expanded.value) finishedWhileCollapsed.value = true;
    if (prev && !now) {
      // Adopt a workspace switch that was deferred while a task ran.
      activeWsId.value = props.wsId;
      activeCwd.value = props.cwd;
    }
  },
);
watch(expanded, (o) => { if (o) finishedWhileCollapsed.value = false; });

// Strip status dot: permission > waiting > busy > done > idle.
const dotKind = computed<"permission" | "waiting" | "busy" | "done" | "idle">(() => {
  const st = session.value?.status;
  if (st === "permission") return "permission";
  if (st === "waiting") return "waiting";
  if (busy.value) return "busy";
  if (finishedWhileCollapsed.value) return "done";
  return "idle";
});
const dotTitle = computed(() => {
  switch (dotKind.value) {
    case "permission": return "Manager needs a permission decision";
    case "waiting": return "Manager is waiting for your input";
    case "busy": return "Manager is working";
    case "done": return "Manager finished while you were away";
    default: return "Manager — idle";
  }
});

async function quickSend() {
  const text = quickText.value.trim();
  if (!text) return;
  quickText.value = "";
  ensureStarted();
  if (!expanded.value) ui.toggleFloatChat();
  await nextTick();
  chatRef.value?.sendMessage(text);
}

// ── Resizable expanded panel height ──
const HEIGHT_KEY = "burrow.manager.height";
const panelHeight = ref<number>(
  Math.min(Math.max(parseInt(localStorage.getItem(HEIGHT_KEY) ?? "360", 10) || 360, 160), 900),
);
let resizing = false;
let startY = 0;
let startH = 0;
function startResize(e: MouseEvent) {
  resizing = true;
  startY = e.clientY;
  startH = panelHeight.value;
  e.preventDefault();
}
function onResizeMove(e: MouseEvent) {
  if (!resizing) return;
  const max = Math.round(window.innerHeight * 0.8);
  panelHeight.value = Math.min(Math.max(startH - (e.clientY - startY), 160), max);
}
function onResizeUp() {
  if (!resizing) return;
  resizing = false;
  localStorage.setItem(HEIGHT_KEY, String(panelHeight.value));
}

// Publish the always-visible strip height so the pet overlay walks ON TOP of
// the Manager row instead of behind it.
const STRIP_H = 38;
function onDocMouseDown(e: MouseEvent) {
  if (wtMenuOpen.value && !(e.target as HTMLElement)?.closest(".mb-wt")) {
    wtMenuOpen.value = false;
  }
}
onMounted(() => {
  window.addEventListener("mousemove", onResizeMove);
  window.addEventListener("mouseup", onResizeUp);
  window.addEventListener("mousedown", onDocMouseDown);
  document.documentElement.style.setProperty("--manager-bar-h", `${STRIP_H}px`);
  if (expanded.value && typeof rootId.value === "number") {
    started.value = true;
    ensureControlSession(rootId.value);
  }
});
onBeforeUnmount(() => {
  window.removeEventListener("mousemove", onResizeMove);
  window.removeEventListener("mouseup", onResizeUp);
  window.removeEventListener("mousedown", onDocMouseDown);
  document.documentElement.style.setProperty("--manager-bar-h", "0px");
});

const SPAWN_MODE_WORKTREE = `Spawn mode: **worktree per agent** (the user enabled isolation). For each task, FIRST create a dedicated worktree, THEN spawn the agent with its \`--cwd\` set to that worktree path, so parallel agents never collide on the same working tree:
\`\`\`sh
burrow worktree feat/the-task          # prints the new worktree path
burrow spawn --token t1 --cwd /path/to/repo/worktrees/feat/the-task claude "FULL TASK PROMPT HERE"
burrow wait t1
\`\`\``;

const SPAWN_MODE_BRANCH = `Spawn mode: **active branch** (default — no worktree). Spawn agents directly in the repo's current working dir; do NOT create a worktree unless the user explicitly asks. Use \`--cwd <repoPath>\` (or omit \`--cwd\` to inherit it):
\`\`\`sh
burrow spawn --token t1 --cwd <repoPath> claude "FULL TASK PROMPT HERE"
burrow wait t1
\`\`\`
If the user explicitly wants isolation for a particular task, you may still create a one-off worktree for it — but never by default.`;

const managerPrimer = computed(() => `You are Burrow's **Manager** — a persistent per-repo orchestrator. Burrow is a desktop IDE that runs AI coding agents in terminal tabs across multiple workspaces. You stay anchored to one repository and coordinate its worktrees, agents, and pull requests on the user's behalf.

You drive the app and git/GitHub by running the \`burrow\` CLI via your Bash tool. Whenever the user asks you to act — create a worktree, spawn an agent, open or switch something, manage a PR — run the matching command instead of just describing it.

## Spawning agents — CRITICAL SYNTAX
\`burrow spawn [--token T] [--cwd DIR] <command...>\` launches an agent in a new Burrow tab, running **interactively**.

To give the spawned agent a task, pass the prompt as a **single quoted positional argument** to \`claude\`:
\`\`\`sh
burrow spawn --cwd <dir> claude "Investigate the foo cache bug and propose a fix. Do NOT change code."
\`\`\`
- NEVER use \`--prompt\`, \`-p\`, or \`--print\` — \`claude\` has no \`--prompt\` flag (it errors \`unknown option '--prompt'\`), and \`-p\`/\`--print\` run non-interactively (forbidden here).
- The whole task goes in ONE pair of double quotes right after \`claude\`. Escape any inner double quotes, or use single quotes around the task and double quotes inside.
- Bare \`burrow spawn --cwd <dir> claude\` (no prompt) just opens an idle interactive agent the user can talk to.

${worktreeMode.value ? SPAWN_MODE_WORKTREE : SPAWN_MODE_BRANCH}

## App / navigation
- \`burrow list-workspaces\` — list every workspace (id, name, path).
- \`burrow list-tabs [--ws ID]\` — list a workspace's tabs (pty-id, title).
- \`burrow new-tab [--ws ID] [--cmd CMD]\` — open a new terminal tab (optionally run CMD).
- \`burrow focus-workspace <ID>\` / \`burrow focus-tab <ID>\` — switch the UI.

## Orchestration
- \`burrow worktree <branch> [--base-ref REF]\` — create a git worktree (nested under the repo). Returns the new worktree path.
- \`burrow wait <token> [--timeout S]\` — block until the spawned agent with that token finishes; prints its result. Default timeout is 300 s.
- \`burrow worktree-remove <branch|path> [--force]\` — delete a worktree (git worktree + its Burrow row). **Always ask the user to confirm before removing a worktree**, and only after the work on it is merged or no longer needed.

## Pull requests (via the \`gh\` CLI under the hood)
- \`burrow pr-create --title T --body B [--base main]\` — open a PR for the current branch.
- \`burrow pr-list [--state open|closed|all]\` — list PRs.
- \`burrow pr-view <number>\` — show a PR's details.
- \`burrow pr-merge <number> [--squash]\` — merge a PR.

Be concise. Confirm what you did. If a request is ambiguous (which worktree? which agent? which PR?), run the relevant \`list\` command first to ground yourself, then act. Destructive actions (worktree-remove, pr-merge) require explicit user confirmation first.`);
</script>

<style scoped>
.mb-root {
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  border-top: 1px solid var(--border, rgba(255, 255, 255, 0.1));
  /* Opaque backing even under translucent themes. */
  background-color: var(--bg-base, #0d0d0d);
  background-image: linear-gradient(var(--bg-panel, #111111), var(--bg-panel, #111111));
  position: relative;
  z-index: 30;
}

/* ── Resize handle ── */
.mb-resize {
  height: 5px;
  margin-top: -3px;
  cursor: row-resize;
  flex-shrink: 0;
}
.mb-resize:hover { background: var(--accent, #3b82f6); opacity: 0.4; }

/* ── Expanded panel ── */
.mb-panel-wrap {
  flex-shrink: 0;
  overflow: hidden;
  transition: height 0.22s cubic-bezier(0.4, 0, 0.2, 1);
}
.mb-panel {
  display: flex;
  flex-direction: column;
  min-height: 0;
  border-bottom: 1px solid var(--border, rgba(255, 255, 255, 0.08));
}
.mb-chat {
  flex: 1;
  min-height: 0;
  background-color: var(--bg-base, #0d0d0d);
}
.mb-chat :deep(.claude-chat) { background: transparent; backdrop-filter: none; }

/* ── Bottom strip ── */
.mb-strip {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 38px;
  padding: 0 10px;
  flex-shrink: 0;
}
.mb-strip-icon { color: var(--accent, #3b82f6); flex-shrink: 0; }
.mb-strip-title { font-size: 12px; font-weight: 600; color: var(--text-primary, #e2e8f0); flex-shrink: 0; }
.mb-strip-sub {
  font-size: 10px;
  color: var(--text-muted, #64748b);
  flex-shrink: 0;
  max-width: 140px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.mb-spacer { flex: 1; }
.mb-quick {
  flex: 1;
  min-width: 0;
  height: 26px;
  border: 1px solid var(--border, rgba(255, 255, 255, 0.12));
  border-radius: 7px;
  background: var(--bg-base, #0d0d0d);
  color: var(--text-primary, #e2e8f0);
  font-family: var(--font-ui);
  font-size: 12px;
  padding: 0 10px;
  outline: none;
}
.mb-quick::placeholder { color: var(--text-muted, #64748b); }
.mb-quick:focus { border-color: var(--accent, #3b82f6); }

/* ── Status dot ── */
.mb-status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}
.mb-dot-idle { background: var(--text-muted, #475569); }
.mb-dot-busy { background: #4ade80; animation: mb-pulse 1.4s ease-in-out infinite; }
.mb-dot-waiting { background: #3b82f6; }
.mb-dot-permission { background: #f59e0b; animation: mb-pulse 1.2s ease-in-out infinite; }
.mb-dot-done { background: #22c55e; }
@keyframes mb-pulse {
  0%, 100% { box-shadow: 0 0 0 0 rgba(74, 222, 128, 0); }
  50% { box-shadow: 0 0 0 4px rgba(74, 222, 128, 0.28); }
}

/* ── Buttons ── */
.mb-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--text-muted, #94a3b8);
  cursor: pointer;
  flex-shrink: 0;
}
.mb-btn:hover { background: var(--bg-hover, rgba(255, 255, 255, 0.08)); color: var(--text-primary, #e2e8f0); }

/* ── Spawn-target picker ── */
.mb-wt { position: relative; flex-shrink: 0; }
.mb-wt-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 26px;
  padding: 0 8px;
  border: 1px solid var(--border, rgba(255, 255, 255, 0.12));
  border-radius: 7px;
  background: transparent;
  color: var(--text-secondary, #94a3b8);
  font-family: var(--font-ui);
  font-size: 11px;
  cursor: pointer;
  white-space: nowrap;
}
.mb-wt-btn:hover { background: var(--bg-hover, rgba(255, 255, 255, 0.08)); color: var(--text-primary, #e2e8f0); }
.mb-wt-label { font-weight: 500; }
.mb-wt-caret { opacity: 0.6; }

.mb-wt-menu {
  position: absolute;
  right: 0;
  bottom: calc(100% + 6px);
  width: 260px;
  padding: 6px;
  background-color: var(--bg-dropdown, var(--bg-panel, #111));
  border: 1px solid var(--border, rgba(255, 255, 255, 0.14));
  border-radius: 10px;
  box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5);
  z-index: 70;
}
.mb-wt-menu-head {
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.07em;
  color: var(--text-muted, #64748b);
  padding: 4px 8px 6px;
}
.mb-wt-item {
  display: flex;
  align-items: center;
  gap: 9px;
  width: 100%;
  padding: 8px;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: var(--text-secondary, #94a3b8);
  cursor: pointer;
  text-align: left;
}
.mb-wt-item:hover { background: var(--bg-hover, rgba(255, 255, 255, 0.07)); }
.mb-wt-item-on { color: var(--text-primary, #e2e8f0); }
.mb-wt-item-on > svg:first-child { color: var(--accent, #3b82f6); }
.mb-wt-item-text { display: flex; flex-direction: column; gap: 2px; flex: 1; min-width: 0; }
.mb-wt-item-title { font-size: 12px; font-weight: 600; color: var(--text-primary, #e2e8f0); }
.mb-wt-item-sub { font-size: 10px; line-height: 1.3; color: var(--text-muted, #64748b); }
.mb-wt-item-check { color: var(--accent, #3b82f6); flex-shrink: 0; }
</style>
