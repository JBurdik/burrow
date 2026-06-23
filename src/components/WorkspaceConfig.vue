<template>
  <div class="wc-overlay" @click.self="$emit('close')">
    <div class="wc-modal" @keydown.esc.stop="$emit('close')">
      <!-- Header -->
      <div class="wc-header">
        <span class="wc-title">{{ workspaceName }} — Project Config</span>
        <button class="wc-close" title="Close (Esc)" @click="$emit('close')">
          <PhX :size="14" />
        </button>
      </div>

      <!-- Tabs -->
      <div class="wc-tabs">
        <button class="wc-tab" :class="{ active: tab === 'prompt' }" @click="tab = 'prompt'">Manager Prompt</button>
        <button class="wc-tab" :class="{ active: tab === 'scripts' }" @click="tab = 'scripts'">Scripts</button>
      </div>

      <!-- Tab: Manager Prompt -->
      <div v-if="tab === 'prompt'" class="wc-body">
        <p class="wc-hint">
          Saved to <code>{{ workspacePath }}/.burrow/manager.md</code>. Overrides the default Manager system prompt for this project.
        </p>
        <textarea
          v-model="promptContent"
          class="wc-textarea"
          placeholder="# Project-specific manager instructions&#10;&#10;Describe the project, conventions, team norms, or anything the Manager should know..."
          spellcheck="false"
        />
        <div class="wc-footer">
          <span v-if="saveState === 'ok'" class="save-msg ok">Saved</span>
          <span v-else-if="saveState === 'err'" class="save-msg err">Save failed</span>
          <button class="wc-btn primary" :disabled="saving" @click="savePrompt">
            {{ saving ? 'Saving…' : 'Save' }}
          </button>
        </div>
      </div>

      <!-- Tab: Scripts -->
      <div v-else class="wc-body scripts-body">
        <div class="script-list">
            <div v-for="s in scripts" :key="s.id" class="script-card">
              <div class="sc-top">
                <span class="sc-dot" :style="{ background: s.color || '#60a5fa' }" />
                <input
                  class="sc-name"
                  :value="s.name"
                  @change="patch(s.id, { name: ($event.target as HTMLInputElement).value })"
                />
                <button class="sc-del" title="Delete script" @click="scriptsStore.removeScript(workspacePath, s.id)">
                  <PhTrash :size="13" />
                </button>
              </div>
              <textarea
                class="sc-steps"
                :value="s.steps.join('\n')"
                placeholder="One shell command per line"
                rows="3"
                @change="patch(s.id, { steps: splitSteps(($event.target as HTMLTextAreaElement).value) })"
              />
              <label class="sc-toggle">
                <input
                  type="checkbox"
                  :checked="s.continueOnError"
                  @change="patch(s.id, { continueOnError: ($event.target as HTMLInputElement).checked })"
                />
                <span>Continue on error</span>
              </label>
            </div>
          </div>
          <button class="wc-btn add-btn" @click="scriptsStore.addScript(workspacePath)">
            <PhPlus :size="13" /> Add script
          </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { PhX, PhPlus, PhTrash } from '@phosphor-icons/vue'
import { useScriptsStore } from '@/stores/scripts'

const props = defineProps<{
  workspacePath: string
  workspaceName: string
}>()
const emit = defineEmits<{ close: [] }>()

const tab = ref<'prompt' | 'scripts'>('prompt')

// ── Manager Prompt ─────────────────────────────────────────────────────────
const promptContent = ref('')
const saving = ref(false)
const saveState = ref<'idle' | 'ok' | 'err'>('idle')
let saveTimer: ReturnType<typeof setTimeout> | null = null

async function loadPrompt() {
  try {
    promptContent.value = await invoke<string>('read_text_file', {
      path: props.workspacePath + '/.burrow/manager.md',
    })
  } catch {
    promptContent.value = ''
  }
}

async function savePrompt() {
  saving.value = true
  try {
    await invoke('write_text_file', {
      path: props.workspacePath + '/.burrow/manager.md',
      content: promptContent.value,
    })
    saveState.value = 'ok'
  } catch {
    saveState.value = 'err'
  } finally {
    saving.value = false
    if (saveTimer) clearTimeout(saveTimer)
    saveTimer = setTimeout(() => { saveState.value = 'idle' }, 2500)
  }
}

// ── Scripts ────────────────────────────────────────────────────────────────
const scriptsStore = useScriptsStore()

const scripts = computed(() => scriptsStore.scriptsFor(props.workspacePath))

function patch(id: string, p: Parameters<typeof scriptsStore.updateScript>[2]) {
  scriptsStore.updateScript(props.workspacePath, id, p)
}

