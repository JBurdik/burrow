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

  async function load() {
    workspaces.value = await invoke<Workspace[]>("list_workspaces");
  }

  async function create(name: string, path: string): Promise<Workspace> {
    const ws = await invoke<Workspace>("create_workspace", { name, path });
    await load();
    return ws;
  }

  async function remove(id: number) {
    await invoke("delete_workspace", { id });
    workspaces.value = workspaces.value.filter((w) => w.id !== id);
    if (active.value?.id === id) active.value = null;
  }

  async function open(ws: Workspace) {
    await invoke("touch_workspace", { id: ws.id });
    active.value = ws;
  }

  function close() { active.value = null; }

  return { workspaces, active, load, create, remove, open, close };
});
