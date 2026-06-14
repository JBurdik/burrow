<template>
  <div class="agent-toolbar">
    <div class="agent-btns">
      <button
        v-for="a in store.agents"
        :key="a.id"
        class="agent-btn"
        :style="{ '--agent-color': a.color }"
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
    <div class="toolbar-end">
      <button class="claude-ui-btn" title="Open Claude chat" @click="$emit('open-chat')">
        <ClaudeIcon :size="13" />
        <span>Claude UI</span>
      </button>
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

defineEmits<{ launch: [cmd: string]; "open-chat": [] }>();

const store = useAgentsStore();
</script>

<style scoped>
.agent-toolbar {
  display: flex;
  align-items: center;
  height: 36px;
  padding: 0 10px;
  background: var(--bg-panel);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}

.agent-btns {
  display: flex;
  align-items: center;
  gap: 5px;
}

.agent-btn {
  display: flex;
  align-items: center;
  gap: 5px;
  border-radius: 5px;
  border: 1px solid color-mix(in srgb, var(--agent-color, var(--accent)) 22%, var(--border));
  background: color-mix(in srgb, var(--agent-color, var(--accent)) 7%, transparent);
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 11.5px;
  font-weight: 500;
  font-family: var(--font-ui);
  padding: 4px 9px;
  transition: background .12s, color .12s;
  white-space: nowrap;
}
.agent-btn:hover:not(:disabled) {
  background: color-mix(in srgb, var(--agent-color, var(--accent)) 14%, transparent);
  color: var(--text-primary);
}
.agent-btn:active:not(:disabled) { opacity: 0.75; }
.agent-btn:disabled { opacity: 0.35; cursor: default; }

.no-agents {
  font-size: 11px;
  color: var(--text-muted);
}

.agent-kbd {
  font-family: var(--font-ui);
  font-size: 9px;
  color: var(--text-muted);
  background: color-mix(in srgb, var(--agent-color, var(--accent)) 10%, rgba(255,255,255,0.05));
  border-radius: 3px;
  padding: 1px 4px;
  margin-left: 1px;
  flex-shrink: 0;
}

.toolbar-end {
  margin-left: auto;
  display: flex;
  align-items: center;
}

.claude-ui-btn {
  display: flex;
  align-items: center;
  gap: 5px;
  border-radius: 5px;
  border: 1px solid color-mix(in srgb, #d97706 18%, var(--border));
  background: color-mix(in srgb, #d97706 6%, transparent);
  color: var(--text-muted);
  cursor: pointer;
  font-size: 11.5px;
  font-weight: 500;
  font-family: var(--font-ui);
  padding: 4px 9px;
  transition: background .12s, color .12s;
}
.claude-ui-btn :deep(svg) { color: #d97706; }
.claude-ui-btn:hover {
  background: color-mix(in srgb, #d97706 13%, transparent);
  color: var(--text-primary);
}
</style>
