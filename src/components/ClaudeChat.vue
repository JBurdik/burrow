<template>
  <div class="claude-chat">
    <div class="chat-main">
    <div class="chat-header">
      <ClaudeIcon :size="16" class="chat-header-icon" />
      <span class="chat-header-title">Claude</span>
      <span class="chat-header-cwd" :title="cwd">{{ cwdDisplay }}</span>
      <div class="perm-mode-dropdown">
        <button
          ref="permBtnEl"
          class="chat-header-btn perm-mode-btn"
          :class="{ 'btn-danger-active': permMeta.danger, 'btn-active': permMode === 'acceptEdits' }"
          :title="permMeta.title"
          @click="togglePermMenu"
        >
          <PhShieldWarning v-if="permMode === 'bypassPermissions'" :size="13" weight="bold" />
          <PhPencilSimple v-else-if="permMode === 'acceptEdits'" :size="13" weight="bold" />
          <PhShieldCheck v-else :size="13" weight="bold" />
          <span class="perm-mode-label">{{ permMeta.label }}</span>
          <PhCaretDown :size="9" weight="bold" class="perm-mode-caret" />
        </button>
        <!-- Teleported to body so the float-card's `overflow:hidden` can't clip it. -->
        <Teleport to="body">
          <div
            v-if="permMenuOpen"
            ref="permMenuEl"
            class="perm-mode-menu"
            :style="{ top: permMenuPos.top + 'px', right: permMenuPos.right + 'px' }"
          >
            <button
              v-for="m in PERM_MODES"
              :key="m"
              class="perm-mode-item"
              :class="{ 'perm-mode-item-active': permMode === m, 'perm-mode-item-danger': PERM_META[m].danger }"
              :title="PERM_META[m].title"
              @click="selectPermMode(m)"
            >
              <PhShieldWarning v-if="m === 'bypassPermissions'" :size="13" weight="bold" />
              <PhPencilSimple v-else-if="m === 'acceptEdits'" :size="13" weight="bold" />
              <PhShieldCheck v-else :size="13" weight="bold" />
              <span>{{ PERM_META[m].label }}</span>
            </button>
          </div>
        </Teleport>
      </div>
      <button class="chat-header-btn" title="New conversation" @click="clearChat">
        <PhArrowCounterClockwise :size="13" />
      </button>
      <button
        v-if="!compact"
        class="chat-header-btn"
        :class="{ 'btn-active': changesVisible }"
        :title="changesVisible ? 'Hide changes' : 'Show changes'"
        @click="changesVisible = !changesVisible"
      >
        <PhGitDiff :size="13" />
        <span v-if="changedFiles.length > 0" class="changes-badge">{{ changedFiles.length }}</span>
      </button>
    </div>

    <!-- Permission prompt (Bash / generic tool) -->
    <div v-if="pendingPermission" class="permission-banner">
      <PhShieldWarning :size="14" class="perm-icon" />
      <div class="perm-body">
        <span class="perm-title">{{ pendingPermission.toolName }} wants to run</span>
        <code class="perm-detail">{{ permissionDetail }}</code>
      </div>
      <button class="perm-btn perm-allow" @click="respondPermission(true)" title="Allow once (Y)">
        Allow <kbd class="perm-kbd">Y</kbd>
      </button>
      <button class="perm-btn perm-always" @click="respondPermission(true, { always: true })" title="Always allow this tool">
        Always
      </button>
      <button class="perm-btn perm-deny" @click="respondPermission(false)" title="Deny (N)">
        Deny <kbd class="perm-kbd">N</kbd>
      </button>
    </div>

    <!-- File edit: diff preview with Accept / Reject -->
    <div v-if="pendingDiff && diffPreview" class="diff-banner">
      <div class="diff-banner-head">
        <PhGitDiff :size="13" class="perm-icon" />
        <span class="perm-title">{{ pendingDiff.toolName }}</span>
        <code class="perm-detail" :title="diffPreview.path">{{ diffPreview.path }}</code>
        <span class="diff-spacer" />
        <button class="perm-btn perm-allow" @click="respondPermission(true)" title="Accept (Y)">Accept <kbd class="perm-kbd">Y</kbd></button>
        <button class="perm-btn perm-always" @click="respondPermission(true, { always: true })" title="Always allow this tool">Always</button>
        <button class="perm-btn perm-deny" @click="respondPermission(false)" title="Reject (N)">Reject <kbd class="perm-kbd">N</kbd></button>
      </div>
      <pre v-if="diffPreview.isWrite" class="diff-banner-body"><span
        v-for="(line, i) in diffPreview.content.split('\n')" :key="i" class="diff-line diff-add">{{ line }}</span></pre>
      <pre v-else class="diff-banner-body"><span
        v-for="(line, i) in diffPreview.oldStr.split('\n')" :key="'o'+i" class="diff-line diff-del">{{ line }}</span><span
        v-for="(line, i) in diffPreview.newStr.split('\n')" :key="'n'+i" class="diff-line diff-add">{{ line }}</span></pre>
    </div>

    <!-- ExitPlanMode: plan approval -->
    <div v-if="pendingPlan" class="plan-banner">
      <div class="plan-head">
        <PhListChecks :size="14" class="perm-icon" />
        <span class="perm-title">Claude proposed a plan</span>
      </div>
      <!-- eslint-disable-next-line vue/no-v-html -->
      <div class="plan-body md-body" v-html="planMd" />
      <textarea
        v-model="planFeedback"
        class="plan-feedback"
        rows="1"
        placeholder="Optional feedback if you keep planning…"
      />
      <div class="plan-actions">
        <button class="perm-btn perm-allow" @click="respondPlan(true)">Approve plan</button>
        <button class="perm-btn perm-deny" @click="respondPlan(false)" title="Keep planning (Esc)">Keep planning</button>
      </div>
    </div>

    <!-- AskUserQuestion: multi-choice -->
    <div v-if="pendingQuestion" class="question-banner">
      <div v-for="(q, qi) in questionSpecs" :key="qi" class="question-block">
        <div class="question-head">
          <span v-if="q.header" class="question-chip">{{ q.header }}</span>
          <span class="question-text">{{ q.question }}</span>
          <span v-if="q.multiSelect" class="question-multi">choose any</span>
        </div>
        <div class="question-options">
          <button
            v-for="(opt, oi) in q.options"
            :key="oi"
            class="question-opt"
            :class="{ picked: isPicked(q.question, opt.label) }"
            @click="toggleOption(q.question, opt.label, !!q.multiSelect)"
          >
            <span class="opt-label">{{ opt.label }}</span>
            <span v-if="opt.description" class="opt-desc">{{ opt.description }}</span>
          </button>
        </div>
      </div>
      <div class="question-actions">
        <button class="perm-btn perm-allow" :disabled="!canSubmitQuestion" @click="submitQuestion">Submit</button>
        <button class="perm-btn perm-deny" @click="cancelQuestion" title="Dismiss (Esc)">Skip</button>
      </div>
    </div>

    <div ref="scrollEl" class="chat-messages">
      <div v-if="messages.length === 0" class="chat-empty">
        <div class="chat-empty-avatar">
          <ClaudeIcon :size="28" />
        </div>
        <span class="chat-empty-title">How can I help you?</span>
        <span class="chat-empty-sub">Working in {{ cwdDisplay }}</span>
      </div>

      <div
        v-for="msg in messages"
        :key="msg.id"
        class="chat-message"
        :class="[`role-${msg.role}`, { partial: msg.partial }]"
      >
        <!-- User message -->
        <template v-if="msg.role === 'user'">
          <div class="user-msg-row">
            <div class="bubble bubble-user">
              <div v-if="msg.images && msg.images.length > 0" class="msg-images">
                <img
                  v-for="(img, i) in msg.images"
                  :key="i"
                  :src="img"
                  class="msg-img"
                  :alt="`Image ${i + 1}`"
                />
              </div>
              {{ msg.text }}
            </div>
            <div class="user-avatar">U</div>
          </div>
        </template>

        <!-- Tool call — compact pill, expandable -->
        <template v-else-if="msg.role === 'tool'">
          <div class="agent-msg-row">
            <div class="agent-avatar-spacer" />
            <div class="tool-pill" @click="msg.toolExpanded = !msg.toolExpanded">
              <PhCaretRight :size="10" class="tool-caret" :class="{ 'tool-caret-open': msg.toolExpanded }" />
              <PhWrench :size="11" class="tool-icon" />
              <span class="tool-name">{{ msg.text }}</span>
            </div>
          </div>
          <div v-if="msg.toolExpanded && msg.toolInput" class="agent-msg-row">
            <div class="agent-avatar-spacer" />
            <pre class="tool-args">{{ JSON.stringify(msg.toolInput, null, 2) }}</pre>
          </div>
        </template>

        <!-- Permission log -->
        <template v-else-if="msg.role === 'permission'">
          <div class="agent-msg-row">
            <div class="agent-avatar-spacer" />
            <div class="bubble bubble-permission" :class="msg.text.startsWith('✓') ? 'perm-granted' : 'perm-rejected'">
              <span class="perm-log-text">{{ msg.text }}</span>
            </div>
          </div>
        </template>

        <!-- Thinking -->
        <template v-else-if="msg.role === 'thinking'">
          <div class="agent-msg-row">
            <div class="agent-avatar-spacer" />
            <details class="bubble-thinking">
              <summary class="thinking-summary">Thinking…</summary>
              <pre class="thinking-body">{{ msg.text }}</pre>
            </details>
          </div>
        </template>

        <!-- Assistant message -->
        <template v-else>
          <div class="agent-msg-row">
            <div class="agent-avatar">
              <ClaudeIcon :size="14" />
            </div>
            <div class="assistant-content">
              <!-- eslint-disable-next-line vue/no-v-html -->
              <div class="md-body" v-html="renderMd(msg.text)" />
              <span v-if="msg.partial" class="partial-cursor" />
            </div>
          </div>
        </template>
      </div>

      <div v-if="busy && !hasPartialAssistant" class="chat-thinking">
        <div class="agent-avatar agent-avatar-sm">
          <ClaudeIcon :size="12" />
        </div>
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

    <!-- @-mention file suggestions dropdown -->
    <div v-if="atSuggestions.length > 0" class="cmd-suggestions">
      <div
        v-for="(p, i) in atSuggestions"
        :key="p"
        class="cmd-suggestion"
        :class="{ selected: i === atIdx }"
        @mousedown.prevent="applyAtSuggestion(p)"
      >
        <span class="cmd-name">@{{ p.slice(p.lastIndexOf('/') + 1) }}</span>
        <span class="cmd-desc">{{ p }}</span>
      </div>
    </div>

    <!-- Image previews above input -->
    <div v-if="pendingImages.length > 0" class="pending-images">
      <div v-for="(img, i) in pendingImages" :key="i" class="pending-img-wrap">
        <img :src="img" class="pending-img" :alt="`Image ${i + 1}`" />
        <button class="pending-img-remove" @click="pendingImages.splice(i, 1)" title="Remove">
          <PhX :size="9" weight="bold" />
        </button>
      </div>
    </div>

    <!-- New-style input bar -->
    <div v-if="!hideComposer" class="chat-input-wrap">
      <div class="chat-input-box" :class="{ 'input-queued': busy && inputText.trim() }">
        <textarea
          ref="inputEl"
          v-model="inputText"
          class="chat-input"
          :placeholder="busy ? 'Type next message — will send when Claude finishes…' : 'Ask your agent anything...'"
          rows="1"
          @keydown="onKeydown"
          @input="onInput"
          @paste="onPaste"
        />
        <div class="chat-input-toolbar">
          <!-- Left: share selection, model dropdown, perm mode -->
          <div class="toolbar-left">
            <img v-if="avatarSrc" :src="avatarSrc" class="toolbar-avatar" alt="Manager" />
            <button
              v-if="editorCtx.selection"
              class="toolbar-btn"
              :title="`Add selection: ${relPath(editorCtx.selection.path)}#L${editorCtx.selection.startLine}-L${editorCtx.selection.endLine}`"
              @click="shareSelection"
            >
              <PhTextAa :size="13" />
            </button>
            <!-- Model switcher -->
            <div class="model-dropdown">
              <button ref="modelBtnEl" class="toolbar-btn toolbar-btn-label" @click="toggleModelMenu">
                {{ selectedModelLabel }}
                <PhCaretDown :size="9" weight="bold" class="btn-caret" />
              </button>
              <Teleport to="body">
                <div
                  v-if="modelMenuOpen"
                  ref="modelMenuEl"
                  class="floating-menu"
                  :style="{ top: modelMenuPos.top + 'px', left: modelMenuPos.left + 'px' }"
                >
                  <button
                    v-for="m in CLAUDE_MODELS"
                    :key="m.id"
                    class="floating-menu-item"
                    :class="{ 'floating-menu-item-active': selectedModel === m.id }"
                    @click="selectModel(m.id)"
                  >
                    {{ m.label }}
                    <span class="model-id-hint">{{ m.id }}</span>
                  </button>
                </div>
              </Teleport>
            </div>
            <!-- Profile switcher (only shown when more than one profile exists) -->
            <div v-if="profilesStore.profiles.length > 1" class="model-dropdown">
              <button
                ref="profileBtnEl"
                class="toolbar-btn toolbar-btn-label"
                :class="{ 'btn-active': selectedProfileId !== DEFAULT_PROFILE_ID }"
                :title="selectedProfile?.configDir ? `CLAUDE_CONFIG_DIR: ${selectedProfile.configDir}` : 'Claude profile'"
                @click="toggleProfileMenu"
              >
                <PhUserGear :size="12" />
                {{ selectedProfile?.name ?? 'Default' }}
                <PhCaretDown :size="9" weight="bold" class="btn-caret" />
              </button>
              <Teleport to="body">
                <div
                  v-if="profileMenuOpen"
                  ref="profileMenuEl"
                  class="floating-menu"
                  :style="{ bottom: profileMenuPos.bottom + 'px', left: profileMenuPos.left + 'px' }"
                >
                  <button
                    v-for="p in profilesStore.profiles"
                    :key="p.id"
                    class="floating-menu-item"
                    :class="{ 'floating-menu-item-active': selectedProfileId === p.id }"
                    @click="selectProfile(p.id)"
                  >
                    {{ p.name }}
                    <span v-if="p.configDir" class="model-id-hint">{{ p.configDir }}</span>
                  </button>
                </div>
              </Teleport>
            </div>
          </div>

          <!-- Right: abort/send -->
          <div class="toolbar-right">
            <button v-if="busy" class="send-btn send-btn-abort" title="Abort" @click="abortTurn">
              <PhStop :size="14" weight="bold" />
            </button>
            <button
              v-else-if="messageQueue.length > 0"
              class="send-btn"
              disabled
              :title="`${messageQueue.length} message${messageQueue.length > 1 ? 's' : ''} queued`"
            >
              {{ messageQueue.length }}
            </button>
            <button v-else class="send-btn" :disabled="!inputText.trim()" @click="sendMessage()">
              <PhArrowUp :size="14" weight="bold" />
            </button>
          </div>
        </div>
      </div>

      <!-- Status line below input -->
      <div class="status-line" style="position:relative;z-index:1;">
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
        <span v-if="messageQueue.length > 0" class="status-item status-queued">
          {{ messageQueue.length }} queued
        </span>
        <span v-if="busy" class="status-item status-busy">thinking…</span>
      </div>
    </div>
    </div><!-- end .chat-main -->

    <!-- Changes panel -->
    <div v-if="changesVisible && !compact" class="chat-changes">
      <div class="chg-header">
        <PhGitDiff :size="12" class="chg-header-icon" />
        <span>Changes</span>
        <span v-if="changedFiles.length" class="chg-count">{{ changedFiles.length }}</span>
        <button class="chg-refresh-btn" title="Refresh" @click="refreshChanges">
          <PhArrowsClockwise :size="11" />
        </button>
      </div>
      <div class="chg-body">
        <div v-if="changedFiles.length === 0" class="chg-empty">No changes yet</div>
        <template v-for="f in changedFiles" :key="f.path">
          <div
            class="chg-file"
            :class="{ 'chg-file-open': diffFile === f.path }"
            @click="toggleFileDiff(f.path)"
          >
            <span class="chg-stats">
              <span class="chg-add">+{{ f.added }}</span>
              <span class="chg-del">-{{ f.deleted }}</span>
            </span>
            <span class="chg-path" :title="f.path">{{ f.shortPath }}</span>
            <span class="chg-status" :class="`chg-status-${f.status}`">{{ f.status }}</span>
          </div>
          <pre v-if="diffFile === f.path && fileDiff" class="chg-diff"><span
            v-for="(line, i) in fileDiff.split('\n')"
            :key="i"
            class="diff-line"
            :class="diffLineClass(line)"
          >{{ line }}</span></pre>
        </template>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, nextTick, onMounted, onBeforeUnmount, watch } from "vue";
