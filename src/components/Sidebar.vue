<template>
  <aside class="sidebar">
    <div class="sidebar-header">
      <div class="header-title">
        <span class="header-label">Workspaces</span>
        <span v-if="store.topLevel.length" class="header-count">{{ store.topLevel.length }}</span>
      </div>
      <button class="icon-btn" title="Open folder" @click="pickFolder">
        <PhFolderPlus :size="15" />
      </button>
    </div>

    <div class="ws-list">
      <TransitionGroup name="ws-move" tag="div" class="ws-list-inner">
      <div
        v-for="(item, wsIdx) in store.topLevel"
        :key="item.id"
        class="ws-group"
        :data-reorder-idx="wsIdx"
        data-reorder-group="ws"
      >
        <div
          class="ws-item"
          :class="{
            active: store.active?.id === item.id,
            'drag-over': wsOverIdx === wsIdx && wsDragIdx !== wsIdx,
            dragging: wsDragIdx === wsIdx,
          }"
          @click="openWs(item)"
          @pointerdown="(e: PointerEvent) => wsDragDown(wsIdx, e, 'ws')"
          @contextmenu.prevent.stop="openCtxMenu(item, $event)"
        >
          <button class="ws-caret" :title="isCollapsed(item.id) ? 'Expand' : 'Collapse'" data-no-drag @click.stop="toggleCollapse(item.id)">
            <PhCaretRight v-if="isCollapsed(item.id)" :size="11" weight="bold" />
            <PhCaretDown v-else :size="11" weight="bold" />
          </button>
          <div class="ws-icon-wrap" title="Change icon via right-click menu">
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
              data-no-drag
              @click.stop="openBranchPicker(item, $event)"
            >
              <PhGitBranch :size="9" />
              <span>{{ wsBranch[item.id] }}</span>
            </button>
          </div>
          <button class="ws-delete" title="Remove" data-no-drag @click.stop="store.remove(item.id)">
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

        <TransitionGroup
          v-if="!isCollapsed(item.id) && termTabs.tabsByWs[item.id]?.length"
          name="ws-move"
          tag="div"
          class="ws-terminals"
        >
          <div
            v-for="(tab, tabIdx) in termTabs.tabsByWs[item.id]"
            :key="tab.id"
            class="ws-term"
            :data-reorder-idx="tabIdx"
            :data-reorder-group="String(item.id)"
            :class="{
              active:
                store.active?.id === item.id && termTabs.activeByWs[item.id] === tab.id,
              'drag-over':
                tabDragGroup === String(item.id) && tabOverIdx === tabIdx && tabDragIdx !== tabIdx,
              dragging: tabDragGroup === String(item.id) && tabDragIdx === tabIdx,
            }"
            @click.stop="selectTab(item, tab.id)"
            @pointerdown="(e: PointerEvent) => tabDragDown(tabIdx, e, String(item.id))"
          >
            <PhRobot v-if="tab.isAgent" :size="11" class="ws-term-icon agent" />
            <PhTerminal v-else :size="11" class="ws-term-icon" />
            <span class="ws-term-label">{{ tab.title }}</span>
            <span
              v-if="(tab.leafCount ?? 1) > 1"
              class="ws-term-split-count"
              :title="`${tab.leafCount} panes`"
            >{{ tab.leafCount }}</span>
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
              data-no-drag
              @click.stop="termTabs.close(item.id, tab.id)"
            />
          </div>
        </TransitionGroup>

        <!-- Worktrees subsection -->
        <div v-if="!isCollapsed(item.id)" class="ws-worktrees">
          <div class="ws-worktree-head">
            <span>Worktrees</span>
            <button class="icon-btn" title="New worktree" @click.stop="openWtDialog(item)">
              <PhPlus :size="11" />
            </button>
          </div>
          <template v-for="wt in store.worktreesByParent[item.id] || []" :key="wt.id">
            <div
              class="ws-term ws-worktree"
              :class="{ active: store.active?.id === wt.id }"
              :title="wt.path"
              @click.stop="selectWorktree(wt)"
              @contextmenu.prevent.stop="openWtCtxMenu(wt, $event)"
            >
              <PhGitBranch :size="11" class="ws-term-icon" />
              <span class="ws-term-label">{{ wt.worktree_branch || wt.name }}</span>
              <span
                v-if="aggStatus(wt.id)"
                class="status-dot"
                :class="`status-${aggStatus(wt.id)}`"
              >{{ aggStatus(wt.id) === 'running' ? spinnerFrame : '' }}</span>
            </div>

            <!-- worktree's own terminal tabs -->
            <div v-if="termTabs.tabsByWs[wt.id]?.length" class="ws-terminals ws-wt-terminals">
              <div
                v-for="tab in termTabs.tabsByWs[wt.id]"
                :key="tab.id"
                class="ws-term"
                :class="{
                  active:
                    store.active?.id === wt.id && termTabs.activeByWs[wt.id] === tab.id,
                }"
                @click.stop="selectTab(wt, tab.id)"
              >
                <PhRobot v-if="tab.isAgent" :size="11" class="ws-term-icon agent" />
                <PhTerminal v-else :size="11" class="ws-term-icon" />
                <span class="ws-term-label">{{ tab.title }}</span>
                <span
                  v-if="(tab.leafCount ?? 1) > 1"
                  class="ws-term-split-count"
                  :title="`${tab.leafCount} panes`"
                >{{ tab.leafCount }}</span>
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
                  data-no-drag
                  @click.stop="termTabs.close(wt.id, tab.id)"
                />
              </div>
            </div>
          </template>
        </div>
      </div>
      </TransitionGroup>

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

    <!-- Workspace context menu -->
    <Teleport to="body">
      <div
        v-if="ctxMenu"
        class="ctx-menu"
        :style="{ left: ctxMenu.x + 'px', top: ctxMenu.y + 'px' }"
        @click.stop
        @contextmenu.prevent.stop
      >
        <button class="ctx-item" @click="ctxOpen()"><PhFolderOpen :size="13" />Open</button>
        <button class="ctx-item" @click="ctxRename()"><PhPencilSimple :size="13" />Rename…</button>
        <button class="ctx-item" @click="ctxIcon()"><PhImage :size="13" />Change icon…</button>
        <button v-if="store.icons[ctxMenu.wsId]" class="ctx-item" @click="ctxClearIcon()"><PhImage :size="13" />Reset icon</button>
        <div class="ctx-sep" />
        <button class="ctx-item ctx-danger" @click="ctxRemove()"><PhTrash :size="13" />Remove</button>
      </div>
    </Teleport>

    <!-- Worktree context menu -->
    <Teleport to="body">
      <div
        v-if="wtCtxMenu"
        class="ctx-menu"
        :style="{ left: wtCtxMenu.x + 'px', top: wtCtxMenu.y + 'px' }"
        @click.stop
        @contextmenu.prevent.stop
      >
        <button class="ctx-item" @click="wtCtxOpen()"><PhFolderOpen :size="13" />Open</button>
        <div class="ctx-sep" />
        <button class="ctx-item ctx-danger" @click="wtCtxRemove()"><PhTrash :size="13" />Remove worktree</button>
      </div>
    </Teleport>

    <!-- Rename dialog -->
    <div class="dialog-overlay" v-if="renameId !== null" @click.self="renameId = null">
      <div class="dialog">
        <h3>Rename workspace</h3>
        <input
          v-model="renameName"
          class="dialog-input"
          placeholder="Workspace name"
          @keydown.enter="confirmRename"
          @keydown.esc="renameId = null"
          ref="renameInputEl"
        />
        <div class="dialog-actions">
          <button class="btn-secondary" @click="renameId = null">Cancel</button>
          <button class="btn-primary" @click="confirmRename" :disabled="!renameName.trim()">Rename</button>
        </div>
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

    <!-- New worktree dialog -->
    <div class="dialog-overlay" v-if="wtParent" @click.self="closeWtDialog">
      <div class="dialog">
        <h3>New worktree — {{ wtParent?.name }}</h3>
        <label class="wt-label">Branch</label>
        <input
          v-model="wtBranch"
          class="dialog-input"
          placeholder="feature/my-branch"
          list="wt-base-branches"
          spellcheck="false"
          @keydown.enter="confirmWorktree"
          @keydown.esc="closeWtDialog"
          ref="wtBranchEl"
        />
        <label class="wt-label">Base branch <span class="wt-hint">(only for a new branch)</span></label>
        <input
          v-model="wtBase"
          class="dialog-input"
          placeholder="defaults to current HEAD"
          list="wt-base-branches"
          spellcheck="false"
          @keydown.enter="confirmWorktree"
          @keydown.esc="closeWtDialog"
        />
        <datalist id="wt-base-branches">
          <option v-for="b in wtBaseList" :key="b" :value="b" />
        </datalist>
        <p class="dialog-path">{{ wtTargetPath }}</p>
        <p v-if="wtError" class="wt-error">{{ wtError }}</p>
        <div class="dialog-actions">
          <button class="btn-secondary" @click="closeWtDialog">Cancel</button>
          <button class="btn-primary" @click="confirmWorktree" :disabled="!wtBranch.trim() || wtBusy">
            {{ wtBusy ? "Creating…" : "Create" }}
          </button>
        </div>
      </div>
    </div>
  </aside>
