<template>
  <div class="agent-toolbar">
    <div class="agent-btns">
      <button
        v-for="a in store.agents"
        :key="a.id"
        class="agent-btn"
        :style="{ borderColor: a.color, color: a.color }"
        :disabled="!a.command.trim()"
        :title="store.commandLine(a) || 'No command set'"
        @click="a.command.trim() && $emit('launch', store.commandLine(a))"
      >
        <component :is="iconFor(a.icon)" :size="12" :style="{ color: a.color }" />
        {{ a.name }}
        <span v-if="a.shortcut" class="agent-kbd">{{ a.shortcut }}</span>
      </button>
      <span v-if="store.agents.length === 0" class="no-agents">No agents configured</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import ClaudeIcon from "@/components/icons/ClaudeIcon.vue";
import GitHubCopilotIcon from "@/components/icons/GitHubCopilotIcon.vue";
import OpenAIIcon from "@/components/icons/OpenAIIcon.vue";
import { useAgentsStore, type AgentIcon } from "@/stores/agents";
import { PhCode, PhGitBranch, PhRobot, PhSparkle, PhTerminal } from "@phosphor-icons/vue";

const iconMap: Record<AgentIcon, unknown> = {
  sparkle: PhSparkle,
  code: PhCode,
  "git-branch": PhGitBranch,
  robot: PhRobot,
  terminal: PhTerminal,
  claude: ClaudeIcon,
  openai: OpenAIIcon,
  "github-copilot": GitHubCopilotIcon,
};
function iconFor(icon: AgentIcon) {
  return iconMap[icon] ?? PhRobot;
}

defineEmits<{ launch: [cmd: string] }>();

const store = useAgentsStore();
</script>

<style scoped>
.agent-toolbar {
  display: flex;
  align-items: center;
  height: 40px;
  padding: 0 12px;
  background: var(--bg-base);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
  gap: 8px;
}

.agent-btns {
  display: flex;
  align-items: center;
  gap: 6px;
}

.agent-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  border-radius: 4px;
  border: 1px solid;
  cursor: pointer;
  font-size: 12px;
  font-weight: 500;
  font-family: var(--font-ui);
  padding: 5px 10px;
  transition: opacity 0.15s;
  white-space: nowrap;
}
.agent-btn { background: rgba(255, 255, 255, 0.03); }
.agent-btn:hover:not(:disabled) { opacity: 0.8; }
.agent-btn:active:not(:disabled) { opacity: 0.6; }
.agent-btn:disabled { opacity: 0.4; cursor: default; }

.no-agents {
  font-size: 11px;
  color: var(--text-muted);
}

.at-divider {
  width: 1px;
  height: 20px;
  background: #2a2a2a;
  flex-shrink: 0;
}

.at-spacer { flex: 1; }

.cmd-input {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 26px;
  padding: 0 12px;
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: 5px;
  cursor: text;
}

.cmd-icon { color: var(--text-muted); flex-shrink: 0; }

.cmd-placeholder {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-muted);
  white-space: nowrap;
}

.cmd-kbd {
  font-family: var(--font-ui);
  font-size: 10px;
  color: var(--text-secondary);
  white-space: nowrap;
}

.agent-kbd {
  font-family: var(--font-ui);
  font-size: 9px;
  color: var(--text-secondary);
  background: rgba(255, 255, 255, 0.06);
  border-radius: 3px;
  padding: 1px 4px;
  margin-left: 2px;
}

.at-gap { width: 8px; }

.toolbar-icon {
  color: var(--text-secondary);
  flex-shrink: 0;
  cursor: pointer;
}
.toolbar-icon:hover { color: var(--text-primary); }
.toolbar-icon.on { color: var(--accent); }
</style>