import { PhArrowUp, PhArrowCounterClockwise, PhWrench, PhStop, PhShieldWarning, PhShieldCheck, PhPencilSimple, PhGitDiff, PhArrowsClockwise, PhListChecks, PhTextAa, PhCaretDown, PhCaretRight, PhX, PhUserGear } from "@phosphor-icons/vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import ClaudeIcon from "@/components/icons/ClaudeIcon.vue";
import { useClaudeChatsStore } from "@/stores/claudeChats";
import { useProfilesStore, DEFAULT_PROFILE_ID } from "@/stores/profiles";
import { useNotificationsStore } from "@/stores/notifications";
import type { TermStatus } from "@/lib/terminalStatus";
import { useEditorContextStore } from "@/stores/editorContext";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { marked } from "marked";
import DOMPurify from "dompurify";

function renderMd(text: string): string {
  return DOMPurify.sanitize(marked.parse(text) as string);
}

const props = defineProps<{
  chatId: number;
  workspaceId: number;
  cwd: string;
  // Compact mode (float chat): hide the heavy chrome (changes panel + diff
  // sidebar), keep the message stream + input + inline permission gates.
  compact?: boolean;
  // Mission-control primer passed to claude_start as --append-system-prompt.
  appendSystemPrompt?: string;
  // Hide the built-in text composer — the host (e.g. the Manager bar) drives
  // sends from its own external input via the exposed sendMessage(). Permission
  // / plan / question gates stay visible.
  hideComposer?: boolean;
  // Optional avatar shown at the start of the composer's bottom toolbar row
  // (used by the Manager bar to give the agent a face).
  avatarSrc?: string;
  // Use a dedicated localStorage key for the model selection instead of the
  // shared global one, so this chat's model is independent of every other chat
  // (the Manager keeps its own model). Falls back to the global key.
  modelKey?: string;
  // Initial model when nothing is stored under modelKey yet.
  defaultModel?: string;
}>();

const chats = useClaudeChatsStore();
const notifStore = useNotificationsStore();
const editorCtx = useEditorContextStore();

// Relative-to-cwd path for a shared selection's @-reference.
function relPath(abs: string): string {
  if (props.cwd && abs.startsWith(props.cwd + "/")) return abs.slice(props.cwd.length + 1);
  return abs.split("/").pop() ?? abs;
}

// Insert the current editor selection as a fenced context block + @file#range header.
function shareSelection() {
  const sel = editorCtx.selection;
  if (!sel) return;
  const ref = `@${relPath(sel.path)}#L${sel.startLine}-L${sel.endLine}`;
  const block = `${ref}\n\`\`\`\n${sel.text}\n\`\`\`\n`;
  inputText.value = inputText.value ? `${inputText.value}\n${block}` : block;
  nextTick(() => { inputEl.value?.focus(); autoResize(); });
}