</template>

<script setup lang="ts">
import { ref, computed, nextTick, onMounted, watch } from "vue";
import {
  PhFolderPlus,
  PhFolder,
  PhFolderOpen,
  PhX,
  PhTerminal,
  PhRobot,
  PhPlus,
  PhGitBranch,
  PhPencilSimple,
  PhImage,
  PhTrash,
  PhCaretRight,
  PhCaretDown,
} from "@phosphor-icons/vue";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { useWorkspaceStore, type Workspace } from "@/stores/workspace";
import { useTerminalTabsStore } from "@/stores/terminalTabs";
import { useUIStore } from "@/stores/ui";
import { spinnerFrame } from "@/lib/spinner";
import { usePointerReorder } from "@/composables/usePointerReorder";
import { aggregateStatus, type TermStatus } from "@/lib/terminalStatus";

const store = useWorkspaceStore();
const termTabs = useTerminalTabsStore();
const ui = useUIStore();

// ── collapse / expand per workspace ──────────────────────────────────────────
const COLLAPSE_KEY = "burrow.ws.collapsed";
const collapsed = ref<Record<number, boolean>>(loadCollapsed());

function loadCollapsed(): Record<number, boolean> {
  try { return JSON.parse(localStorage.getItem(COLLAPSE_KEY) || "{}"); }
  catch { return {}; }
}
// Default collapsed: a workspace with no stored value reads as collapsed.
function isCollapsed(id: number): boolean {
  return collapsed.value[id] !== false;
}
function setCollapsed(id: number, val: boolean) {
  collapsed.value[id] = val;
  localStorage.setItem(COLLAPSE_KEY, JSON.stringify(collapsed.value));
}
function toggleCollapse(id: number) {
  const next = !isCollapsed(id);
  setCollapsed(id, next);
  // Expanding a never-opened workspace shows no tabs until its Terminal mounts.
  // Worktrees need the same eager mount, else their nested rows list no terminals.
  if (!next) { const w = store.workspaces.find((x) => x.id === id); if (w) store.open(w); mountWorktrees(id); }
}

