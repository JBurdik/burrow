<template>
  <nav class="activity-bar">
    <button
      class="ab-btn"
      title="New terminal (⌘T)"
      @click="newTerminal()"
    >
      <PhTerminal :size="18" />
    </button>
    <button
      class="ab-btn"
      title="New Claude chat"
      @click="newChat()"
    >
      <ClaudeIcon :size="18" />
    </button>
  </nav>
</template>

<script setup lang="ts">
import { PhTerminal } from "@phosphor-icons/vue";
import ClaudeIcon from "@/components/icons/ClaudeIcon.vue";
import { useWorkspaceStore } from "@/stores/workspace";
import { useTerminalTabsStore } from "@/stores/terminalTabs";

const ws = useWorkspaceStore();
const termTabs = useTerminalTabsStore();

function newTerminal() {
  if (ws.active) termTabs.add(ws.active.id);
}

function newChat() {
  if (ws.active) termTabs.openChat(ws.active.id);
}
</script>

<style scoped>
.activity-bar {
  width: 44px;
  flex: 0 0 44px;
  flex-shrink: 0;
  background: var(--bg-panel);
  border-right: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 8px 0;
  gap: 2px;
}

.ab-btn {
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  border-radius: 8px;
  transition: color .12s, background .12s;
  position: relative;
}
.ab-btn:hover { color: var(--text-primary); background: var(--bg-hover); }
</style>
