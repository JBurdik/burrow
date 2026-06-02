<template>
  <aside class="sidebar">
    <div class="sidebar-header">
      <span class="header-label">Workspaces</span>
      <button class="icon-btn" title="Open folder" @click="pickFolder">
        <PhFolderPlus :size="14" />
      </button>
    </div>

    <div class="ws-list">
      <div v-for="item in store.workspaces" :key="item.id" class="ws-group">
        <div
          class="ws-item"
          :class="{ active: store.active?.id === item.id }"
          @click="store.open(item)"
        >
          <div class="ws-icon-wrap" @click.stop="pickIcon(item.id)" title="Change icon">
            <img v-if="store.icons[item.id]" :src="store.icons[item.id]" class="ws-custom-icon" />
            <PhFolder v-else :size="14" weight="fill" class="ws-icon" />
          </div>
          <div class="ws-info">
            <div class="ws-name">{{ item.name }}</div>
            <div class="ws-path">{{ item.path }}</div>
            <button
              v-if="wsBranch[item.id]"
              class="ws-branch-pill"
              :title="`Branch: ${wsBranch[item.id]} — click to switch`"
              @click.stop="openBranchPicker(item, $event)"
            >
              <PhGitBranch :size="9" />
              <span>{{ wsBranch[item.id] }}</span>
            </button>
          </div>
          <button class="ws-delete" title="Remove" @click.stop="store.remove(item.id)">
            <PhX :size="10" />
          </button>
        </div>

        <!-- branch picker dropdown -->
        <div
          v-if="showBranchPicker === item.id"
          class="branch-picker"
          @click.stop
        >
          <input
            ref="branchInputEl"
            v-model="branchFilter"
            class="branch-filter"
            placeholder="Switch or create branch…"
            @keydown.enter="filteredBranches().length === 1
              ? switchBranch(item, filteredBranches()[0])
              : (showCreateOption() && createBranch(item, branchFilter))"
            @keydown.esc="showBranchPicker = null"
          />
          <div class="branch-list">
            <div v-if="branchLoading" class="branch-item branch-loading">Loading…</div>
            <template v-else>
              <div
                v-for="b in filteredBranches()"
                :key="b"
                class="branch-item"
                :class="{ 'branch-current': b === wsBranch[item.id] }"
                @click="switchBranch(item, b)"
              >
                <PhGitBranch :size="10" />
                <span>{{ b }}</span>
                <span v-if="b === wsBranch[item.id]" class="branch-check">✓</span>
              </div>
              <div
                v-if="showCreateOption()"
                class="branch-item branch-create"
                @click="createBranch(item, branchFilter)"
              >
                <PhPlus :size="10" />
                <span>Create "{{ branchFilter.trim() }}"</span>
              </div>
              <div v-if="!branchLoading && filteredBranches().length === 0 && !showCreateOption()" class="branch-empty">
                No branches found
              </div>
            </template>
          </div>
        </div>

        <div v-if="termTabs.tabsByWs[item.id]?.length" class="ws-terminals">
          <div
            v-for="(tab, tabIdx) in termTabs.tabsByWs[item.id]"
            :key="tab.id"
            class="ws-term"
            :class="{
              active:
                store.active?.id === item.id && termTabs.activeByWs[item.id] === tab.id,
              'drag-over': dragOverKey === `${item.id}-${tabIdx}`,
            }"
            draggable="true"
            @click.stop="selectTab(item, tab.id)"
            @dragstart="(e) => onDragStart(item.id, tabIdx, e)"
            @dragover="(e) => onDragOver(item.id, tabIdx, e)"
            @dragleave="onDragLeave"
            @drop="(e) => onDrop(item.id, tabIdx, e)"
            @dragend="onDragEnd"
          >
            <PhRobot v-if="tab.isAgent" :size="11" class="ws-term-icon agent" />
            <PhTerminal v-else :size="11" class="ws-term-icon" />
            <span class="ws-term-label">{{ tab.title }}</span>
            <span
              v-if="tab.status && tab.status !== 'idle'"
              class="status-dot"
              :class="`status-${tab.status}`"
            >{{ tab.status === 'running' ? spinnerFrame : '' }}</span>
            <PhX
              :size="9"
              weight="bold"
              class="ws-term-close"
              title="Close"
              @click.stop="termTabs.close(item.id, tab.id)"
            />
          </div>
          <div
            v-if="store.opened.some((w) => w.id === item.id)"
            class="ws-term ws-term-add"
            title="New terminal"
            @click.stop="addTab(item)"
          >
            <PhPlus :size="11" />
            <span class="ws-term-label">New terminal</span>
          </div>
        </div>
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
import { ref, nextTick, onMounted, watch } from "vue";
import {
  PhFolderPlus,
  PhFolder,
  PhFolderOpen,
  PhX,
  PhTerminal,
  PhRobot,
  PhPlus,
  PhGitBranch,
} from "@phosphor-icons/vue";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { useWorkspaceStore, type Workspace } from "@/stores/workspace";
import { useTerminalTabsStore } from "@/stores/terminalTabs";
import { spinnerFrame } from "@/lib/spinner";