// Profile switcher
const profilesStore = useProfilesStore();
const PROFILE_KEY = (id: number) => `burrow.claude.profileId.${id}`;
function loadProfileId(id: number): string {
  return localStorage.getItem(PROFILE_KEY(id)) ?? DEFAULT_PROFILE_ID;
}
const selectedProfileId = ref<string>(loadProfileId(props.chatId));
const selectedProfile = computed(() => profilesStore.get(selectedProfileId.value));
const profileMenuOpen = ref(false);
const profileBtnEl = ref<HTMLElement | null>(null);
const profileMenuEl = ref<HTMLElement | null>(null);
const profileMenuPos = ref({ bottom: 0, left: 0 });
function toggleProfileMenu() {
  if (!profileMenuOpen.value && profileBtnEl.value) {
    const r = profileBtnEl.value.getBoundingClientRect();
    profileMenuPos.value = { bottom: Math.round(window.innerHeight - r.top + 4), left: Math.round(r.left) };
  }
  profileMenuOpen.value = !profileMenuOpen.value;
}
function onProfileMenuOutside(e: MouseEvent) {
  if (!profileMenuOpen.value) return;
  const t = e.target as Node;
  if (profileBtnEl.value?.contains(t) || profileMenuEl.value?.contains(t)) return;
  profileMenuOpen.value = false;
}
async function selectProfile(id: string) {
  profileMenuOpen.value = false;
  if (id === selectedProfileId.value) return;
  selectedProfileId.value = id;
  localStorage.setItem(PROFILE_KEY(props.chatId), id);
  suppressNextDone.value = true;
  await invoke("claude_stop", { id: props.chatId }).catch(() => {});
  const p = profilesStore.get(id);
  await invoke("claude_start", {
    id: props.chatId,
    cwd: props.cwd,
    resumeSessionId: sessionId.value || null,
    permissionMode: permMode.value,
    appendSystemPrompt: props.appendSystemPrompt || null,
    model: selectedModel.value,
    configDir: p?.configDir || null,
    profileCommand: p?.command || null,
    profileArgs: p?.args || null,
  }).catch(() => {});
}

// Model switcher
const CLAUDE_MODELS = [
  { id: "claude-sonnet-4-6", label: "Sonnet 4.6" },
  { id: "claude-opus-4-8", label: "Opus 4.8" },
  { id: "claude-haiku-4-5-20251001", label: "Haiku 4.5" },
] as const;
type ClaudeModelId = typeof CLAUDE_MODELS[number]["id"];
const MODEL_KEY = props.modelKey ?? "burrow.claude.model";
function loadModel(): ClaudeModelId {
  const v = localStorage.getItem(MODEL_KEY);
  if (CLAUDE_MODELS.some((m) => m.id === v)) return v as ClaudeModelId;
  if (props.defaultModel && CLAUDE_MODELS.some((m) => m.id === props.defaultModel)) {
    return props.defaultModel as ClaudeModelId;
  }
  return "claude-sonnet-4-6";
}
const selectedModel = ref<ClaudeModelId>(loadModel());
const modelMenuOpen = ref(false);
const modelBtnEl = ref<HTMLElement | null>(null);
const modelMenuEl = ref<HTMLElement | null>(null);
const modelMenuPos = ref({ top: 0, left: 0 });
function toggleModelMenu() {
  if (!modelMenuOpen.value && modelBtnEl.value) {
    const r = modelBtnEl.value.getBoundingClientRect();
    const menuH = CLAUDE_MODELS.length * 36 + 12;
    modelMenuPos.value = { top: Math.round(r.top - menuH - 6), left: Math.round(r.left) };
  }
  modelMenuOpen.value = !modelMenuOpen.value;
}
function onModelMenuOutside(e: MouseEvent) {
  if (!modelMenuOpen.value) return;
  const t = e.target as Node;
  if (modelBtnEl.value?.contains(t) || modelMenuEl.value?.contains(t)) return;
  modelMenuOpen.value = false;
}
async function selectModel(id: ClaudeModelId) {
  modelMenuOpen.value = false;
  if (id === selectedModel.value) return;
  selectedModel.value = id;
  localStorage.setItem(MODEL_KEY, id);
  suppressNextDone.value = true;
  await invoke("claude_stop", { id: props.chatId }).catch(() => {});
  await invoke("claude_start", {
    id: props.chatId,
    cwd: props.cwd,
    resumeSessionId: sessionId.value || null,
    permissionMode: permMode.value,
    appendSystemPrompt: props.appendSystemPrompt || null,
    model: selectedModel.value,
    configDir: selectedProfile.value?.configDir || null,
    profileCommand: selectedProfile.value?.command || null,
    profileArgs: selectedProfile.value?.args || null,
  }).catch(() => {});
}
const selectedModelLabel = computed(() => CLAUDE_MODELS.find((m) => m.id === selectedModel.value)?.label ?? selectedModel.value);

interface ChatMessage {
  id: number;
  role: "user" | "assistant" | "tool" | "thinking" | "permission";
  text: string;
  images?: string[]; // data URIs for user messages with attached images
  partial?: boolean;
  toolInput?: Record<string, unknown>; // full tool args for expandable tool calls
  toolExpanded?: boolean;
}

// Built-in claude slash commands
interface Command { name: string; description: string }

// Only commands that work in stream-json mode (no TTY display, no editor).
const BUILTIN_COMMANDS: Command[] = [
  { name: "pr",      description: "Write a PR description from recent git diff" },
  { name: "clear",   description: "Clear conversation history" },
  { name: "compact", description: "Compact conversation with summary" },
  { name: "help",    description: "Show available commands" },
  { name: "review",  description: "Review changes in current directory" },
  { name: "init",    description: "Initialize project with CLAUDE.md" },
  { name: "cost",    description: "Show token and cost usage for this session" },
];

const allCommands = ref<Command[]>([...BUILTIN_COMMANDS]);
const suggestions = ref<Command[]>([]);
const suggestionIdx = ref(0);

// @-mention file completion — lazy repo file list (git ls-files), filtered on `@query`.
const fileList = ref<string[]>([]);
let fileListLoaded = false;
const atSuggestions = ref<string[]>([]);
const atIdx = ref(0);

async function ensureFileList() {
  if (fileListLoaded) return;
  fileListLoaded = true;
  try {
    const out = await invoke<{ stdout: string }>("run_git", {
      cwd: props.cwd,
      args: ["ls-files", "--cached", "--others", "--exclude-standard"],
    });
    fileList.value = out.stdout.split("\n").map((s) => s.trim()).filter(Boolean).slice(0, 20000);
  } catch { fileList.value = []; }
}

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
const messageQueue = ref<string[]>([]);
// Set before an INTENTIONAL claude restart (mode switch / abort) so the `exit`
// event that teardown emits doesn't fire a spurious "Claude finished" toast.
const suppressNextDone = ref(false);
const pendingImages = ref<string[]>([]); // data URIs
const sessionId = ref("");
const turnStats = ref<TurnStats | null>(null);
const sessionCost = ref(0);
const scrollEl = ref<HTMLElement | null>(null);
const inputEl = ref<HTMLTextAreaElement | null>(null);
const suggestionsEl = ref<HTMLElement | null>(null);
let unlisten: UnlistenFn | null = null;

// Dangerous mode (bypass all permissions) — persisted per chatId
// Permission mode (per-chat, persisted). Mirrors the VS Code extension's mode picker.
type PermMode = "default" | "acceptEdits" | "bypassPermissions";
const PERM_KEY = (id: number) => `burrow.claude.permMode.${id}`;
const PERM_LAST_KEY = "burrow.claude.permMode.last";
function loadPermMode(id: number): PermMode {
  const v = localStorage.getItem(PERM_KEY(id));
  if (v === "acceptEdits" || v === "bypassPermissions" || v === "default") return v;
  // Migrate the old boolean "dangerous mode" flag → bypassPermissions.
  if (localStorage.getItem(`burrow.claude.dangerous.${id}`) === "1") return "bypassPermissions";
  // New chat: inherit the last-used mode so the user doesn't have to re-pick every time.
  const last = localStorage.getItem(PERM_LAST_KEY);
  if (last === "acceptEdits" || last === "bypassPermissions" || last === "default") return last;
  return "default";
}
const permMode = ref<PermMode>(loadPermMode(props.chatId));
const PERM_META: Record<PermMode, { label: string; title: string; danger?: boolean }> = {
  default: { label: "Ask", title: "Ask before edits & commands (click to change)" },
  acceptEdits: { label: "Auto-edit", title: "Auto-accept file edits; still ask for other tools (click to change)" },
  bypassPermissions: { label: "Bypass", title: "Skip ALL permission checks (click to change)", danger: true },
};
const permMeta = computed(() => PERM_META[permMode.value]);
const PERM_MODES: PermMode[] = ["default", "acceptEdits", "bypassPermissions"];
const permMenuOpen = ref(false);
const permBtnEl = ref<HTMLElement | null>(null);
const permMenuEl = ref<HTMLElement | null>(null);
// The menu is teleported + position:fixed, so anchor it to the button's rect.
const permMenuPos = ref({ top: 0, right: 0 });
function togglePermMenu() {
  if (!permMenuOpen.value && permBtnEl.value) {
    const r = permBtnEl.value.getBoundingClientRect();
    permMenuPos.value = { top: Math.round(r.bottom + 4), right: Math.round(window.innerWidth - r.right) };
  }
  permMenuOpen.value = !permMenuOpen.value;
}
function onPermMenuOutside(e: MouseEvent) {
  if (!permMenuOpen.value) return;
  const t = e.target as Node;
  if (permBtnEl.value?.contains(t) || permMenuEl.value?.contains(t)) return;
  permMenuOpen.value = false;
}

// ── Changes panel ────────────────────────────────────────────────────────────
interface ChangedFile { path: string; shortPath: string; added: number; deleted: number; status: string }
const changesVisible = ref(false);
const changedFiles = ref<ChangedFile[]>([]);
const diffFile = ref<string | null>(null);
const fileDiff = ref("");

interface GitOut { stdout: string; stderr: string; code: number }

async function refreshChanges() {
  if (!props.cwd) return;
  try {
    const [numstat, statusOut] = await Promise.all([
      invoke<GitOut>("run_git", { cwd: props.cwd, args: ["diff", "--numstat", "HEAD"] }),
      invoke<GitOut>("run_git", { cwd: props.cwd, args: ["status", "--porcelain"] }),
    ]);
    const files = new Map<string, ChangedFile>();
    // Parse numstat: "<added>\t<deleted>\t<path>"
    for (const line of numstat.stdout.split("\n")) {
      const m = line.match(/^(\d+|-)\t(\d+|-)\t(.+)$/);
      if (!m) continue;
      const path = m[3].trim();
      files.set(path, {
        path,
        shortPath: path.split("/").pop() ?? path,
        added: parseInt(m[1]) || 0,
        deleted: parseInt(m[2]) || 0,
        status: "M",
      });
    }
    // Layer in status codes (A=added, D=deleted, ?)
    for (const line of statusOut.stdout.split("\n")) {
      if (line.length < 3) continue;
      const xy = line.slice(0, 2).trim();
      const rawPath = line.slice(3).trim();
      const path = rawPath.includes(" -> ") ? rawPath.split(" -> ")[1] : rawPath;
      if (!files.has(path)) {
        files.set(path, { path, shortPath: path.split("/").pop() ?? path, added: 0, deleted: 0, status: xy || "?" });
      } else {
        files.get(path)!.status = xy || "M";
      }
    }
    changedFiles.value = [...files.values()];
    // Auto-show panel when changes appear
    if (files.size > 0 && !changesVisible.value) changesVisible.value = true;
    // Refresh open diff if its file is still changed
    if (diffFile.value && !files.has(diffFile.value)) { diffFile.value = null; fileDiff.value = ""; }
  } catch { /* git not available or not a repo */ }
}

