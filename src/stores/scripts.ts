import { defineStore } from "pinia";
import { ref, watch } from "vue";

// A Script is an ORDERED LIST OF STEPS run SEQUENTIALLY in one terminal tab.
// Steps are chained into a single shell command line:
//   continueOnError = false → "cmd1 && cmd2"  (next runs only if prev SUCCEEDED)
//   continueOnError = true  → "cmd1 ; cmd2"   (next runs regardless)
export interface Script {
  id: string;
  name: string;
  steps: string[];
  continueOnError: boolean;
  icon?: string;
  color?: string;
}

const GLOBAL_KEY = "agentic-ide.scripts.global";
const REPO_KEY = "agentic-ide.scripts.repo";

function loadGlobal(): Script[] {
  try {
    const raw = localStorage.getItem(GLOBAL_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      if (Array.isArray(parsed)) return parsed.map(normalize);
    }
  } catch {
    /* fall through */
  }
  return [];
}

function loadRepo(): Record<number, Script[]> {
  try {
    const raw = localStorage.getItem(REPO_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      if (parsed && typeof parsed === "object") {
        const out: Record<number, Script[]> = {};
        for (const [k, v] of Object.entries(parsed)) {
          if (Array.isArray(v)) out[Number(k)] = v.map(normalize);
        }
        return out;
      }
    }
  } catch {
    /* fall through */
  }
  return {};
}

function normalize(s: any): Script {
  return {
    id: String(s?.id ?? makeId()),
    name: String(s?.name ?? "Script"),
    steps: Array.isArray(s?.steps) ? s.steps.map((x: any) => String(x)) : [],
    continueOnError: !!s?.continueOnError,
    icon: s?.icon,
    color: s?.color,
  };
}

let counter = 0;
function makeId(): string {
  counter++;
  return `script-${counter}-${counter * 7 + 13}-${counter * 31 + 5}`;
}

const PALETTE = ["#60a5fa", "#34d399", "#a78bfa", "#f472b6", "#fbbf24", "#22d3ee"];

export const useScriptsStore = defineStore("scripts", () => {
  const globalScripts = ref<Script[]>(loadGlobal());
  // Per-workspace scripts keyed by workspace id.
  const repoScripts = ref<Record<number, Script[]>>(loadRepo());

  watch(globalScripts, (v) => localStorage.setItem(GLOBAL_KEY, JSON.stringify(v)), { deep: true });
  watch(repoScripts, (v) => localStorage.setItem(REPO_KEY, JSON.stringify(v)), { deep: true });

  function blank(idx: number): Script {
    return {
      id: makeId(),
      name: "New Script",
      steps: [""],
      continueOnError: false,
      color: PALETTE[idx % PALETTE.length],
    };
  }

  // ── Global ──────────────────────────────────────────────────────────────
  function addGlobal() {
    globalScripts.value.push(blank(globalScripts.value.length));
  }
  function updateGlobal(id: string, patch: Partial<Omit<Script, "id">>) {
    const s = globalScripts.value.find((x) => x.id === id);
    if (s) Object.assign(s, patch);
  }
  function removeGlobal(id: string) {
    globalScripts.value = globalScripts.value.filter((x) => x.id !== id);
  }

  // ── Per-repo ────────────────────────────────────────────────────────────
  function addRepo(wsId: number) {
    const list = repoScripts.value[wsId] ?? (repoScripts.value[wsId] = []);
    list.push(blank(list.length));
  }
  function updateRepo(wsId: number, id: string, patch: Partial<Omit<Script, "id">>) {
    const s = repoScripts.value[wsId]?.find((x) => x.id === id);
    if (s) Object.assign(s, patch);
  }
  function removeRepo(wsId: number, id: string) {
    const list = repoScripts.value[wsId];
    if (!list) return;
    repoScripts.value[wsId] = list.filter((x) => x.id !== id);
  }

  // Merged view for a workspace: repo scripts first, then globals; dedupe by id,
  // repo wins.
  function scriptsFor(wsId: number | null | undefined): Script[] {
    const repo = (wsId != null && repoScripts.value[wsId]) || [];
    const seen = new Set(repo.map((s) => s.id));
    return [...repo, ...globalScripts.value.filter((s) => !seen.has(s.id))];
  }

  // Join steps into a single shell line. && (stop on first failure) by default;
  // ; (continue regardless) when continueOnError is set.
  function commandLine(s: Script): string {
    const steps = s.steps.map((x) => x.trim()).filter(Boolean);
    return steps.join(s.continueOnError ? " ; " : " && ");
  }

  return {
    globalScripts,
    repoScripts,
    addGlobal,
    updateGlobal,
    removeGlobal,
    addRepo,
    updateRepo,
    removeRepo,
    scriptsFor,
    commandLine,
  };
});