const store = useWorkspaceStore();
const termTabs = useTerminalTabsStore();

// ── branch switcher ──────────────────────────────────────────────────────────
interface GitOutput { stdout: string; stderr: string; code: number; }

const wsBranch = ref<Record<number, string>>({});
const showBranchPicker = ref<number | null>(null);
const branchList = ref<string[]>([]);
const branchFilter = ref("");
const branchLoading = ref(false);
const branchError = ref("");
const branchInputEl = ref<HTMLInputElement[]>([]);

async function fetchBranch(ws: Workspace) {
  try {
    const out = await invoke<GitOutput>("run_git", { cwd: ws.path, args: ["branch", "--show-current"] });
    if (out.code === 0) wsBranch.value[ws.id] = out.stdout.trim();
  } catch {}
}

async function openBranchPicker(ws: Workspace, e: MouseEvent) {
  e.stopPropagation();
  if (showBranchPicker.value === ws.id) { showBranchPicker.value = null; return; }
  branchLoading.value = true;
  branchError.value = "";
  branchFilter.value = "";
  showBranchPicker.value = ws.id;
  try {
    const out = await invoke<GitOutput>("run_git", { cwd: ws.path, args: ["branch", "--list"] });
    if (out.code === 0) {
      branchList.value = out.stdout.split("\n")
        .map(l => l.replace(/^\*?\s+/, "").trim())
        .filter(Boolean);
    }
  } catch (e: unknown) {
    branchError.value = e instanceof Error ? e.message : "git error";
  } finally {
    branchLoading.value = false;
    await nextTick();
    branchInputEl.value[0]?.focus();
  }
}

async function switchBranch(ws: Workspace, name: string) {
  showBranchPicker.value = null;
  try {
    const out = await invoke<GitOutput>("run_git", { cwd: ws.path, args: ["checkout", name] });
    if (out.code === 0) wsBranch.value[ws.id] = name;
    else branchError.value = out.stderr;
  } catch (e: unknown) {
    branchError.value = e instanceof Error ? e.message : "git error";
  }
}

async function createBranch(ws: Workspace, name: string) {
  if (!name.trim()) return;
  showBranchPicker.value = null;
  try {
    const out = await invoke<GitOutput>("run_git", { cwd: ws.path, args: ["checkout", "-b", name.trim()] });
    if (out.code === 0) wsBranch.value[ws.id] = name.trim();
    else branchError.value = out.stderr;
  } catch (e: unknown) {
    branchError.value = e instanceof Error ? e.message : "git error";
  }
}

