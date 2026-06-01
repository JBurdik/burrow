<template>
  <div class="ide-root">
    <TitleBar :workspace-name="ws.active?.name" @back="ws.close()" />
    <div class="ide-body">
      <Sidebar />
      <div class="ide-main">
        <div v-if="!ws.active" class="no-workspace">
          <PhFolderOpen :size="32" weight="thin" />
          <span>Select a workspace</span>
        </div>
        <Terminal v-else :key="ws.active.id" :cwd="ws.active.path" />
      </div>
      <RightPanel :cwd="ws.active?.path ?? ''" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted } from "vue";
import { PhFolderOpen } from "@phosphor-icons/vue";
import TitleBar from "@/components/TitleBar.vue";
import Sidebar from "@/components/Sidebar.vue";
import Terminal from "@/components/Terminal.vue";
import RightPanel from "@/components/RightPanel.vue";
import { useWorkspaceStore } from "@/stores/workspace";

const ws = useWorkspaceStore();
onMounted(() => ws.load());
</script>

<style>
* {
  box-sizing: border-box;
  margin: 0;
  padding: 0;
}

:root {
  --bg-base: #0d0d0d;
  --bg-panel: #111111;
  --bg-hover: #1a1a1a;
  --bg-selected: #1e3a5f;
  --border: #222222;
  --text-primary: #e2e8f0;
  --text-secondary: #64748b;
  --text-muted: #334155;
  --accent: #3b82f6;
  --accent-dim: #1d4ed8;
  --green: #22c55e;
  --yellow: #eab308;
  --red: #ef4444;
  --font-mono: "JetBrains Mono", "Fira Code", "Cascadia Code", monospace;
  --font-ui: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  --sidebar-width: 220px;
  --right-panel-width: 300px;
  --titlebar-height: 36px;
}

body {
  background: var(--bg-base);
  color: var(--text-primary);
  font-family: var(--font-ui);
  overflow: hidden;
  user-select: none;
}

.ide-root {
  display: flex;
  flex-direction: column;
  width: 100vw;
  height: 100vh;
}

.ide-body {
  display: flex;
  flex: 1;
  overflow: hidden;
}

.ide-main {
  flex: 1;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.no-workspace {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  color: var(--text-muted);
  font-size: 13px;
}
</style>
