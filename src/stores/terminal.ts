import { defineStore } from "pinia";
import { ref } from "vue";

export interface TerminalLine {
  id: number;
  type: "input" | "output" | "error" | "info";
  content: string;
}

let lineId = 0;

export const useTerminalStore = defineStore("terminal", () => {
  const lines = ref<TerminalLine[]>([
    { id: lineId++, type: "info", content: "Burrow v0.1.0" },
    { id: lineId++, type: "info", content: "Type a command or ask the AI agent..." },
    { id: lineId++, type: "output", content: "" },
  ]);

  const currentInput = ref("");
  const cwd = ref("~/code/agentic-ide");

  function addLine(type: TerminalLine["type"], content: string) {
    lines.value.push({ id: lineId++, type, content });
  }

  function submit() {
    const cmd = currentInput.value.trim();
    if (!cmd) return;
    addLine("input", `${cwd.value} $ ${cmd}`);
    currentInput.value = "";
    handleCommand(cmd);
  }

  function handleCommand(cmd: string) {
    if (cmd === "clear") {
      lines.value = [];
      return;
    }
    if (cmd === "help") {
      addLine("output", "Commands: clear, help, pwd, ls");
      return;
    }
    if (cmd === "pwd") {
      addLine("output", cwd.value);
      return;
    }
    addLine("output", `[shell] ${cmd}`);
    addLine("info", "(Tauri shell plugin will execute real commands)");
  }

  return { lines, currentInput, cwd, submit, addLine };
});
