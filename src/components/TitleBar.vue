<template>
  <!-- data-tauri-drag-region makes the whole bar draggable with native decorations: true -->
  <div class="titlebar" :class="{ dev: isDev }" data-tauri-drag-region>
    <!-- Spacer for native macOS traffic lights (~72px) -->
    <div class="traffic-light-spacer" data-tauri-drag-region />

    <div class="titlebar-center" data-tauri-drag-region>
      <button v-if="workspaceName" class="back-btn" @click="$emit('back')" title="Switch workspace">
        <PhHouse :size="13" />
      </button>
      <span class="project-name" data-tauri-drag-region>{{ workspaceName || "Burrow" }}</span>
      <span v-if="branch" class="branch-name" data-tauri-drag-region>
        <PhGitBranch :size="11" />
        {{ branch }}
      </span>
    </div>

    <div class="titlebar-end">
      <div class="tb-menu-wrap">
        <button
          class="tb-btn"
          title="Open folder in…"
          :disabled="!folderPath"
          @click.stop="menuOpen = !menuOpen"
        >
          <PhFolderOpen :size="14" />
          <PhCaretDown :size="9" />
        </button>
        <div v-if="menuOpen" class="tb-menu" @click.stop>
          <button class="tb-menu-item" @click="openIn('finder')"><PhFolderNotchOpen :size="14" />Reveal in Finder</button>
          <button class="tb-menu-item" @click="openIn('vscode')"><PhCode :size="14" />Open in VS Code</button>
          <button class="tb-menu-item" @click="openIn('zed')"><PhLightning :size="14" />Open in Zed</button>
        </div>
      </div>
      <button
        class="tb-btn"
        :class="{ on: rightPanelVisible }"
        :title="rightPanelVisible ? 'Hide explorer & git' : 'Show explorer & git'"
        @click="$emit('toggle-rightpanel')"
      >
        <PhSidebarSimple :size="14" />
      </button>
      <button class="tb-btn" title="Settings (⌘,)" @click="$emit('open-settings')">
        <PhGear :size="14" />
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { PhHouse, PhGitBranch, PhSidebarSimple, PhFolderOpen, PhGear, PhCaretDown, PhFolderNotchOpen, PhCode, PhLightning } from "@phosphor-icons/vue";

const props = defineProps<{ workspaceName?: string; branch?: string; folderPath?: string; rightPanelVisible?: boolean }>();
defineEmits(["back", "toggle-rightpanel", "open-settings"]);

const menuOpen = ref(false);

async function openIn(target: "finder" | "vscode" | "zed") {
  menuOpen.value = false;
  if (!props.folderPath) return;
  try {
    await invoke("open_path_in", { path: props.folderPath, target });
  } catch (e) {
    console.error("open_path_in failed", e);
  }
}

function onDocClick() { menuOpen.value = false; }
onMounted(() => window.addEventListener("click", onDocClick));
onBeforeUnmount(() => window.removeEventListener("click", onDocClick));

const isDev = import.meta.env.DEV;
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

.titlebar.dev {
  background: #5c1a1a;
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

.branch-name {
  display: flex;
  align-items: center;
  gap: 3px;
  font-family: var(--font-mono);
  font-size: 10px;
  color: var(--text-muted);
}

.titlebar-end {
  display: flex;
  align-items: center;
  gap: 2px;
  padding-right: 8px;
  flex-shrink: 0;
  -webkit-app-region: no-drag;
}

.tb-btn {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  display: flex;
  align-items: center;
  padding: 4px 5px;
  border-radius: 4px;
  -webkit-app-region: no-drag;
}
.tb-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
.tb-btn.on { color: var(--accent); }
.tb-btn:disabled { opacity: 0.35; cursor: default; }
.tb-btn:disabled:hover { background: none; color: var(--text-secondary); }

.tb-menu-wrap { position: relative; display: flex; }

.tb-menu {
  position: absolute;
  top: calc(100% + 4px);
  right: 0;
  min-width: 168px;
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 4px;
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.4);
  z-index: 1000;
}

.tb-menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  font-family: var(--font-ui);
  font-size: 12px;
  text-align: left;
  padding: 6px 8px;
  border-radius: 4px;
  white-space: nowrap;
}
.tb-menu-item:hover { background: var(--bg-hover); color: var(--text-primary); }
</style>
