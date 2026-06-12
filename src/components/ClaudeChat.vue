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
            <!-- eslint-disable-next-line vue/no-v-html -->
            <div class="assistant-text md-body" v-html="renderMd(msg.text)" />
            <span v-if="msg.partial" class="partial-cursor" />
          </div>
        </template>
      </div>

      <div v-if="busy && !hasPartialAssistant" class="chat-thinking">
        <span class="thinking-dot" /><span class="thinking-dot" /><span class="thinking-dot" />
      </div>
    </div>

    <!-- Command suggestions dropdown -->
    <div v-if="suggestions.length > 0" ref="suggestionsEl" class="cmd-suggestions">
      <div
        v-for="(s, i) in suggestions"
        :key="s.name"
        class="cmd-suggestion"
        :class="{ selected: i === suggestionIdx }"
        @mousedown.prevent="applySuggestion(s.name)"
      >
        <span class="cmd-name">/{{ s.name }}</span>
        <span class="cmd-desc">{{ s.description }}</span>
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
        @input="onInput"
      />
      <button v-if="busy" class="chat-abort-btn" title="Abort" @click="abortTurn">
        <PhStop :size="14" weight="bold" />
      </button>
      <button v-else class="chat-send-btn" :disabled="!inputText.trim()" @click="sendMessage">
        <PhArrowUp :size="14" weight="bold" />
      </button>
    </div>

    <!-- Status line below input -->
    <div class="status-line">
      <span v-if="model" class="status-item status-model">{{ model }}</span>
      <span v-if="planLabel" class="status-item status-plan">{{ planLabel }}</span>
      <span v-if="fiveHourWindow" class="status-item" :title="'5h usage window'">5h: {{ fiveHourWindow }}</span>
      <span class="status-spacer" />
      <span v-if="sessionId" class="status-item status-muted" :title="sessionId">
        {{ sessionId.slice(0, 8) }}…
      </span>
      <span v-if="turnStats" class="status-item status-muted">
        {{ turnStats.inputTokens.toLocaleString() }}↑ {{ turnStats.outputTokens.toLocaleString() }}↓
      </span>
      <span v-if="sessionCost > 0" class="status-item status-cost">
        ${{ sessionCost.toFixed(4) }}
      </span>
      <span v-if="busy" class="status-item status-busy">thinking…</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, nextTick, onMounted, onBeforeUnmount, watch } from "vue";
import { PhArrowUp, PhArrowCounterClockwise, PhWrench, PhStop } from "@phosphor-icons/vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import ClaudeIcon from "@/components/icons/ClaudeIcon.vue";
import { useClaudeChatsStore } from "@/stores/claudeChats";
import { marked } from "marked";
import DOMPurify from "dompurify";

function renderMd(text: string): string {
  return DOMPurify.sanitize(marked.parse(text) as string);
}

const props = defineProps<{ chatId: number; workspaceId: number; cwd: string }>();

const chats = useClaudeChatsStore();

interface ChatMessage {
  id: number;
  role: "user" | "assistant" | "tool";
  text: string;
  partial?: boolean;
}

// Built-in claude slash commands
interface Command { name: string; description: string }

const BUILTIN_COMMANDS: Command[] = [
  { name: "pr",           description: "Write a PR description from recent git diff" },
  { name: "clear",        description: "Clear conversation history" },
  { name: "compact",      description: "Compact conversation with summary" },
  { name: "help",         description: "Show help and available commands" },
  { name: "review",       description: "Review changes in current directory" },
  { name: "init",         description: "Initialize project with CLAUDE.md" },
  { name: "memory",       description: "Edit memory files" },
  { name: "status",       description: "Show account and session status" },
  { name: "doctor",       description: "Check Claude Code installation health" },
  { name: "config",       description: "Open settings" },
  { name: "permissions",  description: "Manage tool permissions" },
  { name: "cost",         description: "Show token and cost usage for this session" },
  { name: "model",        description: "Switch model" },
];