function filteredBranches() {
  const q = branchFilter.value.toLowerCase();
  if (!q) return branchList.value;
  return branchList.value.filter(b => b.toLowerCase().includes(q));
}

function showCreateOption() {
  const q = branchFilter.value.trim();
  return q && !branchList.value.includes(q);
}

onMounted(() => {
  store.workspaces.forEach(ws => fetchBranch(ws));
  document.addEventListener("click", () => { showBranchPicker.value = null; });
});
watch(() => store.workspaces, (wss) => wss.forEach(ws => {
  if (!(ws.id in wsBranch.value)) fetchBranch(ws);
}), { deep: true });

// ── drag-to-reorder ──────────────────────────────────────────────────────────
const dragSrc = ref<{ wsId: number; fromIdx: number } | null>(null);
const dragOverKey = ref<string | null>(null);

function onDragStart(wsId: number, fromIdx: number, e: DragEvent) {
  dragSrc.value = { wsId, fromIdx };
  e.dataTransfer!.effectAllowed = "move";
}

function onDragOver(wsId: number, toIdx: number, e: DragEvent) {
  if (!dragSrc.value || dragSrc.value.wsId !== wsId) return;
  e.preventDefault();
  e.dataTransfer!.dropEffect = "move";
  dragOverKey.value = `${wsId}-${toIdx}`;
}

function onDragLeave() {
  dragOverKey.value = null;
}

function onDrop(wsId: number, toIdx: number, e: DragEvent) {
  e.preventDefault();
  const src = dragSrc.value;
  dragSrc.value = null;
  dragOverKey.value = null;
  if (!src || src.wsId !== wsId || src.fromIdx === toIdx) return;
  termTabs.reorder(wsId, src.fromIdx, toIdx);
}

function onDragEnd() {
  dragSrc.value = null;
  dragOverKey.value = null;
}

function mimeForPath(path: string): string {
  const ext = path.split('.').pop()?.toLowerCase() ?? '';
  if (ext === 'svg') return 'image/svg+xml';
  if (ext === 'ico') return 'image/x-icon';
  if (ext === 'jpg' || ext === 'jpeg') return 'image/jpeg';
  return 'image/png';
}

async function pickIcon(id: number) {
  const selected = await openDialog({
    multiple: false,
    filters: [{ name: 'Image', extensions: ['png', 'jpg', 'jpeg', 'svg', 'ico'] }],
  });
  if (!selected || typeof selected !== 'string') return;
  const b64 = await invoke<string>('read_file_base64', { path: selected });
  const dataUrl = `data:${mimeForPath(selected)};base64,${b64}`;
  store.setIcon(id, dataUrl);
}

// Activate a terminal from the nested sidebar list. Switch to the workspace
// first if needed; its Terminal stays mounted while opened, so the request
// reaches it once active.
function selectTab(ws: Workspace, tabId: number) {
  if (store.active?.id !== ws.id) store.open(ws);
  nextTick(() => termTabs.activate(ws.id, tabId));
}

function addTab(ws: Workspace) {
  if (store.active?.id !== ws.id) store.open(ws);
  nextTick(() => termTabs.add(ws.id));
}

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

/* Nested terminal list */
.ws-group { margin-bottom: 2px; }

.ws-terminals {
  margin: 1px 4px 4px 22px;
  display: flex;
  flex-direction: column;
  gap: 1px;
  border-left: 1px solid var(--border);
  padding-left: 6px;
}

.ws-term {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 8px;
  border-radius: 3px;
  cursor: pointer;
  color: var(--text-secondary);
  position: relative;
}
.ws-term:hover { background: var(--bg-hover); color: var(--text-primary); }
.ws-term.active { background: var(--bg-selected); color: var(--text-primary); }

.ws-term-icon { color: var(--text-muted); flex-shrink: 0; }
.ws-term-icon.agent { color: var(--accent); }
.ws-term.active .ws-term-icon { color: var(--accent); }

