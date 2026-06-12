<template>
  <nav class="activity-bar">
    <button
      class="ab-btn"
      :class="{ active: ui.mode === 'terminal' }"
      title="Terminal"
      @click="ui.setMode('terminal')"
    >
      <PhTerminal :size="18" />
    </button>
    <button
      class="ab-btn"
      :class="{ active: ui.mode === 'claude' }"
      title="Claude"
      @click="ui.setMode('claude')"
    >
      <ClaudeIcon :size="18" />
    </button>
  </nav>
</template>

<script setup lang="ts">
import { PhTerminal } from "@phosphor-icons/vue";
import ClaudeIcon from "@/components/icons/ClaudeIcon.vue";
import { useUIStore } from "@/stores/ui";

const ui = useUIStore();
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
.ab-btn.active {
  color: var(--text-primary);
  background: color-mix(in srgb, var(--accent) 15%, transparent);
}
.ab-btn.active::before {
  content: "";
  position: absolute;
  left: -1px;
  top: 50%;
  transform: translateY(-50%);
  width: 3px;
  height: 20px;
  border-radius: 0 2px 2px 0;
  background: var(--accent);
}
</style>