const allCommands = ref<Command[]>([...BUILTIN_COMMANDS]);
const suggestions = ref<Command[]>([]);
const suggestionIdx = ref(0);

interface TurnStats { inputTokens: number; outputTokens: number; costUsd: number }

interface AccountInfo {
  email: string;
  display_name: string;
  organization_type: string;  // "claude_max" | "pro" | ...
  rate_limit_tier: string;    // "default_claude_max_5x" | ...
  status_text: string;        // raw `claude status` stdout
}

function msgKey(chatId: number) { return `burrow.claude.msgs.${chatId}`; }

function loadMessages(chatId: number): ChatMessage[] {
  try {
    const raw = localStorage.getItem(msgKey(chatId));
    return raw ? JSON.parse(raw) : [];
  } catch { return []; }
}

function saveMessages(chatId: number, msgs: ChatMessage[]) {
  try {
    // Only persist non-partial messages, cap at 200 to bound storage
    const toSave = msgs.filter((m) => !m.partial).slice(-200);
    localStorage.setItem(msgKey(chatId), JSON.stringify(toSave));
  } catch {}
}

let nextMsgId = 0;
const messages = ref<ChatMessage[]>(loadMessages(props.chatId));
const inputText = ref("");
const busy = ref(false);
const sessionId = ref("");
const turnStats = ref<TurnStats | null>(null);
const sessionCost = ref(0);
const scrollEl = ref<HTMLElement | null>(null);
const inputEl = ref<HTMLTextAreaElement | null>(null);
const suggestionsEl = ref<HTMLElement | null>(null);
let unlisten: UnlistenFn | null = null;
const model = ref("");
const accountInfo = ref<AccountInfo | null>(null);

// Parse plan label from organizationType / rateLimitTier
const planLabel = computed(() => {
  const ot = accountInfo.value?.organization_type ?? "";
  const tier = accountInfo.value?.rate_limit_tier ?? "";
  if (ot === "claude_max") {
    // "default_claude_max_5x" → "Max 5×"
    const m = tier.match(/(\d+)x$/i);
    return m ? `Max ${m[1]}×` : "Max";
  }
  if (ot === "pro") return "Pro";
  if (ot === "free") return "Free";
  return ot;
});

// Parse 5h window from `claude status` plain text.
// Expected line: "5h window: 23% (2h 14m remaining)" or similar.
const fiveHourWindow = computed(() => {
  const text = accountInfo.value?.status_text ?? "";
  const m = text.match(/5[- ]h(?:our)?[^:]*:\s*([^\n]+)/i);
  return m ? m[1].trim() : "";
});

// Seed nextMsgId from loaded messages
nextMsgId = messages.value.reduce((max, m) => Math.max(max, m.id + 1), 0);

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
      if (event.model) model.value = event.model as string;
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
    // Capture usage/cost from result event
    if (type === "result") {
      const usage = event.usage as Record<string, number> | undefined;
      const cost = (event.cost_usd as number) ?? 0;
      if (usage) {
        turnStats.value = {
          inputTokens: usage.input_tokens ?? 0,
          outputTokens: usage.output_tokens ?? 0,
          costUsd: cost,
        };
        sessionCost.value += cost;
      }
    }
    saveMessages(props.chatId, messages.value);
    syncStore();
    scrollToBottom();
    return;
  }
}

