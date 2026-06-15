<template>
  <nav class="activity-bar">
    <button
      class="ab-btn"
      :class="{ active: ui.mode === 'dashboard' }"
      title="Dashboard"
      @click="ui.toggleDashboard()"
    >
      <PhSquaresFour :size="18" />
    </button>
    <div class="ab-sep" />
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
    <button
      class="ab-btn"
      :class="{ active: ui.mode === 'git' }"
      title="Git panel"
      @click="ui.toggleGitPanel()"
    >
      <PhGitBranch :size="18" />
    </button>
    <button
      class="ab-btn"
      :class="{ active: ui.mode === 'mission' }"
      title="Mission Control (task dashboard)"
      @click="toggleMission()"
    >
      <PhRocketLaunch :size="18" />
      <span v-if="ui.missionActiveCount > 0" class="mc-badge">{{ ui.missionActiveCount > 9 ? '9+' : ui.missionActiveCount }}</span>
    </button>
  </nav>
</template>

<script setup lang="ts">
import { PhTerminal, PhGitBranch, PhRocketLaunch, PhSquaresFour } from "@phosphor-icons/vue";
import ClaudeIcon from "@/components/icons/ClaudeIcon.vue";
import { useWorkspaceStore } from "@/stores/workspace";
import { useTerminalTabsStore } from "@/stores/terminalTabs";
import { useUIStore } from "@/stores/ui";

const ws = useWorkspaceStore();
const termTabs = useTerminalTabsStore();
const ui = useUIStore();

function newTerminal() {
  // From any non-terminal view (git/mission), the terminal icon returns to the
  // terminal first instead of silently adding a hidden tab.
  if (ui.mode !== 'terminal') { ui.setMode('terminal'); return; }
  if (ws.active) termTabs.add(ws.active.id);
}

function newChat() {
  if (ui.mode !== 'terminal') { ui.setMode('terminal'); return; }
  if (ws.active) termTabs.openChat(ws.active.id);
}

function toggleMission() {
  ui.setMode(ui.mode === "mission" ? "terminal" : "mission");
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
.ab-sep {
  width: 22px;
  height: 1px;
  background: var(--border);
  margin: 4px 0;
}
.ab-btn:hover { color: var(--text-primary); background: var(--bg-hover); }
.ab-btn.active {
  color: var(--accent);
  background: color-mix(in srgb, var(--accent) 12%, transparent);
}
.mc-badge {
  position: absolute;
  top: 3px;
  right: 3px;
  min-width: 14px;
  height: 14px;
  border-radius: 7px;
  background: var(--yellow);
  color: #000;
  font-size: 9px;
  font-weight: 700;
  line-height: 14px;
  text-align: center;
  padding: 0 3px;
  pointer-events: none;
  animation: badge-pulse 1.4s infinite;
}
@keyframes badge-pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.5; } }
</style>