async function toggleFileDiff(path: string) {
  if (diffFile.value === path) { diffFile.value = null; fileDiff.value = ""; return; }
  diffFile.value = path;
  fileDiff.value = "";
  try {
    const out = await invoke<GitOut>("run_git", { cwd: props.cwd, args: ["diff", "HEAD", "--", path] });
    fileDiff.value = out.stdout || "(no diff — file may be untracked or binary)";
  } catch { fileDiff.value = ""; }
}

async function notifyDone() {
  const session = chats.sessions.find((s) => s.id === props.chatId);
  const body = session?.title || "Claude finished";
  notifStore.push({ type: "done", title: "Claude", body, workspaceId: props.workspaceId });
  if (!document.hasFocus()) {
    let granted = await isPermissionGranted();
    if (!granted) { const p = await requestPermission(); granted = p === "granted"; }
    if (granted) sendNotification({ title: "Burrow", body: `✓ ${body}` });
  }
}

// Alert the user that Claude is blocked on a permission/question/plan decision:
// in-app toast always, plus a native OS notification (with sound) when Burrow is
// not focused — mirrors notifyDone's unfocused path.
async function notifyPermission(cr: CanUseToolReq) {
  const target = (cr.input?.command ?? cr.input?.file_path ?? cr.input?.path ?? cr.description ?? "") as string;
  const body = target ? `${cr.toolName}: ${String(target).slice(0, 80)}` : cr.toolName;
  notifStore.push({ type: "info", title: "Povolení", body, workspaceId: props.workspaceId });
  if (!document.hasFocus()) {
    let granted = await isPermissionGranted();
    if (!granted) { const p = await requestPermission(); granted = p === "granted"; }
    if (granted) sendNotification({ title: "Burrow — povolení", body });
  }
}

function diffLineClass(line: string) {
  if (line.startsWith("+") && !line.startsWith("+++")) return "diff-add";
  if (line.startsWith("-") && !line.startsWith("---")) return "diff-del";
  if (line.startsWith("@@")) return "diff-hunk";
  return "diff-ctx";
}

// A `can_use_tool` control_request from claude. Every blocking surface (permission,
// ExitPlanMode, AskUserQuestion, file edits) arrives on this one channel; we route by toolName.
interface CanUseToolReq {
  requestId: string;
  toolName: string;
  input: Record<string, unknown>;
  description?: string;
  suggestions: Array<Record<string, unknown>>;
  toolUseId?: string;
}
const pendingPermission = ref<CanUseToolReq | null>(null); // Bash / generic tool
const pendingQuestion = ref<CanUseToolReq | null>(null);   // AskUserQuestion
const pendingPlan = ref<CanUseToolReq | null>(null);       // ExitPlanMode
const pendingDiff = ref<CanUseToolReq | null>(null);       // Edit / Write / MultiEdit / NotebookEdit

// AskUserQuestion working selection: question text → chosen option label(s).
const questionAnswers = ref<Record<string, string[]>>({});
// ExitPlanMode "keep planning" feedback.
const planFeedback = ref("");

const permissionDetail = computed(() => {
  const cr = pendingPermission.value;
  if (!cr) return "";
  const r = cr.input;
  return (r.command ?? r.file_path ?? r.path ?? cr.description ?? JSON.stringify(r).slice(0, 120)) as string;
});

// Match keys for "Allow always" rules. Bash gets a command-prefix key so allowing
// `git` once doesn't blanket-allow every Bash call.
function ruleKeys(toolName: string, input: Record<string, unknown>): string[] {
  const keys = [toolName];
  if (toolName === "Bash" && typeof input.command === "string") {
    const first = (input.command as string).trim().split(/\s+/)[0];
    if (first) keys.push(`Bash:${first}`);
  }
  return keys;
}

const planMd = computed(() => {
  const p = pendingPlan.value?.input?.plan;
  return typeof p === "string" ? renderMd(p) : "";
});
interface QuestionSpec { question: string; header?: string; multiSelect?: boolean; options: Array<{ label: string; description?: string }> }
const questionSpecs = computed<QuestionSpec[]>(() =>
  ((pendingQuestion.value?.input?.questions ?? []) as QuestionSpec[]));
const canSubmitQuestion = computed(() =>
  questionSpecs.value.every((q) => (questionAnswers.value[q.question] ?? []).length > 0));

// Diff preview for a pending Edit/Write. For Write/NotebookEdit it's full content;
// for Edit it's old→new strings.
const diffPreview = computed(() => {
  const cr = pendingDiff.value;
  if (!cr) return null;
  const i = cr.input;
  return {
    path: (i.file_path ?? i.path ?? cr.description ?? "") as string,
    isWrite: cr.toolName === "Write" || cr.toolName === "NotebookEdit",
    content: (i.content ?? "") as string,
    oldStr: (i.old_string ?? "") as string,
    newStr: (i.new_string ?? "") as string,
  };
});
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
  messages.value.some((m) => (m.role === "assistant" || m.role === "thinking") && m.partial)
);

function scrollToBottom() {
  nextTick(() => {
    if (scrollEl.value) scrollEl.value.scrollTop = scrollEl.value.scrollHeight;
  });
}

// Auto-title helpers
const FILLER_PREFIX = /^(can you |please |i want (you )?to |how (do i|to) |what (is|are) (the |a )?|could you |would you |help me |i need (you )?to )/i;
function smartTitle(text: string): string {
  const clean = text.replace(FILLER_PREFIX, "").replace(/\s+/g, " ").trim();
  const words = clean.split(" ");
  const slug = words.slice(0, 6).join(" ");
  const title = slug.charAt(0).toUpperCase() + slug.slice(1);
  return title.length < clean.length ? title + "…" : title;
}
function isDefaultTitle(title: string): boolean {
  return /^Chat\s+\d+$/.test(title.trim());
}
// Once Claude sends us a generated title, prefer it and stop overwriting.
const claudeGeneratedTitle = ref(false);
function applyClaudeTitle(raw: unknown) {
  if (typeof raw !== "string" || !raw.trim()) return;
  claudeGeneratedTitle.value = true;
  chats.sync(props.chatId, { title: raw.trim().slice(0, 60) });
}

// Derive the Sidebar status dot from live chat state. Generic tool / file-edit
// requests are a hard allow/deny gate → "permission" (amber + bell); a question /
// plan prompt is a soft "waiting"; an in-flight turn is "running".
function chatStatus(): TermStatus {
  if (pendingPermission.value || pendingDiff.value) return "permission";
  if (pendingQuestion.value || pendingPlan.value) return "waiting";
  if (busy.value) return "running";
  return "idle";
}

function syncStore() {
  chats.sync(props.chatId, {
    busy: busy.value,
    messageCount: messages.value.filter((m) => m.role !== "tool").length,
    status: chatStatus(),
  });
}

function onLine(line: string) {
  let event: Record<string, unknown>;
  try { event = JSON.parse(line) as Record<string, unknown>; }
  catch { return; }

  const type = event.type as string;

  if (type === "control_request") {
    const req = (event.request ?? {}) as Record<string, unknown>;
    if (req.subtype !== "can_use_tool") return; // other control subtypes: ignore (fail-open)
    const cr: CanUseToolReq = {
      requestId: event.request_id as string,
      toolName: (req.tool_name as string) ?? "",
      input: (req.input ?? {}) as Record<string, unknown>,
      description: req.description as string | undefined,
      suggestions: (req.permission_suggestions ?? []) as Array<Record<string, unknown>>,
      toolUseId: req.tool_use_id as string | undefined,
    };
    // Auto-allow when an "always" rule matches — no UI.
    if (chats.hasPermissionRule(ruleKeys(cr.toolName, cr.input))) {
      respondControl(cr.requestId, { behavior: "allow", updatedInput: cr.input });
      return;
    }
    if (cr.toolName === "AskUserQuestion") {
      questionAnswers.value = {};
      pendingQuestion.value = cr;
    } else if (cr.toolName === "ExitPlanMode") {
      planFeedback.value = "";
      pendingPlan.value = cr;
    } else if (["Edit", "Write", "MultiEdit", "NotebookEdit"].includes(cr.toolName)) {
      pendingDiff.value = cr;
    } else {
      pendingPermission.value = cr;
    }
    notifyPermission(cr);
    syncStore(); // surface the permission/waiting dot in the Sidebar
    scrollToBottom();
    return;
  }

  if (type === "system") {
    const sub = event.subtype as string;
    if (sub === "init") {
      const sid = (event.session_id as string) ?? "";
      sessionId.value = sid;
      chats.sync(props.chatId, { claudeSessionId: sid });
    }
    if (sub === "session_title") applyClaudeTitle(event.title);
    if (sub === "hook_started" || sub === "hook_response") return;
  }

  if (type === "assistant") {
    const content = ((event.message as Record<string, unknown>)?.content ?? []) as Array<Record<string, unknown>>;
    const textParts = content.filter((b) => b.type === "text").map((b) => b.text as string).join("");
    const thinkingParts = content.filter((b) => b.type === "thinking").map((b) => b.thinking as string).join("");
    const toolBlocks = content.filter((b) => b.type === "tool_use");

    if (thinkingParts) {
      const last = messages.value[messages.value.length - 1];
      if (last?.role === "thinking" && last.partial) {
        last.text += thinkingParts;
      } else {
        messages.value.push({ id: nextMsgId++, role: "thinking", text: thinkingParts, partial: true });
      }
    }
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
      const toolInput = (tb.input ?? {}) as Record<string, unknown>;
      messages.value.push({ id: nextMsgId++, role: "tool", text: name, toolInput, toolExpanded: false });
    }
    scrollToBottom();
    return;
  }

  if (type === "result" || type === "exit") {
    busy.value = false;
    // Un-partial ALL messages — tool messages are pushed after assistant text,
    // so checking only `last` would leave the assistant text bubble still partial.
    for (const m of messages.value) { if (m.partial) m.partial = false; }
    // Capture usage/cost from result event
    if (type === "result") {
      const usage = event.usage as Record<string, number> | undefined;
      const cost = (event.cost_usd as number) ?? 0;
      if (usage) {
        const inp = usage.input_tokens ?? 0;
        const out = usage.output_tokens ?? 0;
        turnStats.value = { inputTokens: inp, outputTokens: out, costUsd: cost };
        sessionCost.value += cost;
        chats.recordTurn(inp, out);
      }
      // Claude Code ≥1.x emits session_title in the result event after generating one
      if (!claudeGeneratedTitle.value) applyClaudeTitle(event.session_title);
    }
    saveMessages(props.chatId, messages.value);
    syncStore();
    scrollToBottom();
    refreshChanges();
    // An `exit` from an intentional restart (mode switch / abort) is not a real
    // turn boundary — skip the "finished" toast/notification once.
    if (type === "exit" && suppressNextDone.value) {
      suppressNextDone.value = false;
    } else {
      notifyDone();
    }
    // Flush one queued message (next turn will flush the next one).
    if (messageQueue.value.length > 0) {
      const next = messageQueue.value.shift()!;
      nextTick(() => sendMessage(next));
    }
    return;
  }
}

