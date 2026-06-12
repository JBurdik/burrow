<template>
  <div class="claude-chat">
    <div class="chat-header">
      <ClaudeIcon :size="16" class="chat-header-icon" />
      <span class="chat-header-title">Claude</span>
      <span class="chat-header-cwd" :title="cwd">{{ cwdDisplay }}</span>
      <button class="chat-clear-btn" title="New conversation" @click="clearChat">
        <PhArrowCounterClockwise :size="13" />
      </button>
    </div>

    <div ref="scrollEl" class="chat-messages">
      <div v-if="messages.length === 0" class="chat-empty">
        <ClaudeIcon :size="32" class="chat-empty-icon" />
        <span>Ask Claude anything about this project</span>
        <span class="chat-empty-sub">Working in {{ cwdDisplay }}</span>
      </div>

      <div
        v-for="msg in messages"
        :key="msg.id"
        class="chat-message"
        :class="[`role-${msg.role}`, { partial: msg.partial }]"
      >
        <template v-if="msg.role === 'user'">
          <div class="bubble bubble-user">{{ msg.text }}</div>
        </template>
        <template v-else-if="msg.role === 'tool'">
          <div class="bubble bubble-tool">
            <PhWrench :size="11" class="tool-icon" />
            <span class="tool-name">{{ msg.text }}</span>
          </div>
        </template>
        <template v-else>
          <div class="bubble bubble-assistant">
            <pre class="assistant-text">{{ msg.text }}</pre>
            <span v-if="msg.partial" class="partial-cursor" />
          </div>
        </template>
      </div>

      <div v-if="busy && !hasPartialAssistant" class="chat-thinking">
        <span class="thinking-dot" /><span class="thinking-dot" /><span class="thinking-dot" />
      </div>
    </div>

    <div class="chat-input-area">
      <textarea
        ref="inputEl"
        v-model="inputText"
        class="chat-input"
        placeholder="Message Claude… (Enter to send, Shift+Enter for newline)"
        rows="1"
        :disabled="busy"
        @keydown="onKeydown"
        @input="autoResize"
      />
      <button class="chat-send-btn" :disabled="!inputText.trim() || busy" @click="sendMessage">
        <PhArrowUp :size="14" weight="bold" />
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, nextTick, onMounted, onBeforeUnmount, watch } from "vue";
import { PhArrowUp, PhArrowCounterClockwise, PhWrench } from "@phosphor-icons/vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import ClaudeIcon from "@/components/icons/ClaudeIcon.vue";
import { useClaudeChatsStore } from "@/stores/claudeChats";

const props = defineProps<{ chatId: number; workspaceId: number; cwd: string }>();

const chats = useClaudeChatsStore();

interface ChatMessage {
  id: number;
  role: "user" | "assistant" | "tool";
  text: string;
  partial?: boolean;
}

let nextMsgId = 0;
const messages = ref<ChatMessage[]>([]);
const inputText = ref("");
const busy = ref(false);
const sessionId = ref("");
const scrollEl = ref<HTMLElement | null>(null);
const inputEl = ref<HTMLTextAreaElement | null>(null);
let unlisten: UnlistenFn | null = null;

const cwdDisplay = computed(() => {
  const parts = props.cwd.replace(/^\/Users\/[^/]+/, "~").split("/");
  return parts.slice(-2).join("/") || props.cwd;
});

const hasPartialAssistant = computed(() =>
  messages.value.some((m) => m.role === "assistant" && m.partial)
);

function scrollToBottom() {
  nextTick(() => {
    if (scrollEl.value) scrollEl.value.scrollTop = scrollEl.value.scrollHeight;
  });
}

function syncStore() {
  chats.sync(props.chatId, {
    busy: busy.value,
    messageCount: messages.value.filter((m) => m.role !== "tool").length,
  });
}