async function sendMessage() {
  let text = inputText.value.trim();
  if (!text || busy.value) return;
  inputText.value = "";
  await nextTick();
  autoResize();

  // /pr: build a PR description prompt from git diff
  if (text.match(/^\/pr\b/)) {
    try {
      const stat = await invoke<{ stdout: string }>("run_git", { cwd: props.cwd, args: ["diff", "HEAD~1", "--stat", "--no-color"] });
      const diff = await invoke<{ stdout: string }>("run_git", { cwd: props.cwd, args: ["diff", "HEAD~1", "--no-color"] });
      text = `Write a PR description for these changes:\n\n${stat.stdout}\n\`\`\`diff\n${diff.stdout.slice(0, 8000)}\n\`\`\``;
    } catch (e) {
      messages.value.push({ id: nextMsgId++, role: "assistant", text: `Error reading git diff: ${e}` });
      return;
    }
  }

  messages.value.push({ id: nextMsgId++, role: "user", text });
  busy.value = true;

  // Auto-title from first user message
  const session = chats.sessions.find((s) => s.id === props.chatId);
  if (session && session.messageCount === 0) {
    chats.sync(props.chatId, { title: text.slice(0, 40) + (text.length > 40 ? "…" : "") });
  }

  saveMessages(props.chatId, messages.value);
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

async function abortTurn() {
  await invoke("claude_abort", { id: props.chatId }).catch(() => {});
  // Restart with --resume so session continues
  await invoke("claude_start", { id: props.chatId, cwd: props.cwd, resumeSessionId: sessionId.value || null }).catch(() => {});
  busy.value = false;
  const last = messages.value[messages.value.length - 1];
  if (last?.partial) last.partial = false;
  syncStore();
}

async function clearChat() {
  await invoke("claude_stop", { id: props.chatId }).catch(() => {});
  messages.value = [];
  sessionId.value = "";
  busy.value = false;
  turnStats.value = null;
  sessionCost.value = 0;
  localStorage.removeItem(msgKey(props.chatId));
  chats.sync(props.chatId, { claudeSessionId: "", busy: false, messageCount: 0, title: `Chat` });
  await invoke("claude_start", { id: props.chatId, cwd: props.cwd }).catch(() => {});
}

function updateSuggestions() {
  const val = inputText.value;
  const slashMatch = val.match(/^\/(\S*)$/);
  if (!slashMatch) { suggestions.value = []; return; }
  const q = slashMatch[1].toLowerCase();
  suggestions.value = allCommands.value.filter(
    (c) => c.name.toLowerCase().startsWith(q)
  );
  suggestionIdx.value = 0;
}

function applySuggestion(name: string) {
  inputText.value = `/${name} `;
  suggestions.value = [];
  nextTick(() => { inputEl.value?.focus(); autoResize(); });
}

function scrollSuggestionIntoView(idx: number) {
  nextTick(() => {
    if (!suggestionsEl.value) return;
    const items = suggestionsEl.value.querySelectorAll(".cmd-suggestion");
    items[idx]?.scrollIntoView({ block: "nearest" });
  });
}

function onKeydown(e: KeyboardEvent) {
  if (suggestions.value.length > 0) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      suggestionIdx.value = Math.min(suggestionIdx.value + 1, suggestions.value.length - 1);
      scrollSuggestionIntoView(suggestionIdx.value);
      return;
    }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      suggestionIdx.value = Math.max(suggestionIdx.value - 1, 0);
      scrollSuggestionIntoView(suggestionIdx.value);
      return;
    }
    if (e.key === "Tab" || (e.key === "Enter" && !e.shiftKey)) {
      e.preventDefault();
      applySuggestion(suggestions.value[suggestionIdx.value].name);
      return;
    }
    if (e.key === "Escape") { suggestions.value = []; return; }
  }
  if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); sendMessage(); }
}

