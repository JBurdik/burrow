<template>
  <Teleport to="body">
    <Transition name="spotlight">
      <div v-if="isOpen" class="s-overlay" @mousedown.self="close">
        <div class="s-modal">
          <div class="s-bar">
            <PhTerminal :size="18" color="#7C3AED" />
            <input
              ref="inputRef"
              v-model="query"
              placeholder="run claude --"
              class="s-input"
              spellcheck="false"
              autocomplete="off"
              @keydown.esc.prevent="close"
              @keydown.enter.prevent="activate"
              @keydown.up.prevent="move(-1)"
              @keydown.down.prevent="move(1)"
            />
            <div class="s-esc">esc</div>
          </div>

          <div class="s-results">
            <template v-for="(section, si) in sections" :key="section.key">
              <template v-if="section.items.length">
                <div class="s-section-label">{{ section.label }}</div>
                <div
                  v-for="item in section.items"
                  :key="item.id"
                  class="s-row"
                  :style="{ background: selectedId === item.id ? item.iconBg : 'transparent' }"
                  @mouseenter="selectedId = item.id"
                  @click="runItem(item)"
                >
                  <div class="s-icon-wrap" :style="{ background: item.iconBg, borderColor: item.iconBorder }">
                    <component :is="item.icon" :size="14" :color="item.iconColor" />
                  </div>
                  <div class="s-info">
                    <span class="s-title" :style="{ color: item.dim ? '#999' : '#e8e8e8' }">{{ item.title }}</span>
                    <span v-if="item.desc" class="s-desc">{{ item.desc }}</span>
                  </div>
                  <div
                    v-if="item.shortcut"
                    class="s-key"
                    :style="selectedId === item.id && !item.dim
                      ? { background: item.iconBg, borderColor: item.iconBorder, color: item.iconColor }
                      : {}"
                  >{{ item.shortcut }}</div>
                </div>
                <div v-if="si < sections.length - 1" class="s-divider" />
              </template>
            </template>
          </div>

          <div class="s-footer">
            <div class="s-hint"><span class="s-key-sm">↑↓</span><span>navigate</span></div>
            <div class="s-hint"><span class="s-key-sm">↵</span><span>run</span></div>
            <div class="s-hint"><span class="s-key-sm">⌘↵</span><span>new tab</span></div>
            <div class="s-hint"><span class="s-key-sm">⇥</span><span>complete</span></div>
            <div style="flex:1" />
            <div class="s-branding">
              <PhSparkle :size="11" color="#7C3AED" />
              <span>Claude Code</span>
            </div>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick } from "vue";
import {
  PhTerminal, PhSparkle, PhCode, PhGitBranch, PhRobot,
  PhFolderOpen, PhGear, PhPlus, PhColumns, PhPalette, PhKeyboard, PhGlobe, PhPlayCircle,
} from "@phosphor-icons/vue";
import { useAgentsStore } from "@/stores/agents";
import { useScriptsStore } from "@/stores/scripts";
import { useWorkspaceStore } from "@/stores/workspace";
import type { Component } from "vue";

const emit = defineEmits<{
  launch: [cmd: string];
  newTerminal: [];
  newWorkspace: [];
  openSettings: [];
  openBrowser: [];
  repaint: [];
}>();

const isOpen = ref(false);
const query = ref("");
const selectedId = ref("");
const inputRef = ref<HTMLInputElement | null>(null);

const agentsStore = useAgentsStore();
const scriptsStore = useScriptsStore();
const wsStore = useWorkspaceStore();

const ICON_MAP: Record<string, Component> = {
  sparkle: PhSparkle,
  code: PhCode,
  "git-branch": PhGitBranch,
  robot: PhRobot,
  terminal: PhTerminal,
};

function hexBg(hex: string): string {
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  return `rgb(${Math.round(r * 0.11)},${Math.round(g * 0.11)},${Math.round(b * 0.11)})`;
}

interface SpotlightItem {
  id: string;
  title: string;
  desc?: string;
  icon: Component;
  iconColor: string;
  iconBg: string;
  iconBorder: string;
  shortcut?: string;
  dim: boolean;
  action: () => void;
}

