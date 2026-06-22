<template>
  <header class="m-nav">
    <button class="m-nav-back" @click="router.back()" aria-label="Back">‹ Back</button>
    <span class="m-nav-title">{{ session?.title || `PTY ${ptyId}` }}</span>
    <span v-if="session" :class="['s-dot', session.status]" style="margin-left:4px" aria-hidden="true" />
    <button class="m-btn-ghost" style="padding:4px 8px;font-size:12px" @click="reload()" title="Refresh output">↺</button>
  </header>

  <div class="m-body out-body">
    <!-- loading -->
    <div v-if="loading" class="m-state">
      <span class="m-state-icon">⏳</span>
      <span class="m-state-msg">Loading output…</span>
    </div>

    <!-- error -->
    <div v-else-if="loadError" class="m-state">
      <span class="m-state-icon">✕</span>
      <span class="m-state-msg">Could not load output.</span>
      <span class="m-state-detail">{{ loadError }}</span>
      <button class="m-btn-ghost" style="margin-top:8px" @click="reload()">Retry</button>
    </div>

    <!-- session not found in store -->
    <div v-else-if="!session && !loading" class="m-state">
      <span class="m-state-icon">?</span>
      <span class="m-state-msg">Session not found.</span>
    </div>

    <!-- output -->
    <div v-else class="out-wrap">
      <div v-if="session" class="out-meta">
        <span :class="['s-dot', session.status]" aria-hidden="true" />
        <span class="out-status-label">{{ session.status }}</span>
        <span v-if="session.statusDetail" class="out-detail">{{ session.statusDetail }}</span>
        <span class="out-cwd">{{ session.cwd }}</span>
        <span v-if="session.model" class="out-model">{{ session.model }}</span>
      </div>

      <pre v-if="output" class="out-pre" ref="preEl">{{ output }}</pre>
      <div v-else class="m-state" style="padding:24px">
        <span class="m-state-msg">No output captured yet.</span>
      </div>
    </div>
  </div>

  <form v-if="session" class="out-composer" @submit.prevent="send">
    <button
      type="button"
      class="out-interrupt"
      :disabled="sending"
      aria-label="Interrupt session"
      title="Send Ctrl-C"
      @click="interrupt"
    >⌃C</button>
    <textarea
      v-model="draft"
      class="out-input"
      rows="1"
      placeholder="Send a message or command"
      enterkeyhint="send"
      :disabled="sending"
      @keydown.enter.exact.prevent="send"
    />
    <button class="out-send" type="submit" :disabled="sending || !draft.trim()">
      {{ sending ? '…' : 'Send' }}
    </button>
  </form>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { useRemoteStore } from '../store';

// strip ANSI escape codes for plain-text display
function stripAnsi(s: string) {
  // eslint-disable-next-line no-control-regex
  return s.replace(/\x1b\][^\x07]*(?:\x07|\x1b\\)/g, '')
          .replace(/\x1b(?:\[[0-?]*[ -/]*[@-~]|[@-_])/g, '')
          .replace(/\r/g, '');
}

const route  = useRoute();
const router = useRouter();
const store  = useRemoteStore();

const ptyId     = computed(() => Number(route.params.ptyId));
const session   = computed(() => store.allSessions.find((s) => s.ptyId === ptyId.value));
const output    = ref('');
const loading   = ref(false);
const loadError = ref('');
const preEl     = ref<HTMLPreElement | null>(null);
const draft     = ref('');
const sending   = ref(false);
let pollTimer: number | null = null;

async function reload(silent = false) {
  if (!silent) loading.value = true;
  loadError.value = '';
  try {
    const raw = await store.getOutput(ptyId.value);
    output.value = stripAnsi(raw);
  } catch (e: any) {
    loadError.value = e.message ?? 'Failed';
  } finally {
    if (!silent) loading.value = false;
    // scroll to bottom after paint
    requestAnimationFrame(() => {
      preEl.value?.scrollIntoView({ block: 'end' });
    });
  }
}

async function send() {
  const text = draft.value.trim();
  if (!text || sending.value) return;
  sending.value = true;
  try {
    await store.sendInput(ptyId.value, `${text}\r`);
    draft.value = '';
    window.setTimeout(() => reload(true), 200);
  } catch (e: any) {
    loadError.value = e.message ?? 'Could not send input';
  } finally {
    sending.value = false;
  }
}

async function interrupt() {
  if (sending.value) return;
  sending.value = true;
  try {
    await store.interrupt(ptyId.value);
    window.setTimeout(() => reload(true), 200);
  } catch (e: any) {
    loadError.value = e.message ?? 'Could not interrupt session';
  } finally {
    sending.value = false;
  }
}

onMounted(() => {
  reload();
  pollTimer = window.setInterval(() => reload(true), 2000);
});

onUnmounted(() => {
  if (pollTimer !== null) window.clearInterval(pollTimer);
});

// re-fetch when status changes to done/review (turn finished)
watch(() => session.value?.status, (s, prev) => {
  if ((s === 'done' || s === 'review') && prev === 'running') reload();
});
</script>

<style scoped>
.out-body {
  display: flex;
  flex-direction: column;
}

.out-wrap {
  display: flex;
  flex-direction: column;
  flex: 1;
}

.out-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 14px;
  background: var(--bg-panel);
  border-bottom: 1px solid var(--border);
  flex-wrap: wrap;
}
.out-status-label {
  font-size: 11px;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.06em;
}
.out-detail {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--red);
}
.out-cwd {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
  min-width: 0;
}
.out-model {
  font-family: var(--font-mono);
  font-size: 10px;
  color: var(--text-muted);
  background: var(--bg-hover);
  border: 1px solid var(--border);
  border-radius: 3px;
  padding: 0 4px;
  flex-shrink: 0;
}

.out-pre {
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.55;
  color: var(--text-primary);
  background: var(--bg-base);
  margin: 0;
  padding: 14px 14px calc(var(--safe-bottom) + 24px);
  white-space: pre-wrap;
  word-break: break-all;
  overflow-x: hidden;
  flex: 1;
}

.out-composer {
  display: flex;
  align-items: flex-end;
  gap: 8px;
  padding: 8px 10px calc(var(--safe-bottom) + 8px);
  border-top: 1px solid var(--border);
  background: var(--bg-panel);
}

.out-input {
  flex: 1;
  min-width: 0;
  max-height: 112px;
  resize: none;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg-hover);
  color: var(--text-primary);
  font: 13px/1.4 var(--font-ui);
  padding: 9px 10px;
  outline: none;
}

.out-input:focus { border-color: var(--accent); }

.out-send,
.out-interrupt {
  min-height: 38px;
  border-radius: 8px;
  border: 1px solid var(--border);
  font-size: 12px;
  font-weight: 700;
}

.out-send {
  padding: 0 14px;
  border-color: var(--accent);
  background: var(--accent);
  color: #f8fafc;
}

.out-interrupt {
  width: 42px;
  background: var(--bg-hover);
  color: var(--text-secondary);
  font-family: var(--font-mono);
}

.out-send:disabled,
.out-interrupt:disabled { opacity: 0.45; }
</style>
