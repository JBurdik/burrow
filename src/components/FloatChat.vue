<template>
  <!-- Collapsed: round launcher button, bottom-right -->
  <button
    v-if="!ui.floatChatOpen"
    class="fc-launcher"
    :class="{ 'fc-busy': busy }"
    title="Mission Control — control Burrow with chat"
    @click="open"
  >
    <PhSparkle :size="22" weight="fill" />
    <span v-if="busy" class="fc-dot" />
  </button>

  <!-- Expanded: compact chat card -->
  <div v-else class="fc-card">
    <div class="fc-head">
      <PhSparkle :size="14" weight="fill" class="fc-head-icon" />
      <span class="fc-head-title">Mission Control</span>
      <span class="fc-head-sub" :title="cwd">drives this app</span>
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
        :workspace-id="wsId"
        :cwd="cwd"
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

const props = defineProps<{ cwd: string; wsId: number }>();

const ui = useUIStore();
const chats = useClaudeChatsStore();

// One persistent control session per workspace, reused across open/collapse and
// app restarts. Keyed by workspace id in localStorage.
const MAP_KEY = "burrow.floatchat.sessions";
function loadMap(): Record<number, number> {
  try { return JSON.parse(localStorage.getItem(MAP_KEY) || "{}"); } catch { return {}; }
}
function saveMap(m: Record<number, number>) {
  localStorage.setItem(MAP_KEY, JSON.stringify(m));
}

const controlChatId = ref<number | null>(null);

function ensureControlSession(wsId: number) {
  const map = loadMap();
  const existing = map[wsId];
  if (existing && chats.sessions.find((s) => s.id === existing)) {
    controlChatId.value = existing;
    return;
  }
  // create() flips the workspace's active chat; restore it so the in-tab Claude
  // pane isn't yanked to this hidden control session.
  const prevActive = chats.activeByWs[wsId];
  const sess = chats.create(wsId);
  chats.sync(sess.id, { title: "Mission Control" });
  if (prevActive) chats.setActive(wsId, prevActive);
  map[wsId] = sess.id;
  saveMap(map);
  controlChatId.value = sess.id;
}

// Resolve the control session lazily — only once the card is first opened for a
// workspace, so we don't spawn a `claude` process for users who never use it.
watch(
  () => [ui.floatChatOpen, props.wsId] as const,
  ([isOpen, wsId]) => {
    if (isOpen && typeof wsId === "number") ensureControlSession(wsId);
  },
  { immediate: true },
);

function open() {
  ui.floatChatOpen = true;
}

// Busy badge: control session's agent is mid-turn.
const busy = computed(() => {
  const id = controlChatId.value;
  return id !== null && !!chats.sessions.find((s) => s.id === id)?.busy;
});

onMounted(() => {
  if (ui.floatChatOpen && typeof props.wsId === "number") ensureControlSession(props.wsId);
});

const MC_PRIMER = `You are Burrow's Mission Control assistant. Burrow is a desktop IDE that runs AI coding agents in terminal tabs across multiple workspaces.

You control the app itself by running the \`burrow\` CLI via your Bash tool. Whenever the user asks you to act on the app — spawn an agent, open or switch something, list what's open — run the matching command instead of just describing it:

- \`burrow list-workspaces\` — list every workspace (id, name, path).
- \`burrow list-tabs [--ws ID]\` — list a workspace's tabs (pty-id, title).
- \`burrow new-tab [--ws ID] [--cmd CMD]\` — open a new terminal tab (optionally run CMD).
- \`burrow spawn [--cwd DIR] <command...>\` — launch a command (e.g. an agent like \`claude\`) in a new tab in the current project.
- \`burrow focus-workspace <ID>\` — switch the UI to a workspace.
- \`burrow focus-tab <ID>\` — activate a tab by its pty-id.
- \`burrow worktree <branch> [--base-ref REF]\` — create a git worktree (nested under the repo).

Be concise. Confirm what you did. If a request is ambiguous (which workspace? which agent?), run \`burrow list-workspaces\` / \`burrow list-tabs\` first to ground yourself, then act.`;
</script>

<style scoped>
/* ── Collapsed launcher ── */
.fc-launcher {
  position: fixed;
  bottom: 18px;
  right: 18px;
  z-index: 60;
  width: 52px;
  height: 52px;
  border-radius: 50%;
  border: 1px solid var(--border, rgba(255, 255, 255, 0.12));
  background: var(--accent, #7c5cff);
  color: #fff;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.35);
  transition: transform 0.12s ease, box-shadow 0.12s ease;
}
.fc-launcher:hover {
  transform: translateY(-2px);
  box-shadow: 0 10px 26px rgba(0, 0, 0, 0.42);
}
.fc-launcher.fc-busy { animation: fc-pulse 1.4s ease-in-out infinite; }
.fc-dot {
  position: absolute;
  top: 6px;
  right: 6px;
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: #4ade80;
  border: 2px solid var(--bg-base, #14141a);
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
  width: 390px;
  height: 540px;
  max-height: calc(100vh - 64px);
  display: flex;
  flex-direction: column;
  background: var(--bg-panel, #1b1b22);
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
.fc-body { flex: 1; min-height: 0; }
</style>
