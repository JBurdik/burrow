import { defineStore } from "pinia";
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";

export interface Workspace {
  id: number;
  name: string;
  path: string;
  created_at: number;
  last_opened: number | null;
  parent_id?: number | null;
  worktree_branch?: string | null;
}

export const useWorkspaceStore = defineStore("workspace", () => {
  const workspaces = ref<Workspace[]>([]);
  const active = ref<Workspace | null>(null);
  // Workspaces opened this session — each keeps its Terminal (and PTYs) mounted
  // so switching between them never tears down running processes.
  const opened = ref<Workspace[]>([]);

  // Custom icons stored as data URLs in localStorage
  const icons = ref<Record<number, string>>({});

  // Persisted manual order of top-level workspaces (array of ids). The DB has no
  // sort column, so the user's drag order lives in localStorage and is applied as
  // a sort over the raw list. Unknown ids (newly created) sort to the end.
  const ORDER_KEY = "burrow.ws.order";
  const order = ref<number[]>(_loadOrder());

  function _loadOrder(): number[] {
    try { return JSON.parse(localStorage.getItem(ORDER_KEY) || "[]"); }
    catch { return []; }
  }
  function _saveOrder() {
    localStorage.setItem(ORDER_KEY, JSON.stringify(order.value));
  }

  // Top-level repo workspaces (no parent), in the user's manual order. Worktrees
  // are nested under their parent.
  const topLevel = computed(() => {
    const tops = workspaces.value.filter((w) => !w.parent_id);
    const pos = new Map(order.value.map((id, i) => [id, i]));
    return [...tops].sort((a, b) => {
      const pa = pos.has(a.id) ? pos.get(a.id)! : Infinity;
      const pb = pos.has(b.id) ? pos.get(b.id)! : Infinity;
      if (pa !== pb) return pa - pb;
      return a.id - b.id; // stable fallback for ids not yet in the order list
    });
  });

  // Move a top-level workspace from one visible position to another, persisting
  // the new order. Indices are into `topLevel`.
  function reorderTopLevel(from: number, to: number) {
    const ids = topLevel.value.map((w) => w.id);
    if (from < 0 || from >= ids.length || to < 0 || to >= ids.length) return;
    const [moved] = ids.splice(from, 1);
    ids.splice(to, 0, moved);
    order.value = ids;
    _saveOrder();
  }
  // Worktree rows grouped by their parent repo id.
  const worktreesByParent = computed(() => {
    const m: Record<number, Workspace[]> = {};
    for (const w of workspaces.value) {
      if (w.parent_id) (m[w.parent_id] ??= []).push(w);
    }
    return m;
  });

  function _loadIcons() {
    try {
      const stored = localStorage.getItem("ws-icons");
      if (stored) icons.value = JSON.parse(stored);
    } catch {}
  }

  function _saveIcons() {
    localStorage.setItem("ws-icons", JSON.stringify(icons.value));
  }

  function setIcon(id: number, dataUrl: string) {
    icons.value[id] = dataUrl;
    _saveIcons();
  }

  function clearIcon(id: number) {
    delete icons.value[id];
    _saveIcons();
  }

  async function load() {
    workspaces.value = await invoke<Workspace[]>("list_workspaces");
    _loadIcons();
  }

  async function create(name: string, path: string): Promise<Workspace> {
    const ws = await invoke<Workspace>("create_workspace", { name, path });
    await load();
    return ws;
  }

  async function createWorktree(
    parentId: number,
    branch: string,
    baseRef: string | null,
    path: string,
  ): Promise<Workspace> {
    const ws = await invoke<Workspace>("create_worktree", {
      parentId,
      branch,
      baseRef: baseRef || null,
      path,
    });
    await load();
    return ws;
  }

  async function removeWorktree(id: number, force = false) {
    await invoke("remove_worktree", { id, force });
    workspaces.value = workspaces.value.filter((w) => w.id !== id);
    opened.value = opened.value.filter((w) => w.id !== id);
    if (active.value?.id === id) active.value = null;
    clearIcon(id);
  }

  async function remove(id: number) {
    // Remove any child worktrees first so we don't leave dangling git worktrees
    // or orphaned rows pointing at a deleted parent.
    const children = worktreesByParent.value[id] || [];
    for (const wt of children) {
      try {
        await removeWorktree(wt.id);
      } catch {
        // best-effort: keep going so the parent can still be deleted
      }
    }
    await invoke("delete_workspace", { id });
    workspaces.value = workspaces.value.filter((w) => w.id !== id);
    opened.value = opened.value.filter((w) => w.id !== id);
    if (active.value?.id === id) active.value = null;
    clearIcon(id);
  }

  async function rename(id: number, name: string) {
    await invoke("rename_workspace", { id, name });
    const w = workspaces.value.find((x) => x.id === id);
    if (w) w.name = name;
    if (active.value?.id === id) active.value.name = name;
    const o = opened.value.find((x) => x.id === id);
    if (o) o.name = name;
  }

  async function open(ws: Workspace) {
    await invoke("touch_workspace", { id: ws.id });
    if (!opened.value.some((w) => w.id === ws.id)) opened.value.push(ws);
    active.value = ws;
  }

  // Mount a workspace's Terminal (so it reattaches sessions / syncs tabs) WITHOUT
  // making it active. Used to eager-mount worktrees under an expanded parent.
  function ensureOpen(ws: Workspace) {
    if (!opened.value.some((w) => w.id === ws.id)) opened.value.push(ws);
  }

  // Back to the picker: keep `opened` (and its live terminals) intact.
  function close() { active.value = null; }

  return {
    workspaces, active, opened, icons, topLevel, worktreesByParent,
    load, create, remove, rename, open, ensureOpen, close, setIcon, clearIcon,
    createWorktree, removeWorktree, reorderTopLevel,
  };
});
