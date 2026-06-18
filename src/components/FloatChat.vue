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
        :append-system-prompt="MC_PRIMER"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted } from "vue";
import { PhSparkle, PhMinus } from "@phosphor-icons/vue";
import ClaudeChat from "./ClaudeChat.vue";
import { useUIStore } from "@/stores/ui";
import { useClaudeChatsStore } from "@/stores/claudeChats";
import { useWorkspaceStore } from "@/stores/workspace";

const props = defineProps<{ cwd: string; wsId: number }>();

const ui = useUIStore();
const chats = useClaudeChatsStore();
const wsStore = useWorkspaceStore();

// The Manager is anchored to the ROOT repo, not the active worktree. Worktrees
// are their own workspace rows (parent_id set); keying the session by the root id
// keeps the same Manager session alive as you switch between a repo's worktrees,
// instead of showing an empty one per worktree.
const root = computed(() => {
  const w = wsStore.workspaces.find((x) => x.id === props.wsId);
  if (w?.parent_id) {
    const parent = wsStore.workspaces.find((x) => x.id === w.parent_id);
    if (parent) return parent;
  }
  return w ?? null;
});
const rootId = computed(() => root.value?.id ?? props.wsId);
const rootCwd = computed(() => root.value?.path ?? props.cwd);
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

const MC_PRIMER = `You are Burrow's **Manager** — a persistent per-repo orchestrator. Burrow is a desktop IDE that runs AI coding agents in terminal tabs across multiple workspaces. You stay anchored to one repository and coordinate its worktrees, agents, and pull requests on the user's behalf.

You drive the app and git/GitHub by running the \`burrow\` CLI via your Bash tool. Whenever the user asks you to act — create a worktree, spawn an agent, open or switch something, manage a PR — run the matching command instead of just describing it.

App / navigation:
- \`burrow list-workspaces\` — list every workspace (id, name, path).
- \`burrow list-tabs [--ws ID]\` — list a workspace's tabs (pty-id, title).
- \`burrow new-tab [--ws ID] [--cmd CMD]\` — open a new terminal tab (optionally run CMD).
- \`burrow focus-workspace <ID>\` / \`burrow focus-tab <ID>\` — switch the UI.

Orchestration (your core job):
- \`burrow worktree <branch> [--base-ref REF]\` — create a git worktree (nested under the repo).
- \`burrow spawn [--cwd DIR] <command...>\` — launch an agent (e.g. \`claude\`) in a new tab. To put an agent on a fresh worktree, create the worktree first, then \`burrow spawn --cwd <worktree-path> claude\`.
- \`burrow worktree-remove <branch|path> [--force]\` — delete a worktree (git worktree + its Burrow row). **Always ask the user to confirm before removing a worktree**, and only after the work on it is merged or no longer needed.

Pull requests (via the \`gh\` CLI under the hood):
- \`burrow pr-create --title T --body B [--base main]\` — open a PR for the current branch.
- \`burrow pr-list [--state open|closed|all]\` — list PRs.
- \`burrow pr-view <number>\` — show a PR's details.
- \`burrow pr-merge <number> [--squash]\` — merge a PR.

Be concise. Confirm what you did. If a request is ambiguous (which worktree? which agent? which PR?), run the relevant \`list\` command first to ground yourself, then act. Destructive actions (worktree-remove, pr-merge) require explicit user confirmation first.`;
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