async function sendMessage(forcedText?: string) {
  let text = (forcedText ?? inputText.value).trim();
  if (!text) return;
  // While busy: queue the message instead of sending immediately.
  if (busy.value && !forcedText) {
    messageQueue.value.push(text);
    inputText.value = "";
    await nextTick();
    autoResize();
    return;
  }
  if (!forcedText) {
    inputText.value = "";
    await nextTick();
    autoResize();
  }

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

  const msgImages = pendingImages.value.length > 0 ? [...pendingImages.value] : undefined;
  messages.value.push({ id: nextMsgId++, role: "user", text, images: msgImages });
  busy.value = true;

  // Auto-title from first user message (only if still at default and Claude hasn't set one yet)
  if (!claudeGeneratedTitle.value) {
    const session = chats.sessions.find((s) => s.id === props.chatId);
    if (session && isDefaultTitle(session.title)) {
      chats.sync(props.chatId, { title: smartTitle(text) });
    }
  }

  saveMessages(props.chatId, messages.value);
  syncStore();
  scrollToBottom();
  try {
    const images = pendingImages.value.length > 0 ? [...pendingImages.value] : undefined;
    pendingImages.value = [];
    await invoke("claude_send", { id: props.chatId, text, sessionId: sessionId.value || null, images });
  } catch (e) {
    messages.value.push({ id: nextMsgId++, role: "assistant", text: `Error: ${e}` });
    busy.value = false;
    syncStore();
  }
}

// Reply to a can_use_tool control_request. `response` is the inner decision object
// ({behavior:"allow",updatedInput} | {behavior:"deny",message}); the Rust side wraps it.
async function respondControl(requestId: string, response: Record<string, unknown>) {
  try {
    await invoke("claude_respond_control", { id: props.chatId, requestId, response });
  } catch (e) {
    messages.value.push({ id: nextMsgId++, role: "assistant", text: `Control response failed: ${e}` });
    saveMessages(props.chatId, messages.value);
  }
  // Callers clear the pending ref before calling us, so this re-derives the dot
  // (permission/waiting → running/idle) once the gate is answered.
  syncStore();
}

// Generic tool permission + diff Accept/Reject (both pull from pendingPermission|pendingDiff).
function respondPermission(allow: boolean, opts?: { always?: boolean; updatedInput?: Record<string, unknown>; message?: string }) {
  const cr = pendingPermission.value ?? pendingDiff.value;
  if (!cr) return;
  pendingPermission.value = null;
  pendingDiff.value = null;
  const detail = (cr.input.command ?? cr.input.file_path ?? cr.input.path ?? cr.description ?? "") as string;
  const detailStr = detail ? ` — ${detail.length > 80 ? detail.slice(0, 80) + "…" : detail}` : "";
  if (allow) {
    if (opts?.always) {
      const keys = ruleKeys(cr.toolName, cr.input);
      chats.addPermissionRule(keys[keys.length - 1]);
    }
    const label = opts?.always ? "✓ Always allowed" : "✓ Allowed";
    messages.value.push({ id: nextMsgId++, role: "permission", text: `${label}: ${cr.toolName}${detailStr}` });
    saveMessages(props.chatId, messages.value);
    respondControl(cr.requestId, { behavior: "allow", updatedInput: opts?.updatedInput ?? cr.input });
  } else {
    messages.value.push({ id: nextMsgId++, role: "permission", text: `✗ Denied: ${cr.toolName}${detailStr}` });
    saveMessages(props.chatId, messages.value);
    respondControl(cr.requestId, { behavior: "deny", message: opts?.message || "User denied this action." });
  }
}

function toggleOption(question: string, label: string, multi: boolean) {
  const cur = questionAnswers.value[question] ?? [];
  if (multi) {
    questionAnswers.value[question] = cur.includes(label) ? cur.filter((l) => l !== label) : [...cur, label];
  } else {
    questionAnswers.value[question] = cur.includes(label) ? [] : [label];
  }
}
function isPicked(question: string, label: string) {
  return (questionAnswers.value[question] ?? []).includes(label);
}

function submitQuestion() {
  const cr = pendingQuestion.value;
  if (!cr || !canSubmitQuestion.value) return;
  pendingQuestion.value = null;
  // The tool reads input.answers keyed by question text; multi-select joins with ", ".
  const answers: Record<string, string> = {};
  for (const [q, labels] of Object.entries(questionAnswers.value)) {
    if (labels.length) answers[q] = labels.join(", ");
  }
  respondControl(cr.requestId, { behavior: "allow", updatedInput: { ...cr.input, answers } });
}
function cancelQuestion() {
  const cr = pendingQuestion.value;
  if (!cr) return;
  pendingQuestion.value = null;
  // allow with empty answers → tool reports "did not answer" (clean dismiss, no error).
  respondControl(cr.requestId, { behavior: "allow", updatedInput: { ...cr.input, answers: {} } });
}

function respondPlan(approve: boolean) {
  const cr = pendingPlan.value;
  if (!cr) return;
  pendingPlan.value = null;
  if (approve) {
    respondControl(cr.requestId, { behavior: "allow", updatedInput: cr.input });
  } else {
    respondControl(cr.requestId, { behavior: "deny", message: planFeedback.value.trim() || "Keep planning — do not exit plan mode yet." });
  }
}

// Pick a permission mode from the header dropdown (default / acceptEdits / bypassPermissions).
// Restart claude with --resume so the conversation continues under the new mode.
async function selectPermMode(mode: PermMode) {
  permMenuOpen.value = false;
  if (mode === permMode.value) return;
  permMode.value = mode;
  localStorage.setItem(PERM_KEY(props.chatId), permMode.value);
  localStorage.setItem(PERM_LAST_KEY, permMode.value);
  suppressNextDone.value = true; // restart below — don't toast on the teardown `exit`
  await invoke("claude_stop", { id: props.chatId }).catch(() => {});
  await invoke("claude_start", {
    id: props.chatId,
    cwd: props.cwd,
    resumeSessionId: sessionId.value || null,
    permissionMode: permMode.value,
    appendSystemPrompt: props.appendSystemPrompt || null,
    model: selectedModel.value,
    configDir: selectedProfile.value?.configDir || null,
    profileCommand: selectedProfile.value?.command || null,
    profileArgs: selectedProfile.value?.args || null,
  }).catch(() => {});
}

async function abortTurn() {
  suppressNextDone.value = true; // abort + restart — don't toast on the teardown `exit`
  await invoke("claude_abort", { id: props.chatId }).catch(() => {});
  // Restart with --resume so session continues
  await invoke("claude_start", {
    id: props.chatId,
    cwd: props.cwd,
    resumeSessionId: sessionId.value || null,
    permissionMode: permMode.value,
    appendSystemPrompt: props.appendSystemPrompt || null,
    model: selectedModel.value,
    configDir: selectedProfile.value?.configDir || null,
    profileCommand: selectedProfile.value?.command || null,
    profileArgs: selectedProfile.value?.args || null,
  }).catch(() => {});
  busy.value = false;
  messageQueue.value = [];
  const last = messages.value[messages.value.length - 1];
  if (last?.partial) last.partial = false;
  syncStore();
}