// Item click: toggle collapse. Expanding also opens the workspace.
function openWs(item: Workspace) {
  const next = !isCollapsed(item.id);
  setCollapsed(item.id, next);
  if (!next) { store.open(item); mountWorktrees(item.id); }
}

// Worktree row click: just open/activate it (its terminals are already mounted
// via mountWorktrees; no per-worktree collapse state to toggle).
function selectWorktree(wt: Workspace) {
  store.open(wt);
}

// Mount every worktree of an expanded parent so each one's Terminal restores its
// saved/daemon sessions into tabsByWs — otherwise the nested rows show no tabs.
function mountWorktrees(parentId: number) {
  for (const wt of store.worktreesByParent[parentId] || []) store.ensureOpen(wt);
}

// Aggregate status of a workspace's tabs (highest-priority wins). Drives the
// worktree row dot so a finished/working agent is visible without expanding.
// Returns null for idle (no dot to show).
function aggStatus(id: number): TermStatus | null {
  const tabs = termTabs.tabsByWs[id] || [];
  const s = aggregateStatus(tabs, (t) => t.status);
  return s === "idle" ? null : s;
}

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

// Parse `git branch --list` into a plain branch-name array. Shared by the branch
// picker and the new-worktree base-branch field.
async function listBranches(path: string): Promise<string[]> {
  try {
    const out = await invoke<GitOutput>("run_git", { cwd: path, args: ["branch", "--list"] });
    if (out.code === 0) {
      return out.stdout.split("\n")
        .map(l => l.replace(/^\*?\s+/, "").trim())
        .filter(Boolean);
    }
  } catch {}
  return [];
}