function splitSteps(raw: string): string[] {
  return raw.split('\n').map((l) => l.trimEnd()).filter((l) => l.length > 0)
}

// ── Keyboard ───────────────────────────────────────────────────────────────
function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') emit('close')
}

onMounted(() => {
  loadPrompt()
  scriptsStore.loadForPath(props.workspacePath)
  document.addEventListener('keydown', onKeydown)
})

onUnmounted(() => {
  document.removeEventListener('keydown', onKeydown)
  if (saveTimer) clearTimeout(saveTimer)
})
</script>

<style scoped>
.wc-overlay {
  position: fixed;
  inset: 0;
  z-index: 800;
  background: rgba(0, 0, 0, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
}

.wc-modal {
  width: 640px;
  max-height: 80vh;
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: 10px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  box-shadow: 0 24px 64px rgba(0, 0, 0, 0.6);
}

/* Header */
.wc-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 16px;
  height: 48px;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}
.wc-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}
.wc-close {
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  display: flex;
  padding: 4px;
  border-radius: 4px;
}
.wc-close:hover { color: var(--text-primary); background: var(--bg-hover); }

/* Tabs */
.wc-tabs {
  display: flex;
  border-bottom: 1px solid var(--border);
  padding: 0 16px;
  flex-shrink: 0;
}
.wc-tab {
  background: none;
  border: none;
  border-bottom: 2px solid transparent;
  color: var(--text-muted);
  cursor: pointer;
  font-size: 12px;
  padding: 10px 12px 8px;
  margin-bottom: -1px;
}
.wc-tab:hover { color: var(--text-primary); }
.wc-tab.active {
  color: var(--text-primary);
  border-bottom-color: var(--accent);
}

/* Body */
.wc-body {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.wc-hint {
  font-size: 11px;
  color: var(--text-muted);
  margin: 0;
}
.wc-hint code {
  font-family: monospace;
  color: var(--text-secondary);
}
.err-hint { color: var(--red, #f87171); }

.wc-textarea {
  flex: 1;
  min-height: 280px;
  resize: vertical;
  background: var(--bg-base);
  border: 1px solid var(--border);
  border-radius: 6px;
  color: var(--text-primary);
  font-family: monospace;
  font-size: 12px;
  line-height: 1.6;
  padding: 10px 12px;
  outline: none;
}
.wc-textarea:focus { border-color: var(--accent); }

/* Footer */
.wc-footer {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 10px;
  flex-shrink: 0;
}
.save-msg { font-size: 12px; }
.save-msg.ok { color: var(--green, #34d399); }
.save-msg.err { color: var(--red, #f87171); }

.wc-btn {
  background: var(--bg-hover);
  border: 1px solid var(--border);
  border-radius: 5px;
  color: var(--text-primary);
  cursor: pointer;
  font-size: 12px;
  padding: 5px 14px;
  display: flex;
  align-items: center;
  gap: 5px;
}
.wc-btn:hover { background: var(--bg-panel); border-color: var(--accent); }
.wc-btn:disabled { opacity: 0.5; cursor: not-allowed; }
.wc-btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
.wc-btn.primary:hover { opacity: 0.85; }

/* Scripts */
.scripts-body { gap: 10px; }

.script-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.script-card {
  background: var(--bg-base);
  border: 1px solid var(--border);
  border-radius: 7px;
  padding: 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.sc-top {
  display: flex;
  align-items: center;
  gap: 8px;
}
.sc-dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  flex-shrink: 0;
}
.sc-name {
  flex: 1;
  background: transparent;
  border: none;
  border-bottom: 1px solid transparent;
  color: var(--text-primary);
  font-size: 13px;
  font-weight: 500;
  outline: none;
  padding: 1px 0;
}
.sc-name:focus { border-bottom-color: var(--accent); }

.sc-del {
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  display: flex;
  padding: 2px;
  border-radius: 3px;
}
.sc-del:hover { color: var(--red, #f87171); }

.sc-steps {
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: 5px;
  color: var(--text-primary);
  font-family: monospace;
  font-size: 11px;
  line-height: 1.6;
  outline: none;
  padding: 6px 8px;
  resize: vertical;
  width: 100%;
  box-sizing: border-box;
}
.sc-steps:focus { border-color: var(--accent); }

.sc-toggle {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  color: var(--text-muted);
  cursor: pointer;
  user-select: none;
}
.sc-toggle input { cursor: pointer; accent-color: var(--accent); }

.add-btn { align-self: flex-start; margin-top: 4px; }
</style>