const sections = computed(() => {
  const q = query.value.toLowerCase().trim();

  const agentItems: SpotlightItem[] = agentsStore.agents
    .filter((a) => !q || a.name.toLowerCase().includes(q) || agentsStore.commandLine(a).includes(q))
    .map((a, i) => ({
      id: `agent-${a.id}`,
      title: `Run ${a.name}`,
      desc: agentsStore.commandLine(a),
      icon: ICON_MAP[a.icon] ?? PhRobot,
      iconColor: a.color,
      iconBg: hexBg(a.color),
      iconBorder: `${a.color}33`,
      shortcut: a.shortcut || undefined,
      dim: i !== 0,
      action: () => { emit("launch", agentsStore.commandLine(a)); close(); },
    }));

  const scriptItems: SpotlightItem[] = scriptsStore.scriptsFor(wsStore.active?.id)
    .filter((s) => scriptsStore.commandLine(s) && (!q || s.name.toLowerCase().includes(q) || scriptsStore.commandLine(s).toLowerCase().includes(q)))
    .map((s) => {
      const color = s.color || "#34d399";
      return {
        id: `script-${s.id}`,
        title: `Run ${s.name}`,
        desc: scriptsStore.commandLine(s),
        icon: PhPlayCircle as Component,
        iconColor: color,
        iconBg: hexBg(color),
        iconBorder: `${color}33`,
        shortcut: undefined,
        dim: true,
        action: () => { emit("launch", scriptsStore.commandLine(s)); close(); },
      };
    });

  const recentWorkspaces = [...wsStore.workspaces]
    .sort((a, b) => (b.last_opened ?? 0) - (a.last_opened ?? 0))
    .slice(0, 3)
    .filter((w) => !q || w.name.toLowerCase().includes(q) || w.path.toLowerCase().includes(q));

  const recentItems: SpotlightItem[] = [
    ...recentWorkspaces.map((w) => ({
      id: `ws-${w.id}`,
      title: w.name,
      desc: w.path,
      icon: PhFolderOpen as Component,
      iconColor: "#a78bfa",
      iconBg: hexBg("#a78bfa"),
      iconBorder: "#a78bfa33",
      shortcut: undefined,
      dim: true,
      action: () => { wsStore.open(w); close(); },
    })),
    ...([
      { id: "cmd-settings", title: "Settings → Agents", icon: PhGear as Component, color: "#555555", shortcut: undefined, action: () => { emit("openSettings"); close(); } },
      { id: "cmd-newterm", title: "New Terminal", icon: PhTerminal as Component, color: "#34d399", shortcut: "⌃`", action: () => { emit("newTerminal"); close(); } },
      { id: "cmd-browser", title: "Open Browser Tab", icon: PhGlobe as Component, color: "#60a5fa", shortcut: undefined, action: () => { emit("openBrowser"); close(); } },
      { id: "cmd-repaint", title: "Repaint Terminal (un-scramble)", icon: PhTerminal as Component, color: "#fbbf24", shortcut: "⌘⇧R", action: () => { emit("repaint"); close(); } },
    ] as const)
      .filter(({ title }) => !q || title.toLowerCase().includes(q))
      .map((c) => ({
        id: c.id,
        title: c.title,
        icon: c.icon,
        iconColor: c.color,
        iconBg: hexBg(c.color),
        iconBorder: `${c.color}33`,
        shortcut: c.shortcut,
        dim: true,
        action: c.action,
      })),
  ];

  const cmdDefs: { id: string; title: string; icon: Component; shortcut?: string; action: () => void }[] = [
    { id: "cmd-new-ws", title: "New Workspace", icon: PhPlus as Component, action: () => { emit("newWorkspace"); close(); } },
    { id: "cmd-split", title: "Split Terminal", icon: PhColumns as Component, shortcut: "⌘\\", action: () => close() },
    { id: "cmd-theme", title: "Change Theme", icon: PhPalette as Component, action: () => close() },
    { id: "cmd-keys", title: "Keyboard Shortcuts", icon: PhKeyboard as Component, shortcut: "⌘K ⌘S", action: () => close() },
  ];

  const commandItems: SpotlightItem[] = cmdDefs
    .filter((c) => !q || c.title.toLowerCase().includes(q))
    .map((c) => ({
      id: c.id,
      title: c.title,
      icon: c.icon,
      iconColor: "#555555",
      iconBg: "#161616",
      iconBorder: "#55555533",
      shortcut: c.shortcut,
      dim: true,
      action: c.action,
    }));

  return [
    { key: "agents", label: "AGENTS", items: agentItems },
    { key: "scripts", label: "SCRIPTS", items: scriptItems },
    { key: "recent", label: "RECENT", items: recentItems },
    { key: "commands", label: "COMMANDS", items: commandItems },
  ].filter((s) => s.items.length > 0);
});

const flatItems = computed(() => sections.value.flatMap((s) => s.items));

function selectFirst() {
  selectedId.value = flatItems.value[0]?.id ?? "";
}

watch(query, () => nextTick(selectFirst));

function move(dir: 1 | -1) {
  const items = flatItems.value;
  const idx = items.findIndex((i) => i.id === selectedId.value);
  const next = Math.max(0, Math.min(items.length - 1, idx + dir));
  selectedId.value = items[next]?.id ?? "";
}