async function openBranchPicker(ws: Workspace, e: MouseEvent) {
  e.stopPropagation();
  if (showBranchPicker.value === ws.id) { showBranchPicker.value = null; return; }
  branchLoading.value = true;
  branchError.value = "";
  branchFilter.value = "";
  showBranchPicker.value = ws.id;
  try {
    branchList.value = await listBranches(ws.path);
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
  store.workspaces.forEach(ws => {
    fetchBranch(ws);
    // Persisted-expanded workspaces must reopen so their Terminal mounts and
    // tabsByWs populates — otherwise the row looks expanded but lists no tabs
    // until a manual collapse+expand fires store.open().
    if (!isCollapsed(ws.id)) { store.open(ws); mountWorktrees(ws.id); }
  });
  document.addEventListener("click", () => { showBranchPicker.value = null; ctxMenu.value = null; wtCtxMenu.value = null; });
});
watch(() => store.workspaces, (wss) => wss.forEach(ws => {
  if (!(ws.id in wsBranch.value)) fetchBranch(ws);
  // Reopen persisted-expanded parents (and mount their worktrees) once the
  // async load() populates the list — Sidebar onMounted may have run while empty.
  if (!ws.parent_id && !isCollapsed(ws.id)) { store.ensureOpen(ws); mountWorktrees(ws.id); }
}), { deep: true });

// ── drag-to-reorder ──────────────────────────────────────────────────────────
// Pointer-based (not HTML5 DnD): Tauri's native drag-drop handler swallows the
// webview's drop events. Group = workspace id, so a tab only reorders within its
// own project's list.
const {
  dragIdx: tabDragIdx,
  overIdx: tabOverIdx,
  dragGroup: tabDragGroup,
  down: tabDragDown,
} = usePointerReorder((from, to, group) => {
  if (group != null) termTabs.reorder(Number(group), from, to);
});

// Top-level workspace reorder. Distinct group "ws" so a workspace drag can only
// target other workspace rows — never a nested terminal row (which carries a
// numeric workspace-id group).
const {
  dragIdx: wsDragIdx,
  overIdx: wsOverIdx,
  down: wsDragDown,
} = usePointerReorder((from, to) => store.reorderTopLevel(from, to));

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

// ── context menu ───────────────────────────────────────────────────────────
const ctxMenu = ref<{ wsId: number; x: number; y: number } | null>(null);

function openCtxMenu(item: Workspace, e: MouseEvent) {
  // Clamp x so the menu (≈180px) never spills past the right edge.
  const x = Math.min(e.clientX, window.innerWidth - 190);
  ctxMenu.value = { wsId: item.id, x, y: e.clientY };
}
function ctxItem(): Workspace | undefined {
  return store.workspaces.find((w) => w.id === ctxMenu.value?.wsId);
}
function ctxOpen() {
  const w = ctxItem();
  ctxMenu.value = null;
  if (w) store.open(w);
}
function ctxRename() {
  const w = ctxItem();
  ctxMenu.value = null;
  if (w) startRename(w);
}
async function ctxIcon() {
  const id = ctxMenu.value?.wsId;
  ctxMenu.value = null;
  if (id != null) await pickIcon(id);
}
function ctxClearIcon() {
  const id = ctxMenu.value?.wsId;
  ctxMenu.value = null;
  if (id != null) store.clearIcon(id);
}
function ctxRemove() {
  const id = ctxMenu.value?.wsId;
  ctxMenu.value = null;
  if (id != null) store.remove(id);
}

