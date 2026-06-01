<template>
  <div class="dialog-overlay" @click.self="$emit('close')">
    <div class="settings-panel">
      <div class="settings-head">
        <div class="settings-title">
          <PhGear :size="16" />
          <span>Agent Settings</span>
        </div>
        <button class="icon-btn" title="Close" @click="$emit('close')">
          <PhX :size="14" />
        </button>
      </div>

      <p class="settings-sub">
        Configure CLI agents. Each launches its command in a new terminal tab.
      </p>

      <div class="agent-rows">
        <div v-for="a in store.agents" :key="a.id" class="agent-row">
          <input
            type="color"
            class="color-swatch"
            :value="a.color"
            @input="store.update(a.id, { color: ($event.target as HTMLInputElement).value })"
          />
          <input
            class="row-input name"
            placeholder="Display name"
            :value="a.name"
            @input="store.update(a.id, { name: ($event.target as HTMLInputElement).value })"
          />
          <div class="cmd-wrap">
            <PhCaretRight :size="10" class="cmd-prefix" />
            <input
              class="row-input cmd"
              placeholder="command --flags"
              :value="a.command"
              @input="store.update(a.id, { command: ($event.target as HTMLInputElement).value })"
            />
          </div>
          <button class="row-del" title="Remove agent" @click="store.remove(a.id)">
            <PhTrash :size="13" />
          </button>
        </div>

        <div v-if="store.agents.length === 0" class="agents-empty">
          No agents. Add one below.
        </div>
      </div>

      <div class="settings-foot">
        <button class="btn-secondary" @click="store.reset()">
          <PhArrowCounterClockwise :size="12" /> Reset
        </button>
        <div class="foot-spacer" />
        <button class="btn-primary" @click="addAgent">
          <PhPlus :size="12" /> Add Agent
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import {
  PhGear, PhX, PhCaretRight, PhTrash, PhPlus, PhArrowCounterClockwise,
} from "@phosphor-icons/vue";
import { useAgentsStore } from "@/stores/agents";

defineEmits<{ close: [] }>();

const store = useAgentsStore();

const PALETTE = ["#a78bfa", "#34d399", "#60a5fa", "#f472b6", "#fbbf24", "#22d3ee"];

function addAgent() {
  const color = PALETTE[store.agents.length % PALETTE.length];
  store.add("New Agent", "", color);
}
</script>

<style scoped>
.dialog-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.6);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 200;
}

.settings-panel {
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 20px;
  width: 560px;
  max-height: 80vh;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.settings-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.settings-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
}

.settings-sub {
  font-size: 11px;
  color: var(--text-secondary);
  line-height: 1.5;
}

.icon-btn {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  display: flex;
  padding: 4px;
  border-radius: 4px;
}
.icon-btn:hover { color: var(--text-primary); background: var(--bg-hover); }

.agent-rows {
  display: flex;
  flex-direction: column;
  gap: 8px;
  overflow-y: auto;
  padding: 2px;
}

.agent-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.color-swatch {
  width: 26px;
  height: 26px;
  padding: 0;
  border: 1px solid var(--border);
  border-radius: 5px;
  background: none;
  cursor: pointer;
  flex-shrink: 0;
}

.row-input {
  background: var(--bg-base);
  border: 1px solid var(--border);
  border-radius: 5px;
  color: var(--text-primary);
  font-size: 12px;
  outline: none;
  padding: 6px 9px;
}
.row-input:focus { border-color: var(--accent); }
.row-input.name { width: 150px; flex-shrink: 0; }

.cmd-wrap {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 6px;
  background: var(--bg-base);
  border: 1px solid var(--border);
  border-radius: 5px;
  padding-left: 9px;
}
.cmd-wrap:focus-within { border-color: var(--accent); }
.cmd-prefix { color: var(--text-muted); flex-shrink: 0; }
.row-input.cmd {
  flex: 1;
  border: none;
  background: none;
  padding-left: 0;
  font-family: var(--font-mono);
  font-size: 11px;
}

.row-del {
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  display: flex;
  padding: 5px;
  border-radius: 4px;
  flex-shrink: 0;
}
.row-del:hover { color: var(--red); background: rgba(239, 68, 68, 0.12); }

.agents-empty {
  font-size: 11px;
  color: var(--text-muted);
  text-align: center;
  padding: 20px;
}

.settings-foot {
  display: flex;
  align-items: center;
  gap: 8px;
  padding-top: 4px;
  border-top: 1px solid var(--border);
  margin-top: 4px;
}
.foot-spacer { flex: 1; }

.btn-primary {
  display: flex;
  align-items: center;
  gap: 5px;
  background: var(--accent);
  border: none;
  border-radius: 5px;
  color: #fff;
  cursor: pointer;
  font-size: 12px;
  font-weight: 600;
  padding: 6px 14px;
}
.btn-primary:hover { background: var(--accent-dim); }

.btn-secondary {
  display: flex;
  align-items: center;
  gap: 5px;
  background: var(--bg-hover);
  border: 1px solid var(--border);
  border-radius: 5px;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 12px;
  padding: 6px 14px;
}
.btn-secondary:hover { color: var(--text-primary); border-color: #444; }
</style>
