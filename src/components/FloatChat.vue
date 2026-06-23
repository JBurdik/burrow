<template>
  <!-- Collapsed: round launcher button, bottom-right -->
  <button
    v-if="!ui.floatChatOpen"
    class="fc-launcher"
    :class="{ 'fc-busy': busy, 'fc-attn': needsAttention }"
    :title="launcherTitle"
    @click="open"
  >
    <PhSparkle :size="20" weight="fill" class="fc-launcher-icon" />
    <span v-if="busy" class="fc-dot" />
    <span v-if="needsAttention" class="fc-badge" :class="`fc-badge-${attentionKind}`" />
  </button>

  <!-- Expanded: compact chat card -->
  <div v-else class="fc-card">
    <div class="fc-head">
      <PhSparkle :size="14" weight="fill" class="fc-head-icon" />
      <span class="fc-head-title">Manager</span>
      <span class="fc-head-sub" :title="rootCwd">{{ rootName }}</span>
      <button
        class="fc-head-btn fc-wt-btn"
        :class="{ 'fc-wt-on': worktreeMode }"
        :title="worktreeMode
          ? 'Spawn mode: worktree per agent (isolated) — click for active branch'
          : 'Spawn mode: active branch (shared) — click for worktree per agent'"
        @click="toggleWorktreeMode"
      >
        <PhTree v-if="worktreeMode" :size="13" weight="bold" />
        <PhGitBranch v-else :size="13" weight="bold" />
      </button>
      <button class="fc-head-btn" title="Collapse" @click="ui.toggleFloatChat()">
        <PhMinus :size="13" weight="bold" />
      </button>
    </div>
    <div class="fc-body">
      <ClaudeChat
        v-if="controlChatId !== null"
        :key="controlChatId"
        compact
        :chat-id="controlChatId"
        :workspace-id="rootId"
        :cwd="rootCwd"
        :append-system-prompt="managerPrimer"
        :avatar-src="managerAvatar"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { PhSparkle, PhMinus, PhGitBranch, PhTree } from "@phosphor-icons/vue";
import { getDefaultManagerPrimer, SPAWN_MODE_WORKTREE, SPAWN_MODE_BRANCH } from "@/utils/managerPrimer";
import ClaudeChat from "./ClaudeChat.vue";
import { useUIStore } from "@/stores/ui";
import { useClaudeChatsStore } from "@/stores/claudeChats";
import { useWorkspaceStore } from "@/stores/workspace";
import managerAvatar from "@/assets/manager-avatar.png";

const props = defineProps<{ cwd: string; wsId: number }>();

const ui = useUIStore();
const chats = useClaudeChatsStore();
const wsStore = useWorkspaceStore();

// activeWsId tracks which workspace the Manager is currently anchored to.
// It only updates when the Manager session is idle so that switching workspaces
// while a task is in progress doesn't interrupt the running claude process.
const activeWsId = ref<number>(props.wsId);
const activeCwd = ref<string>(props.cwd);

// The Manager is anchored to the ROOT repo, not the active worktree. Worktrees
// are their own workspace rows (parent_id set); keying the session by the root id
// keeps the same Manager session alive as you switch between a repo's worktrees,
// instead of showing an empty one per worktree.
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

// Worktree preference: false = spawn agents in the repo's active branch (no
// worktree, default), true = isolate each spawned agent in its own git worktree.
// Persisted globally; the Manager primer reflects the current choice each turn.
const WT_KEY = "burrow.floatchat.worktreeMode";
const worktreeMode = ref<boolean>(localStorage.getItem(WT_KEY) === "1");
watch(worktreeMode, (v) => localStorage.setItem(WT_KEY, v ? "1" : "0"));
function toggleWorktreeMode() { worktreeMode.value = !worktreeMode.value; }