// ── worktree context menu ────────────────────────────────────────────────────
const wtCtxMenu = ref<{ wtId: number; x: number; y: number } | null>(null);

function openWtCtxMenu(wt: Workspace, e: MouseEvent) {
  const x = Math.min(e.clientX, window.innerWidth - 200);
  wtCtxMenu.value = { wtId: wt.id, x, y: e.clientY };
}
function wtCtxItem(): Workspace | undefined {
  return store.workspaces.find((w) => w.id === wtCtxMenu.value?.wtId);
}
function wtCtxOpen() {
  const w = wtCtxItem();
  wtCtxMenu.value = null;
  if (w) store.open(w);
}
async function wtCtxRemove() {
  const id = wtCtxMenu.value?.wtId;
  wtCtxMenu.value = null;
  if (id == null) return;
  try {
    await store.removeWorktree(id);
  } catch (err) {
    // Likely uncommitted changes — offer a forced removal.
    const msg = err instanceof Error ? err.message : String(err);
    if (confirm(`Could not remove worktree:\n\n${msg}\n\nForce remove (discards uncommitted changes)?`)) {
      try { await store.removeWorktree(id, true); }
      catch (e2) { alert(`Force remove failed:\n${e2 instanceof Error ? e2.message : e2}`); }
    }
  }
}

// ── new worktree dialog ──────────────────────────────────────────────────────
const wtParent = ref<Workspace | null>(null);
const wtBranch = ref("");
const wtBase = ref("");
const wtBaseList = ref<string[]>([]);
const wtBusy = ref(false);
const wtError = ref("");
const wtBranchEl = ref<HTMLInputElement>();

const wtTargetPath = computed(() => {
  if (!wtParent.value) return "";
  const repo = wtParent.value.path.split("/").filter(Boolean).pop() || "repo";
  const branch = wtBranch.value.trim() || "<branch>";
  return `${ui.worktreesDir}/${repo}/${branch}`;
});

async function openWtDialog(parent: Workspace) {
  wtParent.value = parent;
  wtBranch.value = "";
  wtBase.value = "";
  wtError.value = "";
  wtBaseList.value = [];
  await nextTick();
  wtBranchEl.value?.focus();
  wtBaseList.value = await listBranches(parent.path);
}

function closeWtDialog() {
  wtParent.value = null;
}

async function confirmWorktree() {
  const branch = wtBranch.value.trim();
  if (!wtParent.value || !branch || wtBusy.value) return;
  wtBusy.value = true;
  wtError.value = "";
  try {
    const ws = await store.createWorktree(wtParent.value.id, branch, wtBase.value.trim() || null, wtTargetPath.value);
    wtParent.value = null;
    store.open(ws);
  } catch (err) {
    wtError.value = err instanceof Error ? err.message : String(err);
  } finally {
    wtBusy.value = false;
  }
}

// ── rename dialog ──────────────────────────────────────────────────────────
const renameId = ref<number | null>(null);
const renameName = ref("");
const renameInputEl = ref<HTMLInputElement>();

async function startRename(w: Workspace) {
  renameId.value = w.id;
  renameName.value = w.name;
  await nextTick();
  renameInputEl.value?.focus();
  renameInputEl.value?.select();
}
async function confirmRename() {
  const name = renameName.value.trim();
  if (renameId.value === null || !name) return;
  await store.rename(renameId.value, name);
  renameId.value = null;
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
  width: var(--sidebar-width, 220px);
  flex: 0 0 var(--sidebar-width, 220px);
  background: var(--bg-panel);
  backdrop-filter: var(--backdrop-blur, none);
  -webkit-backdrop-filter: var(--backdrop-blur, none);
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
  padding: 11px 12px 9px;
  flex-shrink: 0;
}