function onInput() {
  autoResize();
  updateSuggestions();
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

  // Load account info (plan, 5h window) — non-blocking.
  invoke<AccountInfo>("claude_get_account", { cwd: props.cwd })
    .then((info) => { accountInfo.value = info; })
    .catch(() => {});

  // Load installed skills and merge with built-ins for command suggestions.
  try {
    const skills = await invoke<{ name: string; description: string; enabled: boolean }[]>("list_skills");
    const skillCmds: Command[] = skills
      .filter((s) => s.enabled)
      .map((s) => ({ name: s.name, description: s.description || `/${s.name} skill` }));
    // Merge: skills override built-ins with same name.
    const builtinNames = new Set(skillCmds.map((s) => s.name));
    allCommands.value = [
      ...BUILTIN_COMMANDS.filter((c) => !builtinNames.has(c.name)),
      ...skillCmds,
    ].sort((a, b) => a.name.localeCompare(b.name));
  } catch { /* browser-only dev without Tauri */ }
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

/* Status line */
.status-line {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 3px 10px;
  background: var(--bg-panel);
  border-top: 1px solid var(--border);
  flex-shrink: 0;
  min-height: 22px;
}

.status-spacer { flex: 1; }

.status-item {
  font-size: 10px;
  font-family: var(--font-mono);
  color: var(--text-secondary);
  white-space: nowrap;
}

.status-muted { color: var(--text-muted); }
.status-model { color: var(--text-primary); font-weight: 600; }
.status-plan {
  color: #f59e0b;
  font-weight: 600;
  background: color-mix(in srgb, #f59e0b 12%, transparent);
  padding: 1px 5px;
  border-radius: 3px;
}
.status-cost { color: #a78bfa; }
.status-busy { color: var(--accent); animation: blink 1s step-end infinite; }

/* Command suggestions */
.cmd-suggestions {
  border-top: 1px solid var(--border);
  background: var(--bg-panel);
  max-height: 200px;
  overflow-y: auto;
  flex-shrink: 0;
}

.cmd-suggestion {
  display: flex;
  align-items: baseline;
  gap: 10px;
  padding: 6px 12px;
  cursor: pointer;
  transition: background .1s;
}
.cmd-suggestion:hover,
.cmd-suggestion.selected { background: var(--bg-hover); }

.cmd-name {
  font-family: var(--font-mono);
  font-size: 12px;
  font-weight: 600;
  color: var(--accent);
  flex-shrink: 0;
  min-width: 100px;
}

.cmd-desc {
  font-size: 11px;
  color: var(--text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

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

.chat-abort-btn {
  background: #dc2626;
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
  transition: background .12s;
}
.chat-abort-btn:hover { background: #b91c1c; }

/* Markdown body inside assistant bubble */
.md-body {
  font-family: var(--font-ui);
  font-size: 13px;
  color: var(--text-primary);
  line-height: 1.6;
  white-space: normal;
}
.md-body :deep(p) { margin: 0 0 8px; }
.md-body :deep(p:last-child) { margin-bottom: 0; }
.md-body :deep(ul), .md-body :deep(ol) { margin: 4px 0 8px; padding-left: 20px; }
.md-body :deep(li) { margin: 2px 0; }
.md-body :deep(code) { font-family: var(--font-mono); font-size: 11px; background: color-mix(in srgb, var(--accent) 12%, transparent); padding: 1px 4px; border-radius: 3px; }
.md-body :deep(pre) { background: var(--bg-base); border: 1px solid var(--border); border-radius: 6px; padding: 10px 12px; overflow-x: auto; margin: 6px 0; }
.md-body :deep(pre code) { background: none; padding: 0; font-size: 11px; }
.md-body :deep(blockquote) { border-left: 3px solid var(--accent); margin: 6px 0; padding-left: 10px; color: var(--text-secondary); }
.md-body :deep(h1), .md-body :deep(h2), .md-body :deep(h3) { font-weight: 700; margin: 10px 0 4px; color: var(--text-primary); }
.md-body :deep(h1) { font-size: 16px; }
.md-body :deep(h2) { font-size: 14px; }
.md-body :deep(h3) { font-size: 13px; }
.md-body :deep(a) { color: var(--accent); text-decoration: underline; }
.md-body :deep(hr) { border: none; border-top: 1px solid var(--border); margin: 8px 0; }
.md-body :deep(table) { border-collapse: collapse; font-size: 12px; margin: 6px 0; }
.md-body :deep(th), .md-body :deep(td) { border: 1px solid var(--border); padding: 4px 8px; }
.md-body :deep(th) { background: var(--bg-panel); font-weight: 600; }
</style>
