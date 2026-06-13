<template>
  <!-- data-tauri-drag-region makes the whole bar draggable with native decorations: true -->
  <div class="titlebar" :class="{ dev: isDev }" data-tauri-drag-region>
    <!-- Spacer for native macOS traffic lights (~72px) -->
    <div class="traffic-light-spacer" data-tauri-drag-region />

    <!-- Notification center -->
    <div class="tb-menu-wrap titlebar-notif">
      <button
        class="tb-btn notif-btn"
        :class="{ on: notifOpen, 'has-unread': notifStore.unreadCount > 0 }"
        title="Notifications"
        @click.stop="toggleNotif"
      >
        <PhBell :size="14" />
        <span v-if="notifStore.unreadCount > 0" class="notif-badge">
          {{ notifStore.unreadCount > 9 ? "9+" : notifStore.unreadCount }}
        </span>
      </button>
      <div v-if="notifOpen" class="tb-menu notif-menu" @click.stop>
        <div class="notif-header">
          <span class="notif-title">Notifications</span>
          <button
            v-if="notifStore.history.length"
            class="notif-clear-btn"
            @click="notifStore.clearHistory()"
          >Clear all</button>
        </div>
        <div v-if="!notifStore.history.length" class="notif-empty">No notifications</div>
        <div v-else class="notif-list">
          <div
            v-for="item in notifStore.history"
            :key="item.id"
            class="notif-item"
            :class="[`notif-${item.type}`, { 'notif-clickable': item.workspaceId }]"
            @click="navigateToNotif(item.workspaceId)"
          >
            <PhCheckCircle v-if="item.type === 'done'" :size="13" class="notif-icon" />
            <PhWarning v-else-if="item.type === 'error'" :size="13" class="notif-icon" />
            <PhInfo v-else :size="13" class="notif-icon" />
            <div class="notif-body">
              <div class="notif-item-title">{{ item.title }}</div>
              <div v-if="item.body" class="notif-item-body">{{ item.body }}</div>
            </div>
            <span class="notif-time">{{ relTime(item.ts) }}</span>
          </div>
        </div>
      </div>
    </div>

    <!-- Claude 5h usage widget — visible as long as any session exists -->
    <div
      v-if="chats.allSessions.length > 0"
      class="claude-usage"
      :class="{ 'usage-empty': chats.turnsInWindow.length === 0 }"
      :title="chats.turnsInWindow.length > 0
        ? `${chats.turnsInWindow.length} turns · ${fmtTokens(chats.windowTokens)} tokens in last 5h`
        : 'No Claude turns recorded in the last 5h'"
      data-tauri-drag-region
    >
      <ClaudeIcon :size="11" class="usage-icon" />
      <div class="usage-bar-wrap">
        <div class="usage-bar-fill" :style="{ width: usagePct + '%' }" :class="{ 'usage-warn': usagePct > 75, 'usage-crit': usagePct > 90 }" />
      </div>
      <span class="usage-label">{{ chats.turnsInWindow.length }}<span class="usage-window">/ 5h</span></span>
    </div>

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
      <div class="tb-menu-wrap">
        <button
          class="tb-btn"
          title="System & daemon stats"
          @click.stop="toggleStats"
        >
          <PhGauge :size="14" />
          <PhCaretDown :size="9" />
        </button>
        <div v-if="statsOpen" class="tb-menu stats-menu" @click.stop>
          <div class="stat-row">
            <span class="stat-label"><PhCpu :size="13" />CPU</span>
            <span class="stat-val">{{ stats ? stats.cpu_percent.toFixed(0) + "%" : "…" }}</span>
          </div>
          <div class="stat-bar"><div class="stat-bar-fill" :style="{ width: (stats?.cpu_percent ?? 0) + '%' }" /></div>

          <div class="stat-row">
            <span class="stat-label"><PhMemory :size="13" />RAM</span>
            <span class="stat-val">{{ memText }}</span>
          </div>
          <div class="stat-bar"><div class="stat-bar-fill" :style="{ width: memPct + '%' }" /></div>

          <div class="stat-sep" />

          <div class="stat-row">
            <span class="stat-label"><PhStack :size="13" />Daemon</span>
            <span class="stat-val" :class="{ off: daemon && !daemon.connected }">
              {{ daemon ? (daemon.connected ? daemon.alive + "/" + daemon.total + " live" : "offline") : "…" }}
            </span>
          </div>
          <div v-if="daemon?.pid" class="stat-pid">pid {{ daemon.pid }}</div>

          <div class="stat-sep" />

          <button class="tb-menu-item" :disabled="busy" @click="cleanDaemon">
            <PhBroom :size="14" />Clean dead sessions
          </button>
          <button class="tb-menu-item danger" :disabled="busy" @click="restartDaemon">
            <PhArrowsClockwise :size="14" />Restart daemon
          </button>
          <div v-if="actionMsg" class="stat-msg">{{ actionMsg }}</div>
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
import { ref, computed, onMounted, onBeforeUnmount } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { PhHouse, PhGitBranch, PhSidebarSimple, PhFolderOpen, PhGear, PhCaretDown, PhFolderNotchOpen, PhCode, PhLightning, PhGauge, PhCpu, PhMemory, PhStack, PhBroom, PhArrowsClockwise, PhBell, PhCheckCircle, PhWarning, PhInfo } from "@phosphor-icons/vue";
import { useNotificationsStore } from "@/stores/notifications";
import { useClaudeChatsStore } from "@/stores/claudeChats";
import { useWorkspaceStore } from "@/stores/workspace";
import ClaudeIcon from "@/components/icons/ClaudeIcon.vue";