.header-title {
  display: flex;
  align-items: center;
  gap: 7px;
}

.header-label {
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.1em;
  text-transform: uppercase;
  color: var(--text-secondary);
}

.header-count {
  font-size: 9px;
  font-weight: 700;
  color: var(--text-muted);
  background: var(--bg-hover);
  border-radius: 9px;
  padding: 1px 7px;
  line-height: 1.5;
}

.icon-btn {
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  padding: 4px;
  border-radius: 6px;
  transition: color .12s, background .12s;
}
.icon-btn:hover { color: var(--text-primary); background: var(--bg-hover); }
.icon-btn:active { transform: scale(0.92); }

.ws-list {
  flex: 1;
  overflow-y: auto;
  padding: 2px 0 8px;
}

.ws-item {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 7px 10px 7px 11px;
  cursor: pointer;
  border-radius: 8px;
  margin: 1px 6px;
  position: relative;
  transition: background .12s;
}
.ws-item::before {
  content: "";
  position: absolute;
  left: -2px;
  top: 50%;
  transform: translateY(-50%) scaleY(0);
  width: 3px;
  height: 18px;
  border-radius: 2px;
  background: var(--accent);
  transition: transform .15s ease;
}
.ws-item:hover { background: var(--bg-hover); }
.ws-item.active {
  background: color-mix(in srgb, var(--accent) 12%, transparent);
}
.ws-item.active::before { transform: translateY(-50%) scaleY(1); }

.ws-caret {
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  padding: 0;
  margin: 0 -3px 0 -5px;
  flex-shrink: 0;
  border-radius: 3px;
  opacity: 0.6;
  transition: opacity .12s, color .12s;
}
.ws-item:hover .ws-caret { opacity: 1; }
.ws-caret:hover { color: var(--text-primary); }