function activate() {
  flatItems.value.find((i) => i.id === selectedId.value)?.action();
}

function runItem(item: SpotlightItem) {
  item.action();
}

function show() {
  isOpen.value = true;
  query.value = "";
  nextTick(() => {
    inputRef.value?.focus();
    selectFirst();
  });
}

function close() {
  isOpen.value = false;
}

defineExpose({ show, close });
</script>

<style scoped>
.s-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.6);
  display: flex;
  justify-content: center;
  padding-top: 165px;
  z-index: 9000;
}

.s-modal {
  width: 680px;
  max-height: 600px;
  background: var(--bg-panel);
  border: 1px solid #2a2a2a;
  border-radius: 12px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  box-shadow: 0 24px 64px rgba(0, 0, 0, 0.6), 0 1px 0 rgba(255, 255, 255, 0.08);
  align-self: flex-start;
  backdrop-filter: var(--blur-overlay, none);
  -webkit-backdrop-filter: var(--blur-overlay, none);
}

.s-bar {
  display: flex;
  align-items: center;
  gap: 12px;
  height: 56px;
  padding: 0 16px;
  border-bottom: 1px solid #1e1e1e;
  flex-shrink: 0;
}

.s-input {
  flex: 1;
  background: none;
  border: none;
  outline: none;
  color: #e2e2e2;
  font-family: var(--font-ui);
  font-size: 15px;
  caret-color: #7C3AED;
}

.s-input::placeholder { color: #444; }

.s-esc {
  font-size: 11px;
  color: #555555;
  background: #1a1a1a;
  border: 1px solid #2a2a2a;
  border-radius: 4px;
  padding: 3px 8px;
  flex-shrink: 0;
}

.s-results {
  overflow-y: auto;
  padding: 6px 0;
  flex: 1;
}

.s-results::-webkit-scrollbar { width: 4px; }
.s-results::-webkit-scrollbar-track { background: transparent; }
.s-results::-webkit-scrollbar-thumb { background: #2a2a2a; border-radius: 2px; }

.s-section-label {
  height: 26px;
  display: flex;
  align-items: center;
  padding: 0 16px;
  font-size: 10px;
  font-weight: 600;
  color: #3a3a3a;
  font-family: var(--font-ui);
  letter-spacing: 0.05em;
}

.s-row {
  display: flex;
  align-items: center;
  gap: 12px;
  height: 46px;
  padding: 0 12px;
  border-radius: 6px;
  cursor: pointer;
  transition: background 0.08s;
  margin: 0 4px;
}

.s-icon-wrap {
  width: 30px;
  height: 30px;
  border-radius: 7px;
  border: 1px solid transparent;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.s-info {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.s-title {
  font-size: 13px;
  font-weight: 500;
  font-family: var(--font-ui);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.s-desc {
  font-size: 11px;
  color: #383838;
  font-family: var(--font-mono);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.s-key {
  font-size: 11px;
  color: #444444;
  background: #161616;
  border: 1px solid #222222;
  border-radius: 4px;
  padding: 3px 8px;
  flex-shrink: 0;
  font-family: var(--font-ui);
  transition: all 0.08s;
}

.s-divider {
  height: 1px;
  background: #1a1a1a;
  margin: 4px 0;
}

.s-footer {
  display: flex;
  align-items: center;
  gap: 16px;
  height: 36px;
  padding: 0 16px;
  background: #0d0d0d;
  border-top: 1px solid #1e1e1e;
  flex-shrink: 0;
}

.s-hint {
  display: flex;
  align-items: center;
  gap: 5px;
  font-size: 11px;
  color: #333333;
  font-family: var(--font-ui);
}

.s-key-sm {
  font-size: 11px;
  color: #555555;
  background: #161616;
  border: 1px solid #252525;
  border-radius: 4px;
  padding: 2px 6px;
}

.s-branding {
  display: flex;
  align-items: center;
  gap: 5px;
  font-size: 11px;
  color: #333333;
  font-family: var(--font-ui);
}

/* Transition */
.spotlight-enter-active,
.spotlight-leave-active {
  transition: opacity 0.12s ease;
}
.spotlight-enter-active .s-modal,
.spotlight-leave-active .s-modal {
  transition: opacity 0.12s ease, transform 0.12s ease;
}
.spotlight-enter-from,
.spotlight-leave-to {
  opacity: 0;
}
.spotlight-enter-from .s-modal,
.spotlight-leave-to .s-modal {
  opacity: 0;
  transform: translateY(-8px) scale(0.98);
}
</style>
