import { defineStore } from "pinia";
import { ref, watch } from "vue";

export interface AgentConfig {
  id: string;
  name: string;
  command: string;
  color: string;
}

const STORAGE_KEY = "agentic-ide.agents";

const DEFAULTS: AgentConfig[] = [
  { id: "claude", name: "Claude Code", command: "claude", color: "#a78bfa" },
  { id: "codex", name: "Codex", command: "codex", color: "#34d399" },
  { id: "gh", name: "GitHub CLI", command: "gh", color: "#60a5fa" },
];

function load(): AgentConfig[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [...DEFAULTS];
    const parsed = JSON.parse(raw);
    if (Array.isArray(parsed) && parsed.length) return parsed;
  } catch {
    /* fall through to defaults */
  }
  return [...DEFAULTS];
}

let counter = 0;
function makeId(): string {
  counter++;
  return `agent-${counter}-${counter * 7 + 13}`;
}

export const useAgentsStore = defineStore("agents", () => {
  const agents = ref<AgentConfig[]>(load());

  watch(
    agents,
    (val) => localStorage.setItem(STORAGE_KEY, JSON.stringify(val)),
    { deep: true },
  );

  function add(name: string, command: string, color: string) {
    agents.value.push({ id: makeId(), name, command, color });
  }

  function update(id: string, patch: Partial<Omit<AgentConfig, "id">>) {
    const a = agents.value.find((x) => x.id === id);
    if (a) Object.assign(a, patch);
  }

  function remove(id: string) {
    agents.value = agents.value.filter((x) => x.id !== id);
  }

  function reset() {
    agents.value = [...DEFAULTS];
  }

  return { agents, add, update, remove, reset };
});