.ws-icon { color: #60a5fa; flex-shrink: 0; }
.ws-item.active .ws-icon { color: var(--accent); }

.ws-info {
  flex: 1;
  min-width: 0;
}

.ws-name {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  letter-spacing: -0.01em;
}

.ws-path {
  font-size: 10px;
  font-family: var(--font-mono);
  color: var(--text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  margin-top: 1px;
}

.ws-delete {
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  display: none;
  align-items: center;
  padding: 4px;
  border-radius: 6px;
  flex-shrink: 0;
  transition: color .12s, background .12s;
}
.ws-item:hover .ws-delete { display: flex; }
.ws-delete:hover { color: var(--red); background: color-mix(in srgb, var(--red) 14%, transparent); }

/* Nested terminal list */
.ws-group { margin-bottom: 1px; }

/* Worktrees subsection */
.ws-worktrees {
  margin: 0 8px 5px 24px;
  display: flex;
  flex-direction: column;
  gap: 1px;
  border-left: 1.5px solid var(--border);
  padding-left: 8px;
}
.ws-worktree-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 3px 8px 3px 4px;
  font-size: 9px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--text-muted);
}
.ws-worktree .ws-term-icon { color: #a78bfa; }
.ws-worktree.active .ws-term-icon { color: var(--accent); }

/* Terminal tabs nested under a worktree row — indented inside the worktree group */
.ws-wt-terminals {
  margin: 1px 0 3px 12px;
  border-left-color: color-mix(in srgb, #a78bfa 40%, transparent);
}

.wt-label {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-secondary);
  margin-bottom: -6px;
}
.wt-hint { font-weight: 400; color: var(--text-muted); }
.wt-error {
  font-size: 11px;
  color: var(--red);
  white-space: pre-wrap;
  word-break: break-word;
}

.ws-terminals {
  margin: 2px 8px 4px 24px;
  display: flex;
  flex-direction: column;
  gap: 1px;
  border-left: 1.5px solid var(--border);
  padding-left: 8px;
}

.ws-term {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 5px 8px;
  border-radius: 7px;
  cursor: pointer;
  color: var(--text-secondary);
  position: relative;
  transition: background .12s, color .12s;
}
.ws-term:hover { background: var(--bg-hover); color: var(--text-primary); }
.ws-term.active {
  background: color-mix(in srgb, var(--accent) 12%, transparent);
  color: var(--text-primary);
}

.ws-term-icon { color: var(--text-muted); flex-shrink: 0; }
.ws-term-icon.agent { color: var(--accent); }
.ws-term.active .ws-term-icon { color: var(--accent); }

.ws-term-label {
  flex: 1;
  min-width: 0;
  font-size: 11.5px;
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ws-term-split-count {
  flex-shrink: 0;
  min-width: 13px;
  height: 13px;
  padding: 0 4px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-size: 9px;
  font-weight: 600;
  line-height: 1;
  border-radius: 7px;
  background: rgba(255, 255, 255, 0.1);
  color: var(--text-muted);
}

/* Status dot styles are in src/styles/status-dots.css (shared with Terminal.vue).
   No local overrides needed here. */

.ws-term-close {
  opacity: 0;
  color: var(--text-muted);
  flex-shrink: 0;
  border-radius: 3px;
  padding: 1px;
}
.ws-term:hover .ws-term-close { opacity: 0.5; }
.ws-term-close:hover { opacity: 1 !important; color: var(--red); }

.ws-term.drag-over { background: var(--bg-hover); outline: 1px solid var(--accent); outline-offset: -1px; }
.ws-term.dragging { opacity: 0.4; }
.ws-term { touch-action: none; }

/* workspace row drag feedback */
.ws-item { touch-action: none; }
.ws-item.drag-over { outline: 1px solid var(--accent); outline-offset: -1px; }
.ws-item.drag-over::after {
  content: "";
  position: absolute;
  left: 8px;
  right: 8px;
  top: -2px;
  height: 2px;
  border-radius: 2px;
  background: var(--accent);
}
.ws-item.dragging { opacity: 0.45; }

/* FLIP move animation for reordering (workspaces + nested tabs) */
.ws-move-move { transition: transform .22s cubic-bezier(.2, .8, .2, 1); }

.ws-empty {
  font-size: 11.5px;
  color: var(--text-muted);
  text-align: center;
  padding: 40px 20px;
  line-height: 1.7;
  margin: 8px;
  border: 1px dashed var(--border);
  border-radius: 10px;
}

.sidebar-footer {
  border-top: 1px solid var(--border);
  padding: 8px;
  flex-shrink: 0;
}

.footer-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 7px;
  width: 100%;
  background: var(--bg-hover);
  border: 1px solid var(--border);
  border-radius: 8px;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 11.5px;
  font-weight: 600;
  padding: 8px 10px;
  transition: color .12s, border-color .12s, background .12s;
}
.footer-btn:hover {
  color: var(--accent);
  border-color: color-mix(in srgb, var(--accent) 45%, var(--border));
  background: color-mix(in srgb, var(--accent) 10%, transparent);
}
.footer-btn:active { transform: scale(0.985); }

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

/* context menu */
.ctx-menu {
  position: fixed;
  z-index: 1000;
  min-width: 170px;
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: 7px;
  padding: 4px;
  display: flex;
  flex-direction: column;
  gap: 1px;
  box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5);
}
.ctx-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  background: none;
  border: none;
  border-radius: 4px;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 12px;
  font-family: var(--font-ui);
  text-align: left;
  padding: 6px 10px;
}
.ctx-item:hover { background: var(--bg-hover); color: var(--text-primary); }
.ctx-item.ctx-danger:hover { color: var(--red); }
.ctx-sep { height: 1px; background: var(--border); margin: 3px 0; }

.ws-icon-wrap {
  position: relative;
  width: 26px;
  height: 26px;
  flex-shrink: 0;
  border-radius: 7px;
  overflow: hidden;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--bg-hover);
  transition: background .12s;
}
.ws-item.active .ws-icon-wrap {
  background: color-mix(in srgb, var(--accent) 18%, transparent);
}
.ws-custom-icon {
  width: 26px;
  height: 26px;
  object-fit: cover;
}
</style>