async function clearChat() {
  await invoke("claude_stop", { id: props.chatId }).catch(() => {});
  messages.value = [];
  sessionId.value = "";
  busy.value = false;
  messageQueue.value = [];
  pendingImages.value = [];
  turnStats.value = null;
  sessionCost.value = 0;
  claudeGeneratedTitle.value = false;
  localStorage.removeItem(msgKey(props.chatId));
  chats.sync(props.chatId, { claudeSessionId: "", busy: false, messageCount: 0, title: `Chat` });
  await invoke("claude_start", {
    id: props.chatId,
    cwd: props.cwd,
    permissionMode: permMode.value,
    appendSystemPrompt: props.appendSystemPrompt || null,
    model: selectedModel.value,
    configDir: selectedProfile.value?.configDir || null,
    profileCommand: selectedProfile.value?.command || null,
    profileArgs: selectedProfile.value?.args || null,
  }).catch(() => {});
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

// ── @-mention: complete a file path from the repo file list ─────────────────
function atQueryBeforeCursor(): string | null {
  const el = inputEl.value;
  const pos = el?.selectionStart ?? inputText.value.length;
  const upto = inputText.value.slice(0, pos);
  const m = upto.match(/(?:^|\s)@([^\s@]*)$/);
  return m ? m[1] : null;
}

async function updateAtSuggestions() {
  const q = atQueryBeforeCursor();
  if (q === null) { atSuggestions.value = []; return; }
  await ensureFileList();
  if (atQueryBeforeCursor() !== q) return; // cursor moved while loading
  const ql = q.toLowerCase();
  atSuggestions.value = fileList.value
    .filter((p) => p.toLowerCase().includes(ql))
    .sort((a, b) => {
      const ab = a.slice(a.lastIndexOf("/") + 1).toLowerCase();
      const bb = b.slice(b.lastIndexOf("/") + 1).toLowerCase();
      return (Number(!ab.startsWith(ql)) - Number(!bb.startsWith(ql))) || a.length - b.length;
    })
    .slice(0, 8);
  atIdx.value = 0;
}

function applyAtSuggestion(path: string) {
  const el = inputEl.value;
  const pos = el?.selectionStart ?? inputText.value.length;
  const upto = inputText.value.slice(0, pos);
  const after = inputText.value.slice(pos);
  const m = upto.match(/@([^\s@]*)$/);
  if (!m) return;
  const base = upto.slice(0, upto.length - m[0].length);
  inputText.value = `${base}@${path} ${after}`;
  atSuggestions.value = [];
  nextTick(() => { inputEl.value?.focus(); autoResize(); });
}

function onKeydown(e: KeyboardEvent) {
  if (pendingPermission.value || pendingDiff.value) {
    if (e.key === "y" || e.key === "Y") { e.preventDefault(); respondPermission(true); return; }
    if (e.key === "n" || e.key === "N") { e.preventDefault(); respondPermission(false); return; }
  }
  if (pendingQuestion.value && e.key === "Escape") { e.preventDefault(); cancelQuestion(); return; }
  if (pendingPlan.value && e.key === "Escape") { e.preventDefault(); respondPlan(false); return; }
  if (atSuggestions.value.length > 0) {
    if (e.key === "ArrowDown") { e.preventDefault(); atIdx.value = Math.min(atIdx.value + 1, atSuggestions.value.length - 1); return; }
    if (e.key === "ArrowUp") { e.preventDefault(); atIdx.value = Math.max(atIdx.value - 1, 0); return; }
    if (e.key === "Tab" || (e.key === "Enter" && !e.shiftKey)) { e.preventDefault(); applyAtSuggestion(atSuggestions.value[atIdx.value]); return; }
    if (e.key === "Escape") { atSuggestions.value = []; return; }
  }
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
  updateAtSuggestions();
}

function onPaste(e: ClipboardEvent) {
  const items = e.clipboardData?.items;
  if (!items) return;
  for (const item of Array.from(items)) {
    if (item.type.startsWith("image/")) {
      e.preventDefault();
      const file = item.getAsFile();
      if (!file) continue;
      const reader = new FileReader();
      reader.onload = () => {
        if (typeof reader.result === "string") pendingImages.value.push(reader.result);
      };
      reader.readAsDataURL(file);
    }
  }
}

function autoResize() {
  const el = inputEl.value;
  if (!el) return;
  el.style.height = "auto";
  el.style.height = Math.min(el.scrollHeight, 160) + "px";
}

function onWindowKeydown(e: KeyboardEvent) {
  if (!pendingPermission.value && !pendingDiff.value) return;
  if (document.activeElement === inputEl.value) return; // handled by onKeydown
  if (e.key === "y" || e.key === "Y") { e.preventDefault(); respondPermission(true); }
  if (e.key === "n" || e.key === "N") { e.preventDefault(); respondPermission(false); }
}

onMounted(async () => {
  window.addEventListener("keydown", onWindowKeydown);
  window.addEventListener("mousedown", onPermMenuOutside);
  window.addEventListener("mousedown", onModelMenuOutside);
  window.addEventListener("mousedown", onProfileMenuOutside);
  // Float (compact) control chat: pre-allow `burrow` Bash commands so routine
  // control calls (focus/list/new-tab/spawn) don't prompt every time. User can
  // still tighten via the perm-mode switch / Deny.
  if (props.compact) chats.addPermissionRule("Bash:burrow");
  const stored = chats.sessions.find((s) => s.id === props.chatId)?.claudeSessionId ?? "";
  if (stored) sessionId.value = stored;
  await invoke("claude_start", {
    id: props.chatId,
    cwd: props.cwd,
    resumeSessionId: stored || null,
    permissionMode: permMode.value,
    appendSystemPrompt: props.appendSystemPrompt || null,
    model: selectedModel.value,
    configDir: selectedProfile.value?.configDir || null,
    profileCommand: selectedProfile.value?.command || null,
    profileArgs: selectedProfile.value?.args || null,
  }).catch(() => {});
  unlisten = await listen<string>(`claude-data-${props.chatId}`, (ev) => onLine(ev.payload));

  // Load account info (plan, 5h window) — non-blocking.
  invoke<AccountInfo>("claude_get_account", { cwd: props.cwd })
    .then((info) => { accountInfo.value = info; })
    .catch(() => {});

  refreshChanges();

  // Load installed skills and merge with built-ins. Skills override same-named built-ins.
  // Map-based dedup ensures no duplicates regardless of list_skills returning overlaps.
  try {
    const skills = await invoke<{ name: string; description: string; enabled: boolean }[]>("list_skills");
    const merged = new Map<string, Command>();
    for (const c of BUILTIN_COMMANDS) merged.set(c.name, c);
    for (const s of skills) {
      if (s.enabled) merged.set(s.name, { name: s.name, description: s.description || `/${s.name} skill` });
    }
    allCommands.value = [...merged.values()].sort((a, b) => a.name.localeCompare(b.name));
  } catch { /* browser-only dev without Tauri */ }
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onWindowKeydown);
  window.removeEventListener("mousedown", onPermMenuOutside);
  window.removeEventListener("mousedown", onModelMenuOutside);
  window.removeEventListener("mousedown", onProfileMenuOutside);
  unlisten?.();
  invoke("claude_stop", { id: props.chatId }).catch(() => {});
});

watch(() => props.chatId, () => nextTick(() => inputEl.value?.focus()));

// Scroll to bottom when this chat becomes the active one (user clicked it in sidebar).
watch(() => chats.activeByWs[props.workspaceId], (activeId) => {
  if (activeId === props.chatId) nextTick(() => scrollToBottom());
});

// Exposed for host shells (e.g. the Manager bar) that drive this chat from an
// external compact input: send a message and focus the textarea.
function focusInput() {
  nextTick(() => { inputEl.value?.focus(); autoResize(); });
}
defineExpose({ sendMessage, focusInput, selectModel, selectedModel });
</script>

<style scoped>
.claude-chat {
  display: flex;
  flex-direction: row;
  height: 100%;
  background: #0f0f11;
  overflow: hidden;
  --chat-bg: #0f0f11;
  --chat-surface: #18181c;
  --chat-border: rgba(255,255,255,0.08);
  --chat-accent: #7c3aed;
  --chat-accent-dim: #6d28d9;
  --chat-text: rgba(255,255,255,0.88);
  --chat-muted: rgba(255,255,255,0.42);
  --chat-user-bg: #1e1b2e;
  --chat-user-border: rgba(124,58,237,0.35);
}

.chat-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: #0f0f11;
}

/* Changes panel */
.chat-changes {
  width: 230px;
  flex-shrink: 0;
  border-left: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  background: var(--bg-panel);
  overflow: hidden;
}

.chg-header {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 7px 10px;
  border-bottom: 1px solid var(--border);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.07em;
  text-transform: uppercase;
  color: var(--text-muted);
  flex-shrink: 0;
}

.chg-header-icon { color: var(--accent); }

.chg-count {
  background: var(--bg-hover);
  border-radius: 8px;
  padding: 0 5px;
  font-size: 9px;
  font-weight: 700;
  color: var(--text-secondary);
  line-height: 1.6;
}

.chg-refresh-btn {
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  padding: 2px;
  border-radius: 3px;
  margin-left: auto;
}
.chg-refresh-btn:hover { color: var(--text-primary); background: var(--bg-hover); }

.chg-body {
  flex: 1;
  overflow-y: auto;
  padding: 4px 0;
}

.chg-empty {
  font-size: 11px;
  color: var(--text-muted);
  text-align: center;
  padding: 20px 12px;
}

.chg-file {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px;
  cursor: pointer;
  border-radius: 4px;
  margin: 1px 4px;
  transition: background .1s;
}
.chg-file:hover { background: var(--bg-hover); }
.chg-file.chg-file-open { background: color-mix(in srgb, var(--accent) 10%, transparent); }

.chg-stats {
  display: flex;
  gap: 3px;
  font-size: 9px;
  font-family: var(--font-mono);
  flex-shrink: 0;
}
.chg-add { color: var(--green); }
.chg-del { color: var(--red); }

.chg-path {
  flex: 1;
  min-width: 0;
  font-size: 11px;
  font-family: var(--font-mono);
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.chg-status {
  font-size: 9px;
  font-weight: 700;
  padding: 1px 4px;
  border-radius: 3px;
  flex-shrink: 0;
}
.chg-status-M { color: var(--yellow); }
.chg-status-A { color: var(--green); }
.chg-status-D { color: var(--red); }
.chg-status-\? { color: var(--text-muted); }

.chg-diff {
  margin: 0 4px 4px;
  padding: 6px 8px;
  background: var(--bg-base);
  border: 1px solid var(--border);
  border-radius: 5px;
  font-size: 9.5px;
  font-family: var(--font-mono);
  overflow-x: auto;
  white-space: pre;
  max-height: 320px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
}

.diff-line { line-height: 1.5; }
.diff-add { color: var(--green); }
.diff-del { color: var(--red); }
.diff-hunk { color: var(--accent); opacity: 0.8; }
.diff-ctx { color: var(--text-secondary); }

/* Toggle button badge */
.changes-badge {
  position: absolute;
  top: 1px;
  right: 1px;
  min-width: 12px;
  height: 12px;
  padding: 0 3px;
  background: var(--accent);
  color: #fff;
  font-size: 7px;
  font-weight: 700;
  border-radius: 6px;
  line-height: 12px;
  text-align: center;
  pointer-events: none;
}

.chat-header-btn { position: relative; }

.chat-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--chat-border);
  flex-shrink: 0;
  background: var(--chat-surface);
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

