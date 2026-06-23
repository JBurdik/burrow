import { defineStore } from "pinia";
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

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

function normalize(s: Record<string, unknown>): Script {
  return {
    id: String(s?.id ?? makeId()),
    name: String(s?.name ?? "Script"),
    steps: Array.isArray(s?.steps) ? (s.steps as unknown[]).map((x) => String(x)) : [],
    continueOnError: !!s?.continueOnError,
    icon: s?.icon != null ? String(s.icon) : undefined,
    color: s?.color != null ? String(s.color) : undefined,
  };
}

let counter = 0;
function makeId(): string {
  counter++;
  return `script-${counter}-${counter * 7 + 13}-${counter * 31 + 5}`;
}

const PALETTE = ["#60a5fa", "#34d399", "#a78bfa", "#f472b6", "#fbbf24", "#22d3ee"];

function parseTomlValue(val: string): unknown {
  if (val === "true") return true;
  if (val === "false") return false;
  if (val.startsWith("[") && val.endsWith("]")) {
    const inner = val.slice(1, -1).trim();
    if (!inner) return [];
    // Simple CSV split respecting quoted strings
    const items: unknown[] = [];
    let cur = "";
    let inQ = false;
    for (let i = 0; i < inner.length; i++) {
      const ch = inner[i];
      if (ch === '"' && inner[i - 1] !== "\\") { inQ = !inQ; continue; }
      if (ch === "," && !inQ) { items.push(cur); cur = ""; continue; }
      cur += ch;
    }
    if (cur.trim()) items.push(cur.trim());
    return items;
  }
  if ((val.startsWith('"') && val.endsWith('"')) || (val.startsWith("'") && val.endsWith("'"))) {
    return val.slice(1, -1).replace(/\\"/g, '"').replace(/\\\\/g, "\\");
  }
  return val;
}

function parseScriptsToml(content: string): Script[] {
  const scripts: Script[] = [];
  const sections = content.split(/^\[\[scripts\]\]\s*$/m).slice(1);
  for (const section of sections) {
    const raw: Record<string, unknown> = {};
    for (const line of section.split("\n")) {
      const m = line.match(/^\s*(\w+)\s*=\s*(.+?)\s*$/);
      if (!m) continue;
      raw[m[1]] = parseTomlValue(m[2].trim());
    }
    if (raw.id || raw.name) scripts.push(normalize(raw));
  }
  return scripts;
}

function escapeToml(s: string): string {
  return s.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}

function serializeScriptsToml(scripts: Script[]): string {
  return scripts.map((s) => {
    const stepsArr = "[" + s.steps.map((x) => `"${escapeToml(x)}"`).join(", ") + "]";
    let block = `[[scripts]]\n`;
    block += `id = "${escapeToml(s.id)}"\n`;
    block += `name = "${escapeToml(s.name)}"\n`;
    block += `steps = ${stepsArr}\n`;
    block += `continueOnError = ${s.continueOnError}\n`;
    if (s.color) block += `color = "${escapeToml(s.color)}"\n`;
    if (s.icon) block += `icon = "${escapeToml(s.icon)}"\n`;
    return block;
  }).join("\n");
}

export const useScriptsStore = defineStore("scripts", () => {
  const scriptsCache = ref<Record<string, Script[]>>({});

  async function loadForPath(workspacePath: string): Promise<Script[]> {
    try {
      const content = await invoke<string>("read_text_file", {
        path: workspacePath + "/.burrow/config.toml",
      });
      const parsed = parseScriptsToml(content);
      scriptsCache.value[workspacePath] = parsed;
      return parsed;
    } catch {
      scriptsCache.value[workspacePath] = [];
      return [];
    }
  }

  async function saveForPath(workspacePath: string): Promise<void> {
    const scripts = scriptsCache.value[workspacePath] ?? [];
    const content = serializeScriptsToml(scripts);
    await invoke("write_text_file", {
      path: workspacePath + "/.burrow/config.toml",
      content,
    });
  }

  function scriptsFor(workspacePath: string | null | undefined): Script[] {
    if (!workspacePath) return [];
    return scriptsCache.value[workspacePath] ?? [];
  }

  function addScript(workspacePath: string): void {
    const list = scriptsCache.value[workspacePath] ?? [];
    list.push({
      id: makeId(),
      name: "New Script",
      steps: [""],
      continueOnError: false,
      color: PALETTE[list.length % PALETTE.length],
    });
    scriptsCache.value[workspacePath] = list;
    saveForPath(workspacePath);
  }

  function updateScript(
    workspacePath: string,
    id: string,
    patch: Partial<Omit<Script, "id">>,
  ): void {
    const s = (scriptsCache.value[workspacePath] ?? []).find((x) => x.id === id);
    if (s) Object.assign(s, patch);
    saveForPath(workspacePath);
  }

  function removeScript(workspacePath: string, id: string): void {
    const list = scriptsCache.value[workspacePath];
    if (!list) return;
    scriptsCache.value[workspacePath] = list.filter((x) => x.id !== id);
    saveForPath(workspacePath);
  }

  // Join steps into a single shell line. && (stop on first failure) by default;
  // ; (continue regardless) when continueOnError is set.
  function commandLine(s: Script): string {
    const steps = s.steps.map((x) => x.trim()).filter(Boolean);
    return steps.join(s.continueOnError ? " ; " : " && ");
  }

  return {
    scriptsCache,
    scriptsFor,
    loadForPath,
    addScript,
    updateScript,
    removeScript,
    commandLine,
  };
});
