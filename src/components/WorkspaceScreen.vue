<template>
  <div class="workspace-screen">
    <div class="ws-header">
      <div class="ws-logo">
        <PhTerminalWindow :size="28" weight="duotone" class="logo-icon" />
        <span class="logo-name">Agentic IDE</span>
      </div>
      <p class="ws-subtitle">Select or create a workspace to start</p>
    </div>

    <div class="ws-body">
      <div class="ws-list-header">
        <span class="section-label">Recent Workspaces</span>
        <button class="btn-primary" @click="pickFolder">
          <PhFolderPlus :size="13" />
          New Workspace
        </button>
      </div>

      <div class="ws-list" v-if="store.workspaces.length">
        <div
          v-for="ws in store.workspaces"
          :key="ws.id"
          class="ws-item"
          @click="openWorkspace(ws)"
        >
          <PhFolder :size="20" weight="fill" class="ws-item-icon" />
          <div class="ws-item-info">
            <div class="ws-item-name">{{ ws.name }}</div>
            <div class="ws-item-path">{{ ws.path }}</div>
          </div>
          <div class="ws-item-meta">
            <span class="ws-time">{{ formatTime(ws.last_opened ?? ws.created_at) }}</span>
            <button class="ws-delete" title="Remove" @click.stop="store.remove(ws.id)">
              <PhX :size="11" />
            </button>
          </div>
        </div>
      </div>

      <div class="ws-empty" v-else>
        <PhFolderOpen :size="40" weight="thin" class="empty-icon" />
        <p>No workspaces yet.</p>
        <button class="btn-primary" @click="pickFolder">
          <PhFolderPlus :size="13" />
          Open a folder
        </button>
      </div>

      <div class="ws-actions">
        <button class="btn-secondary" @click="pickFolder">
          <PhFolderOpen :size="13" />
          Open Folder…
        </button>
      </div>
    </div>

    <!-- Name dialog -->
    <div class="dialog-overlay" v-if="pendingPath" @click.self="pendingPath = ''">
      <div class="dialog">
        <h3>Name this workspace</h3>
        <p class="dialog-path">{{ pendingPath }}</p>
        <input
          v-model="pendingName"
          class="dialog-input"
          placeholder="Workspace name"
          @keydown.enter="confirmCreate"
          @keydown.esc="pendingPath = ''"
          ref="nameInputEl"
        />
        <div class="dialog-actions">
          <button class="btn-secondary" @click="pendingPath = ''">Cancel</button>
          <button class="btn-primary" @click="confirmCreate" :disabled="!pendingName.trim()">Create</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, nextTick, onMounted } from "vue";
import { PhTerminalWindow, PhFolder, PhFolderOpen, PhFolderPlus, PhX } from "@phosphor-icons/vue";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useWorkspaceStore, type Workspace } from "@/stores/workspace";

const emit = defineEmits<{ open: [ws: Workspace] }>();
const store = useWorkspaceStore();

const pendingPath = ref("");
const pendingName = ref("");
const nameInputEl = ref<HTMLInputElement>();

onMounted(() => store.load());

async function pickFolder() {
  const selected = await openDialog({ directory: true, multiple: false });
  if (!selected || typeof selected !== "string") return;
  pendingPath.value = selected;
  pendingName.value = selected.split("/").pop() || selected;
  await nextTick();
  nameInputEl.value?.focus();
  nameInputEl.value?.select();
}

async function confirmCreate() {
  if (!pendingName.value.trim()) return;
  const ws = await store.create(pendingName.value.trim(), pendingPath.value);
  pendingPath.value = "";
  pendingName.value = "";
  openWorkspace(ws);
}

async function openWorkspace(ws: Workspace) {
  await store.open(ws);
  emit("open", ws);
}

function formatTime(ts: number): string {
  const diff = Date.now() - ts * 1000;
  if (diff < 60_000) return "just now";
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`;
  return new Date(ts * 1000).toLocaleDateString();
}
</script>

<style scoped>
.workspace-screen {
  flex: 1;
  background: var(--bg-base);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 40px;
  padding: 40px;
  overflow-y: auto;
}

.ws-header {
  text-align: center;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
}

.ws-logo {
  display: flex;
  align-items: center;
  gap: 10px;
}

.logo-icon { color: var(--accent); }

.logo-name {
  font-size: 22px;
  font-weight: 600;
  color: var(--text-primary);
  letter-spacing: -0.02em;
}

.ws-subtitle {
  font-size: 13px;
  color: var(--text-secondary);
}

.ws-body {
  width: 560px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.ws-list-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.section-label {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--text-muted);
}

.ws-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  max-height: 360px;
  overflow-y: auto;
}

.ws-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 12px;
  border-radius: 6px;
  cursor: pointer;
  border: 1px solid transparent;
  transition: border-color 0.1s, background 0.1s;
}
.ws-item:hover { background: var(--bg-hover); border-color: var(--border); }

.ws-item-icon { color: #60a5fa; flex-shrink: 0; }

.ws-item-info { flex: 1; min-width: 0; }

.ws-item-name {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-primary);
}

.ws-item-path {
  font-size: 11px;
  color: var(--text-secondary);
  font-family: var(--font-mono);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ws-item-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.ws-time {
  font-size: 11px;
  color: var(--text-muted);
}

.ws-delete {
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  padding: 3px 5px;
  border-radius: 3px;
  opacity: 0;
  transition: opacity 0.15s;
}
.ws-item:hover .ws-delete { opacity: 1; }
.ws-delete:hover { color: var(--red); background: rgba(239,68,68,0.1); }

.ws-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  padding: 48px;
  border: 1px dashed var(--border);
  border-radius: 8px;
  color: var(--text-secondary);
  font-size: 13px;
}

.empty-icon { color: var(--text-muted); }

.ws-actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
  padding-top: 4px;
}

.btn-primary {
  display: flex;
  align-items: center;
  gap: 5px;
  background: var(--accent);
  border: none;
  border-radius: 5px;
  color: #fff;
  cursor: pointer;
  font-size: 12px;
  font-weight: 600;
  padding: 6px 14px;
  transition: background 0.15s;
}
.btn-primary:hover:not(:disabled) { background: var(--accent-dim); }
.btn-primary:disabled { opacity: 0.5; cursor: default; }

.btn-secondary {
  display: flex;
  align-items: center;
  gap: 5px;
  background: var(--bg-hover);
  border: 1px solid var(--border);
  border-radius: 5px;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 12px;
  padding: 6px 14px;
  transition: color 0.15s, border-color 0.15s;
}
.btn-secondary:hover { color: var(--text-primary); border-color: #444; }

/* Dialog */
.dialog-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0,0,0,0.6);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 100;
}

.dialog {
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 24px;
  width: 420px;
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.dialog h3 {
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary);
}

.dialog-path {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-secondary);
  padding: 8px 10px;
  background: var(--bg-base);
  border-radius: 4px;
  border: 1px solid var(--border);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dialog-input {
  background: var(--bg-base);
  border: 1px solid var(--border);
  border-radius: 5px;
  color: var(--text-primary);
  font-size: 13px;
  outline: none;
  padding: 8px 10px;
  width: 100%;
}
.dialog-input:focus { border-color: var(--accent); }

.dialog-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