.ws-term-label {
  flex: 1;
  min-width: 0;
  font-size: 11px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.status-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
  position: relative;
}
.status-dot.status-running {
  background: transparent;
  border-radius: 0;
  width: auto;
  height: auto;
  color: #f97316;
  font-size: 11px;
  line-height: 1;
  font-family: monospace;
}
.status-dot.status-waiting { background: #3b82f6; }
.status-dot.status-done    { background: #84cc16; }
/* review = agent finished while you weren't watching; persists until seen */
.status-dot.status-review {
  background: #22c55e;
  box-shadow: 0 0 0 0 rgba(34, 197, 94, 0.6);
  animation: review-pulse 1.8s ease-out infinite;
}
@keyframes review-pulse {
  0%   { box-shadow: 0 0 0 0 rgba(34, 197, 94, 0.6); }
  70%  { box-shadow: 0 0 0 5px rgba(34, 197, 94, 0); }
  100% { box-shadow: 0 0 0 0 rgba(34, 197, 94, 0); }
}

.ws-term-close {
  opacity: 0;
  color: var(--text-muted);
  flex-shrink: 0;
  border-radius: 3px;
  padding: 1px;
}
.ws-term:hover .ws-term-close { opacity: 0.5; }
.ws-term-close:hover { opacity: 1 !important; color: var(--red); }

.ws-term-add { color: var(--text-muted); }
.ws-term-add:hover { color: var(--text-secondary); }

.ws-term.drag-over { background: var(--bg-hover); outline: 1px solid var(--accent); outline-offset: -1px; }

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

/* branch pill + picker */
.ws-branch-pill {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  background: none;
  border: 1px solid var(--border);
  border-radius: 10px;
  color: var(--text-muted);
  cursor: pointer;
  font-size: 9px;
  font-family: var(--font-mono);
  padding: 1px 5px;
  margin-top: 2px;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.ws-branch-pill:hover { color: var(--text-secondary); border-color: #444; background: var(--bg-hover); }
.ws-item.active .ws-branch-pill { border-color: var(--accent); color: var(--accent); }

.branch-picker {
  margin: 0 4px 6px 22px;
  border: 1px solid var(--border);
  border-radius: 5px;
  background: var(--bg-base);
  overflow: hidden;
}

.branch-filter {
  width: 100%;
  background: transparent;
  border: none;
  border-bottom: 1px solid var(--border);
  color: var(--text-primary);
  font-size: 11px;
  outline: none;
  padding: 6px 8px;
  box-sizing: border-box;
}
.branch-filter::placeholder { color: var(--text-muted); }

.branch-list {
  max-height: 140px;
  overflow-y: auto;
}

.branch-item {
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 5px 8px;
  font-size: 11px;
  font-family: var(--font-mono);
  color: var(--text-secondary);
  cursor: pointer;
}
.branch-item:hover { background: var(--bg-hover); color: var(--text-primary); }
.branch-item.branch-current { color: var(--accent); }
.branch-item.branch-create { color: var(--text-muted); font-style: italic; }
.branch-item.branch-create:hover { color: var(--text-primary); background: var(--bg-hover); }
.branch-check { margin-left: auto; color: var(--accent); }
.branch-loading { color: var(--text-muted); font-style: italic; }
.branch-empty { color: var(--text-muted); font-size: 10px; padding: 8px; text-align: center; }

.ws-icon-wrap {
  position: relative;
  width: 14px;
  height: 14px;
  flex-shrink: 0;
  cursor: pointer;
  border-radius: 3px;
  overflow: hidden;
  display: flex;
  align-items: center;
  justify-content: center;
}
.ws-icon-wrap:hover::after {
  content: '';
  position: absolute;
  inset: 0;
  background: rgba(255,255,255,0.15);
  border-radius: 3px;
}
.ws-custom-icon {
  width: 14px;
  height: 14px;
  object-fit: cover;
  border-radius: 2px;
}
</style>
