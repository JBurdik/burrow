import { defineStore } from "pinia";
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

export interface Workspace {
  id: number;
  name: string;
  path: string;
  created_at: number;
  last_opened: number | null;
}

export const useWorkspaceStore = defineStore("workspace", () => {
  const workspaces = ref<Workspace[]>([]);
  const active = ref<Workspace | null>(null);
  // Workspaces opened this session — each keeps its Terminal (and PTYs) mounted
  // so switching between them never tears down running processes.
  const opened = ref<Workspace[]>([]);

  // Custom icons stored as data URLs in localStorage
  const icons = ref<Record<number, string>>({});

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

  async function remove(id: number) {
    await invoke("delete_workspace", { id });
    workspaces.value = workspaces.value.filter((w) => w.id !== id);
    opened.value = opened.value.filter((w) => w.id !== id);
    if (active.value?.id === id) active.value = null;
    clearIcon(id);
  }

  async function open(ws: Workspace) {
    await invoke("touch_workspace", { id: ws.id });
    if (!opened.value.some((w) => w.id === ws.id)) opened.value.push(ws);
    active.value = ws;
  }

  // Back to the picker: keep `opened` (and its live terminals) intact.
  function close() { active.value = null; }

  return { workspaces, active, opened, icons, load, create, remove, open, close, setIcon, clearIcon };
});
