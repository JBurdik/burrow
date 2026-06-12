import { ref, computed } from "vue";
import { defineStore } from "pinia";
import { invoke } from "@tauri-apps/api/core";

export interface ClaudeSession {
  id: number;
  workspaceId: number;
  claudeSessionId: string; // captured from stream-json system/init
  title: string;
  busy: boolean;
  messageCount: number;
}

const SESSIONS_KEY = "burrow.claude.sessions";
const ACTIVE_KEY = "burrow.claude.active";
const COUNTER_KEY = "burrow.claude.nextId";

function loadSessions(): ClaudeSession[] {
  try {
    const raw = localStorage.getItem(SESSIONS_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch { return []; }
}

function loadActive(): Record<number, number> {
  try {
    const raw = localStorage.getItem(ACTIVE_KEY);
    return raw ? JSON.parse(raw) : {};
  } catch { return {}; }
}

function loadCounter(): number {
  return parseInt(localStorage.getItem(COUNTER_KEY) ?? "1", 10);
}

export const useClaudeChatsStore = defineStore("claudeChats", () => {
  const sessions = ref<ClaudeSession[]>(loadSessions());
  const activeByWs = ref<Record<number, number>>(loadActive());
  let nextId = loadCounter();

  function persist() {
    const toSave = sessions.value.map((s) => ({ ...s, busy: false }));
    localStorage.setItem(SESSIONS_KEY, JSON.stringify(toSave));
    localStorage.setItem(ACTIVE_KEY, JSON.stringify(activeByWs.value));
    localStorage.setItem(COUNTER_KEY, String(nextId));
  }

  function sessionsForWs(workspaceId: number): ClaudeSession[] {
    return sessions.value.filter((s) => s.workspaceId === workspaceId);
  }

  function activeSession(workspaceId: number): ClaudeSession | undefined {
    const activeId = activeByWs.value[workspaceId];
    return sessions.value.find((s) => s.id === activeId && s.workspaceId === workspaceId);
  }

  // Create and activate a new session for this workspace.
  function create(workspaceId: number): ClaudeSession {
    const id = nextId++;
    const session: ClaudeSession = {
      id,
      workspaceId,
      claudeSessionId: "",
      title: `Chat ${sessionsForWs(workspaceId).length + 1}`,
      busy: false,
      messageCount: 0,
    };
    sessions.value.push(session);
    activeByWs.value[workspaceId] = id;
    persist();
    return session;
  }

  // Ensure at least one session exists for this workspace; return active.
  function ensureSession(workspaceId: number): ClaudeSession {
    const existing = sessionsForWs(workspaceId);
    if (existing.length === 0) return create(workspaceId);
    const active = activeSession(workspaceId);
    if (active) return active;
    activeByWs.value[workspaceId] = existing[0].id;
    persist();
    return existing[0];
  }

  function setActive(workspaceId: number, sessionId: number) {
    activeByWs.value[workspaceId] = sessionId;
    persist();
  }

  async function remove(id: number) {
    const s = sessions.value.find((x) => x.id === id);
    if (!s) return;
    await invoke("claude_stop", { id }).catch(() => {});
    sessions.value = sessions.value.filter((x) => x.id !== id);
    // If removed was active, fall back to first remaining for that ws.
    if (activeByWs.value[s.workspaceId] === id) {
      const remaining = sessionsForWs(s.workspaceId);
      if (remaining.length) activeByWs.value[s.workspaceId] = remaining[0].id;
      else delete activeByWs.value[s.workspaceId];
    }
    persist();
  }

  // Called by ClaudeChat.vue to sync live state back.
  function sync(id: number, patch: Partial<Pick<ClaudeSession, "busy" | "messageCount" | "claudeSessionId" | "title">>) {
    const s = sessions.value.find((x) => x.id === id);
    if (!s) return;
    Object.assign(s, patch);
    if (patch.claudeSessionId !== undefined || patch.title !== undefined || patch.messageCount !== undefined) {
      persist();
    }
  }

  // Sessions whose workspace is currently in ws.opened — used by App.vue for keep-alive mounting.
  // The caller filters by opened workspace ids.
  const allSessions = computed(() => sessions.value);

  return {
    sessions,
    activeByWs,
    allSessions,
    sessionsForWs,
    activeSession,
    create,
    ensureSession,
    setActive,
    remove,
    sync,
  };
});