.chat-header-btn {
  background: none;
  border: none;
  color: rgba(255,255,255,0.45);
  cursor: pointer;
  display: flex;
  align-items: center;
  padding: 4px;
  border-radius: 5px;
  transition: color .12s, background .12s;
}
.chat-header-btn:hover { color: rgba(255,255,255,0.85); background: rgba(255,255,255,0.07); }
.btn-danger-active { color: #ef4444 !important; background: rgba(239,68,68,0.15) !important; }
.perm-mode-btn { width: auto !important; gap: 4px; padding: 0 7px; }
.perm-mode-label { font-size: 10px; font-weight: 600; }
.perm-mode-caret { opacity: .6; margin-left: -1px; }
.btn-active { color: #a78bfa !important; background: rgba(124,58,237,0.15) !important; }

/* Permission-mode dropdown */
.perm-mode-dropdown { position: relative; display: flex; }
.perm-mode-menu {
  position: fixed;
  z-index: 1000;
  min-width: 150px;
  padding: 4px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  background: #1e1e26;
  border: 1px solid rgba(255,255,255,0.1);
  border-radius: 8px;
  box-shadow: 0 10px 30px rgba(0,0,0,0.5);
}
.perm-mode-item {
  display: flex;
  align-items: center;
  gap: 7px;
  width: 100%;
  padding: 6px 8px;
  background: none;
  border: none;
  border-radius: 5px;
  color: rgba(255,255,255,0.8);
  font-size: 11px;
  font-weight: 500;
  text-align: left;
  cursor: pointer;
  transition: color .12s, background .12s;
}
.perm-mode-item:hover { background: rgba(255,255,255,0.06); }
.perm-mode-item-active { color: #a78bfa; background: rgba(124,58,237,0.12); }
.perm-mode-item-danger { color: #ef4444; }
.perm-mode-item-danger:hover { background: rgba(239,68,68,0.12); }
.perm-mode-item-danger.perm-mode-item-active { color: #ef4444; background: rgba(239,68,68,0.12); }

/* Permission banner */
.permission-banner {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 9px 12px;
  background: color-mix(in srgb, #f59e0b 10%, var(--bg-panel));
  border-bottom: 2px solid color-mix(in srgb, #f59e0b 50%, transparent);
  border-top: 1px solid color-mix(in srgb, #f59e0b 30%, transparent);
  flex-shrink: 0;
  animation: perm-slide-in 0.15s ease-out;
}
@keyframes perm-slide-in {
  from { opacity: 0; transform: translateY(-4px); }
  to   { opacity: 1; transform: translateY(0); }
}
.perm-icon { color: #f59e0b; flex-shrink: 0; }
.perm-body { flex: 1; display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.perm-title { font-size: 11px; font-weight: 600; color: var(--text-primary); }
.perm-detail {
  font-size: 10px;
  font-family: var(--font-mono);
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 100%;
}
.perm-btn {
  display: flex;
  align-items: center;
  gap: 5px;
  border: none;
  border-radius: 5px;
  font-size: 11px;
  font-weight: 600;
  font-family: var(--font-ui);
  padding: 5px 11px;
  cursor: pointer;
  flex-shrink: 0;
  transition: filter .1s;
}
.perm-btn:hover { filter: brightness(1.1); }
.perm-btn:active { filter: brightness(0.9); }
.perm-allow { background: #16a34a; color: #fff; }
.perm-always { background: color-mix(in srgb, #16a34a 22%, var(--bg-panel)); color: var(--text-primary); }
.perm-deny  { background: #b91c1c; color: #fff; }
.perm-btn:disabled { opacity: 0.4; cursor: default; filter: none; }
.perm-kbd {
  font-size: 9px;
  font-family: var(--font-mono);
  font-weight: 700;
  background: rgba(255,255,255,0.2);
  border-radius: 3px;
  padding: 1px 4px;
  line-height: 1.4;
}

/* ── File-edit diff banner ─────────────────────────────────────────────── */
.diff-banner {
  flex-shrink: 0;
  background: var(--bg-panel);
  border-top: 1px solid color-mix(in srgb, #6366f1 30%, transparent);
  border-bottom: 2px solid color-mix(in srgb, #6366f1 45%, transparent);
  animation: perm-slide-in 0.15s ease-out;
}
.diff-banner-head { display: flex; align-items: center; gap: 8px; padding: 8px 12px; }
.diff-banner-head .perm-icon { color: #818cf8; }
.diff-spacer { flex: 1; }
.diff-banner-body {
  margin: 0;
  max-height: 220px;
  overflow: auto;
  padding: 6px 12px 10px;
  font-family: var(--font-mono);
  font-size: 11px;
  line-height: 1.5;
}
.diff-banner-body .diff-line { display: block; white-space: pre-wrap; word-break: break-all; }

/* ── ExitPlanMode banner ───────────────────────────────────────────────── */
.plan-banner {
  flex-shrink: 0;
  padding: 10px 12px;
  background: color-mix(in srgb, #10b981 8%, var(--bg-panel));
  border-top: 1px solid color-mix(in srgb, #10b981 30%, transparent);
  border-bottom: 2px solid color-mix(in srgb, #10b981 45%, transparent);
  animation: perm-slide-in 0.15s ease-out;
}
.plan-head { display: flex; align-items: center; gap: 7px; margin-bottom: 6px; }
.plan-head .perm-icon { color: #10b981; }
.plan-body { max-height: 260px; overflow: auto; font-size: 12px; color: var(--text-primary); }
.plan-feedback {
  width: 100%;
  margin: 8px 0;
  resize: vertical;
  background: var(--bg-base);
  border: 1px solid var(--border-subtle, rgba(255,255,255,0.1));
  border-radius: 5px;
  color: var(--text-primary);
  font-family: var(--font-ui);
  font-size: 11px;
  padding: 6px 8px;
  box-sizing: border-box;
}
.plan-actions, .question-actions { display: flex; gap: 8px; justify-content: flex-end; }

/* ── AskUserQuestion banner ────────────────────────────────────────────── */
.question-banner {
  flex-shrink: 0;
  padding: 10px 12px;
  background: color-mix(in srgb, #3b82f6 8%, var(--bg-panel));
  border-top: 1px solid color-mix(in srgb, #3b82f6 30%, transparent);
  border-bottom: 2px solid color-mix(in srgb, #3b82f6 45%, transparent);
  animation: perm-slide-in 0.15s ease-out;
}
.question-block { margin-bottom: 10px; }
.question-head { display: flex; align-items: center; gap: 7px; margin-bottom: 6px; flex-wrap: wrap; }
.question-chip {
  font-size: 9px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.04em;
  background: color-mix(in srgb, #3b82f6 25%, transparent); color: #93c5fd;
  border-radius: 4px; padding: 2px 6px;
}
.question-text { font-size: 12px; font-weight: 600; color: var(--text-primary); }
.question-multi { font-size: 9px; color: var(--text-secondary); font-style: italic; }
.question-options { display: flex; flex-direction: column; gap: 5px; }
.question-opt {
  display: flex; flex-direction: column; gap: 1px; text-align: left;
  background: var(--bg-base);
  border: 1px solid var(--border-subtle, rgba(255,255,255,0.12));
  border-radius: 6px; padding: 7px 10px; cursor: pointer;
  transition: border-color .1s, background .1s;
}
.question-opt:hover { border-color: color-mix(in srgb, #3b82f6 55%, transparent); }
.question-opt.picked {
  border-color: #3b82f6;
  background: color-mix(in srgb, #3b82f6 16%, var(--bg-base));
}
.opt-label { font-size: 12px; font-weight: 600; color: var(--text-primary); }
.opt-desc { font-size: 10px; color: var(--text-secondary); }

.chat-messages {
  flex: 1;
  overflow-y: auto;
  padding: 20px 0 8px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  scroll-behavior: smooth;
}

.chat-empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  text-align: center;
  padding: 40px 24px;
}
.chat-empty-avatar {
  width: 52px;
  height: 52px;
  border-radius: 50%;
  background: linear-gradient(135deg, #7c3aed 0%, #5b21b6 100%);
  display: flex;
  align-items: center;
  justify-content: center;
  color: #fff;
  margin-bottom: 10px;
  box-shadow: 0 0 0 3px rgba(124,58,237,0.2);
}
.chat-empty-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--chat-text, rgba(255,255,255,0.88));
}
.chat-empty-sub { font-size: 11px; font-family: var(--font-mono); color: var(--chat-muted, rgba(255,255,255,0.42)); margin-top: 2px; }

/* Row layouts */
.user-msg-row {
  display: flex;
  align-items: flex-end;
  justify-content: flex-end;
  gap: 8px;
  padding: 3px 16px;
}
.agent-msg-row {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 3px 16px;
}

/* Avatars */
.user-avatar {
  width: 26px;
  height: 26px;
  border-radius: 50%;
  background: rgba(255,255,255,0.1);
  border: 1px solid rgba(255,255,255,0.15);
  color: rgba(255,255,255,0.7);
  font-size: 11px;
  font-weight: 700;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}
.agent-avatar {
  width: 26px;
  height: 26px;
  border-radius: 50%;
  background: linear-gradient(135deg, #7c3aed 0%, #5b21b6 100%);
  display: flex;
  align-items: center;
  justify-content: center;
  color: #fff;
  flex-shrink: 0;
  margin-top: 2px;
}
.agent-avatar-sm {
  width: 22px;
  height: 22px;
  margin-top: 0;
}
.agent-avatar-spacer {
  width: 26px;
  flex-shrink: 0;
}

/* User bubble */
.bubble-user {
  max-width: 72%;
  padding: 10px 14px;
  border-radius: 14px 14px 4px 14px;
  font-size: 13px;
  line-height: 1.55;
  word-break: break-word;
  background: var(--chat-user-bg, #1e1b2e);
  border: 1px solid var(--chat-user-border, rgba(124,58,237,0.35));
  color: var(--chat-text, rgba(255,255,255,0.88));
}

.msg-images {
  display: flex;
  flex-wrap: wrap;
  gap: 5px;
  margin-bottom: 6px;
}
.msg-img {
  max-width: 200px;
  max-height: 160px;
  object-fit: cover;
  border-radius: 5px;
  display: block;
}

/* Assistant message — no bubble, just content */
.assistant-content {
  flex: 1;
  min-width: 0;
  font-size: 13px;
  line-height: 1.65;
  color: var(--chat-text, rgba(255,255,255,0.88));
  padding-top: 4px;
}

.partial-cursor {
  display: inline-block;
  width: 2px;
  height: 13px;
  background: var(--chat-accent, #7c3aed);
  vertical-align: middle;
  margin-left: 2px;
  animation: blink 1s step-end infinite;
}
@keyframes blink { 0%, 100% { opacity: 1; } 50% { opacity: 0; } }

/* Thinking */
.bubble-thinking {
  font-size: 11px;
  font-family: var(--font-mono);
  color: var(--chat-muted, rgba(255,255,255,0.42));
  border: 1px dashed rgba(255,255,255,0.12);
  border-radius: 8px;
  padding: 4px 10px;
  max-width: 90%;
  opacity: 0.75;
}
.thinking-summary {
  cursor: pointer;
  color: var(--chat-muted, rgba(255,255,255,0.42));
  font-style: italic;
  user-select: none;
}
.thinking-summary:hover { color: rgba(255,255,255,0.7); }
.thinking-body {
  margin: 6px 0 2px;
  white-space: pre-wrap;
  color: var(--chat-muted, rgba(255,255,255,0.42));
  font-size: 10px;
  line-height: 1.4;
  max-height: 200px;
  overflow-y: auto;
  scrollbar-width: thin;
}

/* Tool pill — compact, expandable */
.tool-pill {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 3px 9px 3px 6px;
  background: rgba(124,58,237,0.08);
  border: 1px solid rgba(124,58,237,0.22);
  border-radius: 20px;
  font-size: 11px;
  font-family: var(--font-mono);
  color: rgba(255,255,255,0.55);
  cursor: pointer;
  user-select: none;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  transition: background .1s, color .1s;
}
.tool-pill:hover {
  background: rgba(124,58,237,0.14);
  color: rgba(255,255,255,0.8);
}
.tool-caret {
  flex-shrink: 0;
  color: rgba(124,58,237,0.7);
  transition: transform .15s;
}
.tool-caret-open { transform: rotate(90deg); }
.tool-icon { color: rgba(124,58,237,0.8); flex-shrink: 0; }
.tool-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tool-args {
  margin: 0;
  padding: 8px 12px;
  background: rgba(255,255,255,0.03);
  border: 1px solid rgba(255,255,255,0.07);
  border-radius: 8px;
  font-size: 10px;
  font-family: var(--font-mono);
  color: rgba(255,255,255,0.5);
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 200px;
  overflow-y: auto;
  max-width: min(560px, 90vw);
}

/* Permission log */
.bubble-permission {
  display: inline-flex;
  align-items: center;
  padding: 3px 9px;
  border-radius: 20px;
  font-size: 11px;
  font-family: var(--font-mono);
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.bubble-permission.perm-granted {
  background: rgba(22,163,74,0.12);
  border: 1px solid rgba(22,163,74,0.3);
  color: #4ade80;
}
.bubble-permission.perm-rejected {
  background: rgba(185,28,28,0.12);
  border: 1px solid rgba(185,28,28,0.3);
  color: #f87171;
}
.perm-log-text { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

.chat-thinking {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 16px;
}
.thinking-dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: rgba(124,58,237,0.6);
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
  border-top: 1px solid rgba(255,255,255,0.06);
  flex-shrink: 0;
  min-height: 22px;
}

.status-spacer { flex: 1; }

.status-item {
  font-size: 10px;
  font-family: var(--font-mono);
  color: rgba(255,255,255,0.38);
  white-space: nowrap;
}

.status-muted { color: rgba(255,255,255,0.28); }
.status-plan {
  color: #f59e0b;
  font-weight: 600;
  background: rgba(245,158,11,0.12);
  padding: 1px 5px;
  border-radius: 3px;
}
.status-cost { color: #a78bfa; }
.status-busy { color: #a78bfa; animation: blink 1s step-end infinite; }
.status-queued { color: rgba(255,255,255,0.3); font-family: var(--font-mono); }

/* Command suggestions */
.cmd-suggestions {
  border-top: 1px solid rgba(255,255,255,0.07);
  background: #18181c;
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
.cmd-suggestion.selected { background: rgba(255,255,255,0.05); }

.cmd-name {
  font-family: var(--font-mono);
  font-size: 12px;
  font-weight: 600;
  color: #a78bfa;
  flex-shrink: 0;
  min-width: 100px;
}

.cmd-desc {
  font-size: 11px;
  color: rgba(255,255,255,0.38);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* New-style input wrap */
.chat-input-wrap {
  padding: 10px 14px 6px;
  flex-shrink: 0;
  background: #0f0f11;
}

.chat-input-box {
  background: #1a1a20;
  border: 1px solid rgba(255,255,255,0.10);
  border-radius: 14px;
  overflow: hidden;
  transition: border-color .15s;
}
.chat-input-box:focus-within { border-color: rgba(124,58,237,0.5); }
.input-queued { border-color: rgba(124,58,237,0.35) !important; }

.chat-input {
  display: block;
  width: 100%;
  background: transparent;
  border: none;
  color: rgba(255,255,255,0.88);
  font-family: var(--font-ui);
  font-size: 13px;
  line-height: 1.5;
  outline: none;
  padding: 12px 14px 6px;
  resize: none;
  min-height: 40px;
  max-height: 160px;
  overflow-y: auto;
  scrollbar-width: none;
  box-sizing: border-box;
}
.chat-input::-webkit-scrollbar { display: none; }
.chat-input::placeholder { color: rgba(255,255,255,0.3); }

.chat-input-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 10px 10px;
  gap: 6px;
}

.toolbar-left {
  display: flex;
  align-items: center;
  gap: 4px;
}
.toolbar-avatar {
  width: 22px;
  height: 22px;
  border-radius: 50%;
  object-fit: cover;
  object-position: center 18%;
  border: 1px solid var(--border, rgba(255, 255, 255, 0.18));
  flex-shrink: 0;
  margin-right: 2px;
}

.toolbar-right {
  display: flex;
  align-items: center;
  gap: 6px;
}

.toolbar-btn {
  background: none;
  border: none;
  color: rgba(255,255,255,0.45);
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 7px;
  border-radius: 7px;
  font-size: 11px;
  font-family: var(--font-ui);
  transition: color .12s, background .12s;
}
.toolbar-btn:hover { color: rgba(255,255,255,0.8); background: rgba(255,255,255,0.06); }
.toolbar-btn-label { font-weight: 500; }
.btn-caret { opacity: 0.6; }

/* Model / floating menus */
.model-dropdown { position: relative; }

.floating-menu {
  position: fixed;
  z-index: 1000;
  min-width: 200px;
  padding: 4px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  background: #1e1e26;
  border: 1px solid rgba(255,255,255,0.1);
  border-radius: 10px;
  box-shadow: 0 10px 30px rgba(0,0,0,0.5);
}

.floating-menu-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  padding: 7px 10px;
  background: none;
  border: none;
  border-radius: 7px;
  color: rgba(255,255,255,0.8);
  font-size: 12px;
  font-weight: 500;
  text-align: left;
  cursor: pointer;
  transition: background .1s;
}
.floating-menu-item:hover { background: rgba(255,255,255,0.06); }
.floating-menu-item-active { color: #a78bfa; background: rgba(124,58,237,0.12); }

.model-id-hint {
  font-size: 9px;
  font-family: var(--font-mono);
  color: rgba(255,255,255,0.3);
  margin-left: 6px;
}

/* Send button */
.send-btn {
  background: #7c3aed;
  border: none;
  border-radius: 50%;
  color: #fff;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  flex-shrink: 0;
  transition: background .12s, opacity .12s;
}
.send-btn:hover:not(:disabled) { background: #6d28d9; }
.send-btn:disabled { opacity: 0.35; cursor: default; }
.send-btn-abort { background: #dc2626; }
.send-btn-abort:hover:not(:disabled) { background: #b91c1c; }

/* Pending image previews */
.pending-images {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  padding: 6px 14px 0;
  flex-shrink: 0;
}

.pending-img-wrap {
  position: relative;
  flex-shrink: 0;
}

.pending-img {
  width: 72px;
  height: 72px;
  object-fit: cover;
  border-radius: 6px;
  border: 1px solid rgba(255,255,255,0.1);
  display: block;
}

.pending-img-remove {
  position: absolute;
  top: -5px;
  right: -5px;
  width: 16px;
  height: 16px;
  background: #1e1e26;
  border: 1px solid rgba(255,255,255,0.15);
  border-radius: 50%;
  color: rgba(255,255,255,0.5);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  transition: color .1s, background .1s;
}
.pending-img-remove:hover { color: #f87171; background: rgba(185,28,28,0.2); }

/* Markdown body inside assistant messages */
.md-body {
  font-family: var(--font-ui);
  font-size: 13px;
  color: rgba(255,255,255,0.88);
  line-height: 1.65;
  white-space: normal;
}
.md-body :deep(p) { margin: 0 0 10px; }
.md-body :deep(p:last-child) { margin-bottom: 0; }
.md-body :deep(ul), .md-body :deep(ol) { margin: 4px 0 10px; padding-left: 20px; }
.md-body :deep(li) { margin: 3px 0; }
.md-body :deep(code) { font-family: var(--font-mono); font-size: 11px; background: rgba(124,58,237,0.14); color: #c4b5fd; padding: 1px 5px; border-radius: 4px; }
.md-body :deep(pre) { background: rgba(0,0,0,0.35); border: 1px solid rgba(255,255,255,0.08); border-radius: 8px; padding: 12px 14px; overflow-x: auto; margin: 8px 0; }
.md-body :deep(pre code) { background: none; padding: 0; font-size: 11px; color: rgba(255,255,255,0.75); }
.md-body :deep(blockquote) { border-left: 3px solid rgba(124,58,237,0.6); margin: 6px 0; padding-left: 12px; color: rgba(255,255,255,0.55); }
.md-body :deep(h1), .md-body :deep(h2), .md-body :deep(h3) { font-weight: 700; margin: 14px 0 6px; color: rgba(255,255,255,0.95); }
.md-body :deep(h1) { font-size: 16px; }
.md-body :deep(h2) { font-size: 14px; }
.md-body :deep(h3) { font-size: 13px; }
.md-body :deep(a) { color: #a78bfa; text-decoration: underline; }
.md-body :deep(hr) { border: none; border-top: 1px solid rgba(255,255,255,0.1); margin: 10px 0; }
.md-body :deep(table) { border-collapse: collapse; font-size: 12px; margin: 8px 0; }
.md-body :deep(th), .md-body :deep(td) { border: 1px solid rgba(255,255,255,0.1); padding: 5px 10px; }
.md-body :deep(th) { background: rgba(255,255,255,0.05); font-weight: 600; }
</style>