function onLine(line: string) {
  let event: Record<string, unknown>;
  try { event = JSON.parse(line) as Record<string, unknown>; }
  catch { return; }

  const type = event.type as string;

  if (type === "system") {
    const sub = event.subtype as string;
    if (sub === "init") {
      const sid = (event.session_id as string) ?? "";
      sessionId.value = sid;
      chats.sync(props.chatId, { claudeSessionId: sid });
    }
    if (sub === "hook_started" || sub === "hook_response") return;
  }

  if (type === "assistant") {
    const content = ((event.message as Record<string, unknown>)?.content ?? []) as Array<Record<string, unknown>>;
    const textParts = content.filter((b) => b.type === "text").map((b) => b.text as string).join("");
    const toolBlocks = content.filter((b) => b.type === "tool_use");

    if (textParts) {
      const last = messages.value[messages.value.length - 1];
      if (last?.role === "assistant" && last.partial) {
        last.text += textParts;
      } else {
        messages.value.push({ id: nextMsgId++, role: "assistant", text: textParts, partial: true });
      }
    }
    for (const tb of toolBlocks) {
      const name = (tb.name as string) ?? "tool";
      const inputStr = tb.input ? " " + JSON.stringify(tb.input).slice(0, 80) : "";
      messages.value.push({ id: nextMsgId++, role: "tool", text: name + inputStr });
    }
    scrollToBottom();
    return;
  }

  if (type === "result" || type === "exit") {
    busy.value = false;
    const last = messages.value[messages.value.length - 1];
    if (last?.partial) last.partial = false;
    syncStore();
    scrollToBottom();
    return;
  }
}

async function sendMessage() {
  const text = inputText.value.trim();
  if (!text || busy.value) return;
  inputText.value = "";
  await nextTick();
  autoResize();
  messages.value.push({ id: nextMsgId++, role: "user", text });
  busy.value = true;

  // Auto-title from first user message
  const session = chats.sessions.find((s) => s.id === props.chatId);
  if (session && session.messageCount === 0) {
    chats.sync(props.chatId, { title: text.slice(0, 40) + (text.length > 40 ? "…" : "") });
  }

  syncStore();
  scrollToBottom();
  try {
    await invoke("claude_send", { id: props.chatId, text, sessionId: sessionId.value || null });
  } catch (e) {
    messages.value.push({ id: nextMsgId++, role: "assistant", text: `Error: ${e}` });
    busy.value = false;
    syncStore();
  }
}

async function clearChat() {
  await invoke("claude_stop", { id: props.chatId }).catch(() => {});
  messages.value = [];
  sessionId.value = "";
  busy.value = false;
  chats.sync(props.chatId, { claudeSessionId: "", busy: false, messageCount: 0, title: `Chat` });
  await invoke("claude_start", { id: props.chatId, cwd: props.cwd }).catch(() => {});
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); sendMessage(); }
}

function autoResize() {
  const el = inputEl.value;
  if (!el) return;
  el.style.height = "auto";
  el.style.height = Math.min(el.scrollHeight, 160) + "px";
}

onMounted(async () => {
  const stored = chats.sessions.find((s) => s.id === props.chatId)?.claudeSessionId ?? "";
  if (stored) sessionId.value = stored;
  await invoke("claude_start", {
    id: props.chatId,
    cwd: props.cwd,
    resumeSessionId: stored || null,
  }).catch(() => {});
  unlisten = await listen<string>(`claude-data-${props.chatId}`, (ev) => onLine(ev.payload));
});

onBeforeUnmount(() => {
  unlisten?.();
  invoke("claude_stop", { id: props.chatId }).catch(() => {});
});

watch(() => props.chatId, () => nextTick(() => inputEl.value?.focus()));
</script>

<style scoped>
.claude-chat {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--bg-base);
  overflow: hidden;
}

.chat-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
  background: var(--bg-panel);
}

