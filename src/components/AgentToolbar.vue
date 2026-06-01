<template>
  <div class="agent-toolbar">
    <div class="agent-btns">
      <button
        v-for="a in store.agents"
        :key="a.id"
        class="agent-btn"
        :style="{ borderColor: a.color, color: a.color }"
        :disabled="!a.command.trim()"
        :title="a.command || 'No command set'"
        @click="a.command.trim() && $emit('launch', a.command)"
      >
        <span class="dot" :style="{ background: a.color }" />
        {{ a.name }}
      </button>
      <span v-if="store.agents.length === 0" class="no-agents">No agents configured</span>
    </div>

    <div class="at-divider" />
    <div class="at-spacer" />

    <div class="cmd-input">
      <PhCaretRight :size="11" class="cmd-icon" />
      <span class="cmd-placeholder">run agent...</span>
      <span class="cmd-kbd">⌘K</span>
    </div>

    <div class="at-gap" />
    <PhActivity :size="14" class="toolbar-icon" />
    <PhGear :size="14" class="toolbar-icon" title="Agent settings" @click="settingsOpen = true" />

    <SettingsModal v-if="settingsOpen" @close="settingsOpen = false" />
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { PhCaretRight, PhActivity, PhGear } from "@phosphor-icons/vue";
import { useAgentsStore } from "@/stores/agents";
import SettingsModal from "./SettingsModal.vue";

defineEmits<{ launch: [cmd: string] }>();

const store = useAgentsStore();
const settingsOpen = ref(false);
</script>

<style scoped>
.agent-toolbar {
  display: flex;
  align-items: center;
  height: 40px;
  padding: 0 12px;
  background: #0d0d0d;
  border-bottom: 1px solid #1e1e1e;
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

.dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
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
  background: #111111;
  border: 1px solid #252525;
  border-radius: 5px;
  cursor: text;
}

.cmd-icon { color: #444; flex-shrink: 0; }

.cmd-placeholder {
  font-family: var(--font-mono);
  font-size: 11px;
  color: #3a3a3a;
  white-space: nowrap;
}

.cmd-kbd {
  font-family: var(--font-ui);
  font-size: 10px;
  color: #2c2c2c;
  white-space: nowrap;
}

.at-gap { width: 8px; }

.toolbar-icon {
  color: #333;
  flex-shrink: 0;
  cursor: pointer;
}
.toolbar-icon:hover { color: #666; }
</style>
