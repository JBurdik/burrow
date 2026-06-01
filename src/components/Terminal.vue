<template>
  <div class="terminal-pane" @click="focusActive">
    <AgentToolbar @launch="spawnAgent" />

    <div class="terminal-tabs">
      <button
        v-for="tab in tabs"
        :key="tab.id"
        class="tab"
        :class="{ active: activeId === tab.id }"
        @click.stop="activateTab(tab.id)"
      >
        <PhRobot v-if="tab.isAgent" :size="12" class="tab-agent-icon" />
        <PhTerminal v-else :size="12" class="tab-term-icon" />
        <span class="tab-label">{{ tab.title }}</span>
        <PhX
          v-if="tabs.length > 1"
          :size="9"
          weight="bold"
          class="tab-close"
          @click.stop="closeTab(tab.id)"
        />
      </button>
      <button class="tab tab-add" @click="addTab()" title="New terminal">
        <PhPlus :size="12" />
      </button>
    </div>

    <div class="terminal-body">
      <XTerm
        v-for="tab in tabs"
        :key="tab.id"
        :pty-id="tab.id"
        :cwd="cwd"
        :initial-cmd="tab.initialCmd"
        :class="{ hidden: activeId !== tab.id }"
        :ref="(el) => setRef(tab.id, el)"
        @title="(t) => onTitle(tab.id, t)"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, nextTick } from "vue";
import { PhRobot, PhTerminal, PhX, PhPlus } from "@phosphor-icons/vue";
import XTerm from "./XTerm.vue";
import AgentToolbar from "./AgentToolbar.vue";

defineProps<{ cwd: string }>();

interface Tab {
  id: number;
  title: string;
  defaultTitle: string;
  isAgent: boolean;
  initialCmd?: string;
}

let nextId = 1;
const tabs = ref<Tab[]>([{ id: nextId, title: "Terminal 1", defaultTitle: "Terminal 1", isAgent: false }]);
nextId++;
const activeId = ref(tabs.value[0].id);
const xtermRefs = new Map<number, InstanceType<typeof XTerm>>();

function setRef(id: number, el: unknown) {
  if (el) xtermRefs.set(id, el as InstanceType<typeof XTerm>);
  else xtermRefs.delete(id);
}

function onTitle(id: number, title: string) {
  const tab = tabs.value.find((t) => t.id === id);
  if (!tab) return;
  if (!title) {
    tab.title = tab.defaultTitle;
    tab.isAgent = false;
  } else {
    tab.title = title.replace(/^🤖\s*/, "Claude");
    tab.isAgent = title.includes("Claude") || title.toLowerCase().includes("claude");
  }
}

function activateTab(id: number) {
  activeId.value = id;
  nextTick(() => xtermRefs.get(id)?.focus());
}

function addTab(initialCmd?: string) {
  const id = nextId++;
  const defaultTitle = `Terminal ${tabs.value.length + 1}`;
  tabs.value.push({ id, title: defaultTitle, defaultTitle, isAgent: false, initialCmd });
  activeId.value = id;
  nextTick(() => xtermRefs.get(id)?.focus());
}

function spawnAgent(cmd: string) {
  addTab(cmd);
}

function closeTab(id: number) {
  const idx = tabs.value.findIndex((t) => t.id === id);
  tabs.value.splice(idx, 1);
  if (activeId.value === id) {
    activeId.value = tabs.value[Math.max(0, idx - 1)]?.id ?? 0;
  }
}

function focusActive() {
  xtermRefs.get(activeId.value)?.focus();
}
</script>

<style scoped>
.terminal-pane {
  flex: 1;
  display: flex;
  flex-direction: column;
  background: #0a0a0a;
  overflow: hidden;
  min-width: 0;
}

.terminal-tabs {
  display: flex;
  align-items: center;
  background: var(--bg-panel);
  border-bottom: 1px solid var(--border);
  padding: 0 4px;
  flex-shrink: 0;
  overflow-x: auto;
}

.tab {
  display: flex;
  align-items: center;
  gap: 6px;
  background: none;
  border: none;
  border-bottom: 2px solid transparent;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 11px;
  font-family: var(--font-ui);
  padding: 6px 10px;
  white-space: nowrap;
  transition: color 0.1s;
  max-width: 200px;
  flex-shrink: 0;
}
.tab:hover { color: var(--text-primary); }
.tab.active { color: var(--text-primary); border-bottom-color: var(--accent); }
.tab-add { color: var(--text-muted); font-size: 14px; max-width: none; }
.tab-add:hover { color: var(--text-secondary); }

.tab-label {
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 140px;
}

.tab-agent-icon { color: var(--accent); flex-shrink: 0; }
.tab-term-icon  { color: var(--text-muted); flex-shrink: 0; }

.tab-close {
  opacity: 0.4;
  border-radius: 3px;
  padding: 2px;
  flex-shrink: 0;
  cursor: pointer;
}
.tab-close:hover { opacity: 1; background: rgba(239,68,68,0.2); color: var(--red); }

.terminal-body {
  flex: 1;
  display: flex;
  overflow: hidden;
  position: relative;
}

.hidden { display: none; }
</style>