const props = defineProps<{ workspaceName?: string; branch?: string; folderPath?: string; rightPanelVisible?: boolean }>();
defineEmits(["back", "toggle-rightpanel", "open-settings"]);

const menuOpen = ref(false);

// ── Notification center ─────────────────────────────────────────────────────
const notifStore = useNotificationsStore();
const notifOpen = ref(false);
const wsStore = useWorkspaceStore();

function navigateToNotif(workspaceId?: number) {
  if (!workspaceId) return;
  const ws = wsStore.workspaces.find((w) => w.id === workspaceId);
  if (ws) wsStore.open(ws);
  notifOpen.value = false;
}

// ── Claude 5h usage widget ──────────────────────────────────────────────────
const chats = useClaudeChatsStore();

// Soft cap: Claude Pro ~45 turns / 5h. Bar fills toward this; goes red past 90%.
const TURNS_SOFT_CAP = 45;

const usagePct = computed(() => Math.min(100, (chats.turnsInWindow.length / TURNS_SOFT_CAP) * 100));

function fmtTokens(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(0) + "k";
  return String(n);
}

function toggleNotif() {
  notifOpen.value = !notifOpen.value;
  if (notifOpen.value) notifStore.markAllRead();
}

function relTime(ts: number): string {
  const diff = Date.now() - ts;
  if (diff < 60_000) return "now";
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h`;
  return `${Math.floor(diff / 86_400_000)}d`;
}

// ── Stats dropdown ──────────────────────────────────────────────────────────
type SystemStats = { cpu_percent: number; mem_used: number; mem_total: number };
type DaemonStats = { connected: boolean; pid: number | null; total: number; alive: number };

const statsOpen = ref(false);
const stats = ref<SystemStats | null>(null);
const daemon = ref<DaemonStats | null>(null);
const busy = ref(false);
const actionMsg = ref("");
let statsTimer: number | undefined;

const memPct = computed(() =>
  stats.value && stats.value.mem_total ? (stats.value.mem_used / stats.value.mem_total) * 100 : 0
);
const memText = computed(() => {
  if (!stats.value) return "…";
  const gb = (b: number) => (b / 1024 ** 3).toFixed(1);
  return `${gb(stats.value.mem_used)} / ${gb(stats.value.mem_total)} GB`;
});

async function refreshStats() {
  try {
    [stats.value, daemon.value] = await Promise.all([
      invoke<SystemStats>("system_stats"),
      invoke<DaemonStats>("daemon_stats"),
    ]);
  } catch (e) {
    console.error("stats refresh failed", e);
  }
}

function toggleStats() {
  statsOpen.value = !statsOpen.value;
  if (statsOpen.value) {
    actionMsg.value = "";
    refreshStats();
    statsTimer = window.setInterval(refreshStats, 2000);
  } else {
    clearInterval(statsTimer);
  }
}

async function cleanDaemon() {
  busy.value = true;
  try {
    const n = await invoke<number>("clean_daemon");
    actionMsg.value = n ? `Reaped ${n} dead session${n === 1 ? "" : "s"}` : "No dead sessions";
    await refreshStats();
  } catch (e) {
    actionMsg.value = "Clean failed";
    console.error(e);
  } finally {
    busy.value = false;
  }
}

async function restartDaemon() {
  busy.value = true;
  actionMsg.value = "Restarting…";
  try {
    const pid = await invoke<number>("restart_daemon");
    actionMsg.value = `Daemon restarted (pid ${pid})`;
    await refreshStats();
  } catch (e) {
    actionMsg.value = "Restart failed";
    console.error(e);
  } finally {
    busy.value = false;
  }
}

async function openIn(target: "finder" | "vscode" | "zed") {
  menuOpen.value = false;
  if (!props.folderPath) return;
  try {
    await invoke("open_path_in", { path: props.folderPath, target });
  } catch (e) {
    console.error("open_path_in failed", e);
  }
}

function onDocClick() {
  menuOpen.value = false;
  notifOpen.value = false;
  if (statsOpen.value) { statsOpen.value = false; clearInterval(statsTimer); }
}
onMounted(() => window.addEventListener("click", onDocClick));
onBeforeUnmount(() => { window.removeEventListener("click", onDocClick); clearInterval(statsTimer); });

const isDev = import.meta.env.DEV;
</script>

<style scoped>
.titlebar {
  height: var(--titlebar-height);
  background: var(--bg-panel);
  backdrop-filter: var(--backdrop-blur, none);
  -webkit-backdrop-filter: var(--backdrop-blur, none);
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
.tb-menu-item:disabled { opacity: 0.4; cursor: default; }
.tb-menu-item:disabled:hover { background: none; color: var(--text-secondary); }
.tb-menu-item.danger:hover { background: rgba(220, 60, 60, 0.15); color: #ff7676; }

.stats-menu { min-width: 200px; padding: 8px; }

.stat-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-family: var(--font-ui);
  font-size: 12px;
  color: var(--text-secondary);
  padding: 2px 2px;
}
.stat-label { display: flex; align-items: center; gap: 6px; }
.stat-val { font-family: var(--font-mono); font-size: 11px; color: var(--text-primary); }
.stat-val.off { color: #ff7676; }

.stat-bar {
  height: 4px;
  background: var(--bg-hover);
  border-radius: 2px;
  overflow: hidden;
  margin: 3px 2px 8px;
}
.stat-bar-fill {
  height: 100%;
  background: var(--accent);
  border-radius: 2px;
  transition: width 0.4s ease;
}

.stat-pid {
  font-family: var(--font-mono);
  font-size: 10px;
  color: var(--text-muted);
  padding: 0 2px 2px;
}

.stat-sep { height: 1px; background: var(--border); margin: 6px 0; }

.stat-msg {
  font-family: var(--font-ui);
  font-size: 11px;
  color: var(--text-muted);
  padding: 6px 2px 2px;
  text-align: center;
}

/* Claude 5h usage widget */
.claude-usage {
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 3px 8px 3px 6px;
  border-radius: 6px;
  border: 1px solid var(--border);
  background: var(--bg-hover);
  cursor: default;
  flex-shrink: 0;
  margin-left: 4px;
  -webkit-app-region: no-drag;
}

.usage-icon { color: #d97706; flex-shrink: 0; }
.usage-empty { opacity: 0.45; }

.usage-bar-wrap {
  width: 36px;
  height: 3px;
  background: rgba(255,255,255,0.08);
  border-radius: 2px;
  overflow: hidden;
  flex-shrink: 0;
}

.usage-bar-fill {
  height: 100%;
  background: var(--accent);
  border-radius: 2px;
  transition: width 0.4s ease, background 0.2s;
}
.usage-bar-fill.usage-warn { background: #f59e0b; }
.usage-bar-fill.usage-crit { background: var(--red); }

.usage-label {
  font-size: 10px;
  font-family: var(--font-mono);
  color: var(--text-secondary);
  white-space: nowrap;
}

.usage-window {
  color: var(--text-muted);
  font-size: 9px;
  margin-left: 1px;
}

/* Notification center */
.titlebar-notif {
  margin-left: 4px;
  -webkit-app-region: no-drag;
  flex-shrink: 0;
}

.notif-btn {
  position: relative;
}
.notif-btn.has-unread { color: var(--green); }

.notif-badge {
  position: absolute;
  top: 1px;
  right: 1px;
  min-width: 14px;
  height: 14px;
  padding: 0 3px;
  border-radius: 7px;
  background: var(--green);
  color: #000;
  font-size: 8px;
  font-weight: 700;
  line-height: 14px;
  text-align: center;
  pointer-events: none;
}

.notif-menu {
  left: 0;
  right: auto;
  min-width: 280px;
  max-width: 320px;
  padding: 0;
  overflow: hidden;
}

.notif-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 10px 6px;
  border-bottom: 1px solid var(--border);
}
.notif-title {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-primary);
}
.notif-clear-btn {
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  font-size: 10px;
  padding: 2px 4px;
  border-radius: 3px;
}
.notif-clear-btn:hover { color: var(--text-secondary); background: var(--bg-hover); }

.notif-empty {
  font-size: 12px;
  color: var(--text-muted);
  text-align: center;
  padding: 20px 12px;
}

.notif-list {
  max-height: 320px;
  overflow-y: auto;
  padding: 4px;
}

.notif-item {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 7px 8px;
  border-radius: 5px;
  cursor: default;
}
.notif-item:hover { background: var(--bg-hover); }
.notif-clickable { cursor: pointer; }

.notif-icon { flex-shrink: 0; margin-top: 1px; }
.notif-done  .notif-icon { color: var(--green); }
.notif-error .notif-icon { color: var(--red); }
.notif-info  .notif-icon { color: var(--accent); }

.notif-body { flex: 1; min-width: 0; }
.notif-item-title {
  font-size: 11px;
  font-weight: 500;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.notif-item-body {
  font-size: 10px;
  color: var(--text-secondary);
  margin-top: 1px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.notif-time {
  flex-shrink: 0;
  font-size: 9px;
  color: var(--text-muted);
  margin-top: 2px;
}
</style>
