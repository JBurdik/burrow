<template>
  <!-- data-tauri-drag-region makes the whole bar draggable with native decorations: true -->
  <div class="titlebar" data-tauri-drag-region>
    <!-- Spacer for native macOS traffic lights (~72px) -->
    <div class="traffic-light-spacer" />

    <div class="titlebar-center">
      <button v-if="workspaceName" class="back-btn" @click="$emit('back')" title="Switch workspace">
        <PhHouse :size="13" />
      </button>
      <span class="project-name">{{ workspaceName || "Agentic IDE" }}</span>
    </div>

    <div class="titlebar-end" />
  </div>
</template>

<script setup lang="ts">
import { PhHouse } from "@phosphor-icons/vue";

defineProps<{ workspaceName?: string }>();
defineEmits(["back"]);
</script>

<style scoped>
.titlebar {
  height: var(--titlebar-height);
  background: var(--bg-panel);
  border-bottom: 1px solid var(--border);
  display: flex;
  align-items: center;
  flex-shrink: 0;
  /* macOS Overlay titlebar sits on top — match its height so native buttons line up */
  padding-top: env(titlebar-area-y, 0px);
}

/* Reserve room for the three native traffic light buttons */
.traffic-light-spacer {
  width: 72px;
  flex-shrink: 0;
}

.titlebar-center {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
}

.back-btn {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  display: flex;
  align-items: center;
  padding: 3px 5px;
  border-radius: 4px;
  /* must not be a drag region */
  -webkit-app-region: no-drag;
}
.back-btn:hover { background: var(--bg-hover); color: var(--text-primary); }

.project-name {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-secondary);
}

.titlebar-end {
  width: 72px;
  flex-shrink: 0;
}
</style>