// When the active workspace changes, adopt it only if the Manager is idle.
// If a task is running, defer until it finishes so we don't kill claude mid-turn.
watch(
  () => [props.wsId, props.cwd] as const,
  ([wsId, cwd]) => {
    const busy = controlChatId.value
      ? chats.sessions.find((s) => s.id === controlChatId.value)?.busy
      : false;
    if (!busy) {
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

// Resolve the Manager session lazily — only once the card is first opened for a
// repo, so we don't spawn a `claude` process for users who never use it.
watch(
  () => [ui.floatChatOpen, rootId.value] as const,
  ([isOpen, repoId]) => {
    if (isOpen && typeof repoId === "number") ensureControlSession(repoId);
  },
  { immediate: true },
);

function open() {
  ui.floatChatOpen = true;
  finishedWhileCollapsed.value = false;
}

// The live Manager session row (status/busy mirror the in-tab chat model).
const session = computed(() =>
  controlChatId.value === null
    ? null
    : chats.sessions.find((s) => s.id === controlChatId.value) ?? null,
);

// Busy dot: Manager session's agent is mid-turn.
const busy = computed(() => !!session.value?.busy);

// Latch a turn that completed while the card was collapsed, so the user gets a
// "finished while you were away" badge even though chat status falls back to idle.
const finishedWhileCollapsed = ref(false);
watch(
  () => session.value?.busy,
  (now, prev) => {
    if (prev && !now && !ui.floatChatOpen) finishedWhileCollapsed.value = true;
    // Adopt a deferred workspace switch that was blocked by an in-progress task.
    if (prev && !now) {
      activeWsId.value = props.wsId;
      activeCwd.value = props.cwd;
    }
  },
);

// Attention badge: blocked on input (permission/waiting) or finished while away.
// Permission outranks waiting outranks a plain finish.
const attentionKind = computed<"permission" | "waiting" | "done" | null>(() => {
  const st = session.value?.status;
  if (st === "permission") return "permission";
  if (st === "waiting") return "waiting";
  if (finishedWhileCollapsed.value) return "done";
  return null;
});
const needsAttention = computed(() => attentionKind.value !== null);

const launcherTitle = computed(() => {
  switch (attentionKind.value) {
    case "permission": return "Manager needs a permission decision";
    case "waiting": return "Manager is waiting for your input";
    case "done": return "Manager finished while you were away";
    default: return "Manager — orchestrate worktrees, agents & PRs with chat";
  }
});

onMounted(() => {
  if (ui.floatChatOpen && typeof rootId.value === "number") ensureControlSession(rootId.value);
});

const projectManagerPrompt = ref('');
watch(rootCwd, async (cwd) => {
  if (!cwd) return;
  try {
    const content = await invoke<string>('read_text_file', { path: cwd + '/.burrow/manager.md' });
    projectManagerPrompt.value = content.replace(/<!--[\s\S]*?-->/g, '').trim();
  } catch {
    projectManagerPrompt.value = '';
  }
}, { immediate: true });

const managerPrimer = computed(() => {
  if (projectManagerPrompt.value) {
    const spawnMode = worktreeMode.value ? SPAWN_MODE_WORKTREE : SPAWN_MODE_BRANCH;
    return projectManagerPrompt.value + '\n\n---\n\n' + spawnMode;
  }
  return getDefaultManagerPrimer(worktreeMode.value);
});
</script>

<style scoped>
/* ── Collapsed launcher ── */
.fc-launcher {
  position: fixed;
  bottom: 18px;
  right: 18px;
  z-index: 60;
  width: 46px;
  height: 46px;
  border-radius: 50%;
  border: 1px solid var(--border, rgba(255, 255, 255, 0.12));
  background-color: var(--bg-base, #0d0d0d);
  background-image: linear-gradient(var(--bg-panel, #111111), var(--bg-panel, #111111));
  backdrop-filter: none;
  color: var(--text-muted, #999);
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.35);
  transition: transform 0.12s ease, box-shadow 0.12s ease, border-color 0.12s ease, color 0.12s ease;
}
.fc-launcher-icon { color: var(--accent, #7c5cff); }
.fc-launcher:hover {
  transform: translateY(-2px);
  border-color: var(--accent, #7c5cff);
  box-shadow: 0 10px 26px rgba(0, 0, 0, 0.42);
}
.fc-launcher.fc-busy { animation: fc-pulse 1.4s ease-in-out infinite; }
.fc-dot {
  position: absolute;
  top: 5px;
  right: 5px;
  width: 9px;
  height: 9px;
  border-radius: 50%;
  background: #4ade80;
  border: 2px solid var(--bg-panel, #1b1b22);
}
/* Attention badge — distinct from the busy dot: a colored ring at top-left. */
.fc-badge {
  position: absolute;
  top: 4px;
  left: 4px;
  width: 12px;
  height: 12px;
  border-radius: 50%;
  border: 2px solid var(--bg-panel, #1b1b22);
  box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.3);
}
.fc-badge-permission { background: #f59e0b; animation: fc-badge-pulse 1.2s ease-in-out infinite; }
.fc-badge-waiting { background: #3b82f6; }
.fc-badge-done { background: #22c55e; }
.fc-launcher.fc-attn { border-color: var(--accent, #7c5cff); }
@keyframes fc-badge-pulse {
  0%, 100% { box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.3); }
  50% { box-shadow: 0 0 0 4px rgba(245, 158, 11, 0.35); }
}
@keyframes fc-pulse {
  0%, 100% { box-shadow: 0 6px 20px rgba(0, 0, 0, 0.35); }
  50% { box-shadow: 0 6px 26px var(--accent, #7c5cff); }
}

/* ── Expanded card ── */
.fc-card {
  position: fixed;
  bottom: 18px;
  right: 18px;
  z-index: 60;
  width: 460px;
  height: 540px;
  max-height: calc(100vh - 64px);
  display: flex;
  flex-direction: column;
  /* Force a SOLID card even under translucent themes (e.g. "stonks", whose
     --bg-panel is rgba): composite the panel tint over an opaque --bg-base. */
  background-color: var(--bg-base, #0d0d0d);
  background-image: linear-gradient(var(--bg-panel, #111111), var(--bg-panel, #111111));
  backdrop-filter: none;
  border: 1px solid var(--border, rgba(255, 255, 255, 0.12));
  border-radius: 14px;
  overflow: hidden;
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5);
}
.fc-head {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 8px 10px;
  background-color: var(--bg-base, #0d0d0d);
  background-image: linear-gradient(var(--bg-panel, #111111), var(--bg-panel, #111111));
  backdrop-filter: none;
  border-bottom: 1px solid var(--border, rgba(255, 255, 255, 0.1));
  flex-shrink: 0;
}
.fc-head-icon { color: var(--accent, #7c5cff); flex-shrink: 0; }
.fc-head-title { font-size: 12px; font-weight: 600; color: var(--text-primary, #eee); }
.fc-head-sub {
  font-size: 10px;
  color: var(--text-muted, #888);
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.fc-head-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--text-muted, #999);
  cursor: pointer;
}
.fc-head-btn:hover { background: var(--bg-hover, rgba(255, 255, 255, 0.08)); color: var(--text-primary, #eee); }
.fc-wt-btn.fc-wt-on { color: var(--accent, #7c5cff); background: var(--bg-hover, rgba(124, 92, 255, 0.14)); }
.fc-body {
  flex: 1;
  min-height: 0;
  /* Opaque backing so a translucent-theme ClaudeChat can't show the panes behind. */
  background-color: var(--bg-base, #0d0d0d);
  background-image: linear-gradient(var(--bg-panel, #111111), var(--bg-panel, #111111));
}
/* Kill any per-theme translucency on the embedded chat so the card stays solid. */
.fc-body :deep(.claude-chat) { background: transparent; backdrop-filter: none; }
</style>