.chat-header-icon { color: #d97706; flex-shrink: 0; }

.chat-header-title {
  font-size: 12px;
  font-weight: 700;
  color: var(--text-primary);
  letter-spacing: 0.02em;
}

.chat-header-cwd {
  flex: 1;
  font-size: 10px;
  font-family: var(--font-mono);
  color: var(--text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.chat-clear-btn {
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  padding: 4px;
  border-radius: 5px;
  transition: color .12s, background .12s;
}
.chat-clear-btn:hover { color: var(--text-primary); background: var(--bg-hover); }

.chat-messages {
  flex: 1;
  overflow-y: auto;
  padding: 16px 16px 8px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  scroll-behavior: smooth;
}

.chat-empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  color: var(--text-muted);
  font-size: 13px;
  text-align: center;
  padding: 40px 20px;
}
.chat-empty-icon { opacity: 0.25; margin-bottom: 4px; }
.chat-empty-sub { font-size: 11px; font-family: var(--font-mono); opacity: 0.7; }

.bubble {
  max-width: 90%;
  padding: 10px 13px;
  border-radius: 10px;
  font-size: 13px;
  line-height: 1.55;
  word-break: break-word;
}

.role-user { display: flex; justify-content: flex-end; }
.bubble-user { background: var(--accent); color: #fff; border-bottom-right-radius: 4px; }

.role-assistant { display: flex; justify-content: flex-start; }
.bubble-assistant {
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-bottom-left-radius: 4px;
  position: relative;
  max-width: 95%;
}
.assistant-text {
  margin: 0;
  font-family: var(--font-mono);
  font-size: 12px;
  white-space: pre-wrap;
  color: var(--text-primary);
}
.partial-cursor {
  display: inline-block;
  width: 2px;
  height: 13px;
  background: var(--accent);
  vertical-align: middle;
  margin-left: 2px;
  animation: blink 1s step-end infinite;
}
@keyframes blink { 0%, 100% { opacity: 1; } 50% { opacity: 0; } }

.role-tool { display: flex; justify-content: flex-start; }
.bubble-tool {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 4px 10px;
  background: color-mix(in srgb, var(--accent) 10%, transparent);
  border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
  border-radius: 20px;
  font-size: 11px;
  font-family: var(--font-mono);
  color: var(--text-secondary);
  max-width: 90%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tool-icon { color: var(--accent); flex-shrink: 0; }
.tool-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

.chat-thinking {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 0;
}
.thinking-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--text-muted);
  animation: thinking 1.2s ease-in-out infinite;
}
.thinking-dot:nth-child(2) { animation-delay: 0.2s; }
.thinking-dot:nth-child(3) { animation-delay: 0.4s; }
@keyframes thinking { 0%, 80%, 100% { opacity: 0.3; transform: scale(0.8); } 40% { opacity: 1; transform: scale(1); } }

.chat-input-area {
  display: flex;
  align-items: flex-end;
  gap: 8px;
  padding: 10px 12px;
  border-top: 1px solid var(--border);
  background: var(--bg-panel);
  flex-shrink: 0;
}

.chat-input {
  flex: 1;
  background: var(--bg-base);
  border: 1px solid var(--border);
  border-radius: 8px;
  color: var(--text-primary);
  font-family: var(--font-ui);
  font-size: 13px;
  line-height: 1.5;
  outline: none;
  padding: 8px 11px;
  resize: none;
  min-height: 36px;
  max-height: 160px;
  overflow-y: auto;
  transition: border-color .15s;
}
.chat-input:focus { border-color: var(--accent); }
.chat-input:disabled { opacity: 0.5; cursor: not-allowed; }
.chat-input::placeholder { color: var(--text-muted); }

.chat-send-btn {
  background: var(--accent);
  border: none;
  border-radius: 7px;
  color: #fff;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  flex-shrink: 0;
  transition: background .12s, opacity .12s;
}
.chat-send-btn:hover:not(:disabled) { background: var(--accent-dim); }
.chat-send-btn:disabled { opacity: 0.4; cursor: default; }
</style>
