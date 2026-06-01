<template>
  <aside class="sidebar">
    <div class="sidebar-header">
      <span class="header-label">Workspaces</span>
      <button class="icon-btn" title="Open folder" @click="pickFolder">
        <PhFolderPlus :size="14" />
      </button>
    </div>

    <div class="ws-list">
      <div
        v-for="item in store.workspaces"
        :key="item.id"
        class="ws-item"
        :class="{ active: store.active?.id === item.id }"
        @click="store.open(item)"
      >
        <PhFolder :size="14" weight="fill" class="ws-icon" />
        <div class="ws-info">
          <div class="ws-name">{{ item.name }}</div>
          <div class="ws-path">{{ item.path }}</div>
        </div>
        <button class="ws-delete" title="Remove" @click.stop="store.remove(item.id)">
          <PhX :size="10" />
        </button>
      </div>

      <div v-if="store.workspaces.length === 0" class="ws-empty">
        No workspaces.<br />Click + to open a folder.
      </div>
    </div>

    <div class="sidebar-footer">
      <button class="footer-btn" @click="pickFolder">
        <PhFolderOpen :size="13" />
        Open Folder…
      </button>
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
  </aside>
</template>

<script setup lang="ts">
import { ref, nextTick } from "vue";
import { PhFolderPlus, PhFolder, PhFolderOpen, PhX } from "@phosphor-icons/vue";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useWorkspaceStore } from "@/stores/workspace";

const store = useWorkspaceStore();

const pendingPath = ref("");
const pendingName = ref("");
const nameInputEl = ref<HTMLInputElement>();

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
  store.open(ws);
}
</script>

<style scoped>
.sidebar {
  width: var(--sidebar-width);
  background: var(--bg-panel);
  border-right: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
  overflow: hidden;
}

.sidebar-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}

.header-label {
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--text-secondary);
}

.icon-btn {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  display: flex;
  align-items: center;
  padding: 2px 4px;
  border-radius: 3px;
}
.icon-btn:hover { color: var(--text-primary); background: var(--bg-hover); }

.ws-list {
  flex: 1;
  overflow-y: auto;
  padding: 4px 0;
}

.ws-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  cursor: pointer;
  border-radius: 3px;
  margin: 0 4px;
  position: relative;
}
.ws-item:hover { background: var(--bg-hover); }
.ws-item.active { background: var(--bg-selected); }

.ws-icon { color: #60a5fa; flex-shrink: 0; }
.ws-item.active .ws-icon { color: var(--accent); }

.ws-info {
  flex: 1;
  min-width: 0;
}

.ws-name {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ws-path {
  font-size: 10px;
  font-family: var(--font-mono);
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ws-delete {
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  display: none;
  align-items: center;
  padding: 3px 4px;
  border-radius: 3px;
  flex-shrink: 0;
}
.ws-item:hover .ws-delete { display: flex; }
.ws-delete:hover { color: var(--red); }

.ws-empty {
  font-size: 11px;
  color: var(--text-muted);
  text-align: center;
  padding: 24px 16px;
  line-height: 1.6;
}

.sidebar-footer {
  border-top: 1px solid var(--border);
  padding: 8px;
  flex-shrink: 0;
}

.footer-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  background: none;
  border: 1px solid var(--border);
  border-radius: 4px;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 11px;
  padding: 5px 10px;
}
.footer-btn:hover { color: var(--text-primary); border-color: #444; background: var(--bg-hover); }

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
  width: 400px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.dialog h3 {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
}

.dialog-path {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-secondary);
  padding: 6px 8px;
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
  padding: 7px 10px;
  width: 100%;
}
.dialog-input:focus { border-color: var(--accent); }

.dialog-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
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
}
.btn-secondary:hover { color: var(--text-primary); border-color: #444; }
</style>
