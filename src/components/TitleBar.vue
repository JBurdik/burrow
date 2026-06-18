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
            @click="navigateToNotif(item.workspaceId, item.tabId)"
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

    <!-- Claude plan-usage strip — real utilization %, same data claude.ai shows.
         One bar per limit window (5h session, weekly, weekly-Sonnet). -->
    <div
      v-if="usageBars.length"
      class="usage-strip"
      :class="{ error: !!usageError }"
      :title="usageError ? `usage unavailable: ${usageError}` : 'claude plan usage'"
      data-tauri-drag-region
    >
      <ClaudeIcon :size="11" class="usage-icon" />
      <span
        v-for="b in usageBars"
        :key="b.key"
        class="usage-bar"
        :class="usageSeverity(b.pct)"
        :title="usageBarTitle(b)"
      >
        <span class="ub-label">{{ b.label }}</span>
        <span class="ub-track"><span class="ub-fill" :style="{ width: Math.min(b.pct, 100) + '%' }" /></span>
        <span class="ub-pct">{{ b.pct }}%</span>
      </span>
    </div>

    <div class="titlebar-center" data-tauri-drag-region>
      <button v-if="workspaceName" class="back-btn" @click="$emit('back')" title="Switch workspace">
        <PhHouse :size="13" />
      </button>
      <span class="project-name" data-tauri-drag-region>{{ workspaceName || "Burrow" }}</span>
      <div v-if="branch" class="tb-branch-wrap">
        <button
          class="branch-btn"
          :title="`Branch: ${branch} — click to switch`"
          @click.stop="openBranchPicker"
        >
          <PhGitBranch :size="11" />
          {{ branch }}
        </button>
        <div v-if="branchPickerOpen" class="tb-branch-picker" @click.stop>
          <input
            ref="branchInputEl"
            v-model="branchFilter"
            class="tb-branch-filter"
            placeholder="Switch or create branch…"
            @keydown.enter="onBranchEnter"
            @keydown.esc="branchPickerOpen = false"
          />
          <div class="tb-branch-list">
            <div v-if="branchLoading" class="tb-branch-item tb-branch-loading">Loading…</div>
            <template v-else>
              <div
                v-for="b in filteredBranches"
                :key="b"
                class="tb-branch-item"
                :class="{ 'tb-branch-current': b === branch }"
                @click="switchBranch(b)"
              >
                <PhGitBranch :size="10" />
                <span>{{ b }}</span>
                <span v-if="b === branch" class="tb-branch-check">✓</span>
              </div>
              <div
                v-if="showCreateOption"
                class="tb-branch-item tb-branch-create"
                @click="createBranch(branchFilter.trim())"
              >
                <PhPlus :size="10" />
                <span>Create "{{ branchFilter.trim() }}"</span>
              </div>
              <div v-if="!branchLoading && filteredBranches.length === 0 && !showCreateOption" class="tb-branch-empty">
                No branches found
              </div>
            </template>
          </div>
        </div>
      </div>
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
          <button
            class="tb-menu-item"
            :disabled="busy"
            title="Kill alive PTYs that no open or saved tab references (closed-tab leftovers)"
            @click="killOrphans"
          >
            <PhSkull :size="14" />Kill orphaned sessions
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
import { ref, computed, onMounted, onBeforeUnmount, nextTick } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { PhHouse, PhGitBranch, PhSidebarSimple, PhFolderOpen, PhGear, PhCaretDown, PhFolderNotchOpen, PhCode, PhLightning, PhGauge, PhCpu, PhMemory, PhStack, PhBroom, PhArrowsClockwise, PhBell, PhCheckCircle, PhWarning, PhInfo, PhPlus, PhSkull } from "@phosphor-icons/vue";
import { useNotificationsStore } from "@/stores/notifications";
import { useWorkspaceStore } from "@/stores/workspace";
import { useGitStore } from "@/stores/git";
import { useTerminalTabsStore } from "@/stores/terminalTabs";
import ClaudeIcon from "@/components/icons/ClaudeIcon.vue";

const props = defineProps<{ workspaceName?: string; branch?: string; folderPath?: string; rightPanelVisible?: boolean }>();
defineEmits(["back", "toggle-rightpanel", "open-settings"]);

const menuOpen = ref(false);

// ── Branch picker ───────────────────────────────────────────────────────────
const git = useGitStore();
const branchPickerOpen = ref(false);
const branchFilter = ref("");
const branchLoading = ref(false);
const branchInputEl = ref<HTMLInputElement | null>(null);

const filteredBranches = computed(() => {
  const q = branchFilter.value.toLowerCase();
  return q ? git.branches.filter(b => b.toLowerCase().includes(q)) : git.branches;
});
const showCreateOption = computed(() => {
  const q = branchFilter.value.trim();
  return q && !git.branches.includes(q);
});

async function openBranchPicker() {
  if (branchPickerOpen.value) { branchPickerOpen.value = false; return; }
  if (!props.folderPath) return;
  branchPickerOpen.value = true;
  branchFilter.value = "";
  branchLoading.value = true;
  try {
    await git.fetchBranches();
  } finally {
    branchLoading.value = false;
    await nextTick();
    branchInputEl.value?.focus();
  }
}

async function switchBranch(name: string) {
  branchPickerOpen.value = false;
  try { await git.switchBranch(name); }
  catch (e) { console.error("branch switch failed", e); }
}

async function createBranch(name: string) {
  if (!name) return;
  branchPickerOpen.value = false;
  try { await git.createBranch(name); }
  catch (e) { console.error("branch create failed", e); }
}

function onBranchEnter() {
  if (filteredBranches.value.length === 1) { switchBranch(filteredBranches.value[0]); return; }
  if (showCreateOption.value) createBranch(branchFilter.value.trim());
}

// ── Notification center ─────────────────────────────────────────────────────
const notifStore = useNotificationsStore();
const notifOpen = ref(false);
const wsStore = useWorkspaceStore();
const termTabs = useTerminalTabsStore();

function navigateToNotif(workspaceId?: number, tabId?: number) {
  if (!workspaceId) return;
  const ws = wsStore.workspaces.find((w) => w.id === workspaceId);
  if (ws) {
    wsStore.open(ws);
    if (tabId != null) termTabs.activate(workspaceId, tabId);
  }
  notifOpen.value = false;
}

// ── Claude plan-usage strip ──────────────────────────────────────────────────
// Real utilization % from the OAuth usage endpoint (Rust `claude_plan_usage`),
// the same numbers claude.ai's UI shows. Polled every 60s; Rust caches 60s.
type UsageWindow = { utilization: number; resets_at?: string };
type PlanUsage = Record<string, UsageWindow | undefined>;
type UsageBar = { key: string; label: string; pct: number; resets?: string };

const planUsage = ref<PlanUsage | null>(null);
const usageError = ref<string | null>(null);
let usageTimer: number | undefined;

async function refreshUsage() {
  try {
    const j = await invoke<{ ok: boolean; usage?: PlanUsage; error?: string }>("claude_plan_usage");
    if (j?.ok && j.usage) {
      planUsage.value = j.usage;
      usageError.value = null;
    } else {
      usageError.value = j?.error || "unknown";
    }
  } catch (e) {
    usageError.value = "invoke_failed";
    console.error("claude_plan_usage failed", e);
  }
}

// One bar per limit window. Sonnet/Opus weekly bars only appear once used —
// they read 0% on plans that don't split per-model, so showing them is noise.
const usageBars = computed<UsageBar[]>(() => {
  const u = planUsage.value;
  if (!u) return [];
  const out: UsageBar[] = [];
  const add = (key: string, label: string, hideZero = false) => {
    const w = u[key];
    if (!w) return;
    const pct = Math.round(w.utilization || 0);
    if (hideZero && pct <= 0) return;
    out.push({ key, label, pct, resets: w.resets_at });
  };
  add("five_hour", "5h");
  add("seven_day", "wk");
  add("seven_day_sonnet", "son", true);
  add("seven_day_opus", "opus", true);
  return out;
});

function usageSeverity(pct: number): string {
  if (pct >= 85) return "crit";
  if (pct >= 60) return "warn";
  return "";
}

function relTimeFuture(iso?: string): string {
  if (!iso) return "";
  let s = Math.round((new Date(iso).getTime() - Date.now()) / 1000);
  if (s <= 0) return "now";
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60), mm = m % 60;
  if (h < 24) return mm ? `${h}h ${mm}m` : `${h}h`;
  const d = Math.floor(h / 24), hh = h % 24;
  return hh ? `${d}d ${hh}h` : `${d}d`;
}

function usageBarTitle(b: UsageBar): string {
  const reset = b.resets ? ` · resets in ${relTimeFuture(b.resets)}` : "";
  return `${b.label}: ${b.pct}% used${reset}`;
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

async function killOrphans() {
  busy.value = true;
  try {
    // Every pty id the UI currently knows is live (across all opened workspaces).
    // Rust unions this with the saved terminal_tabs rows so reattachable / not-yet-
    // opened sessions are never killed — only true closed-tab leftovers.
    const keepIds = Object.values(termTabs.tabsByWs).flat().map((t) => t.id);
    const n = await invoke<number>("kill_orphan_sessions", { keepIds });
    actionMsg.value = n ? `Killed ${n} orphaned session${n === 1 ? "" : "s"}` : "No orphaned sessions";
    await refreshStats();
  } catch (e) {
    actionMsg.value = "Kill failed";
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
  branchPickerOpen.value = false;
  if (statsOpen.value) { statsOpen.value = false; clearInterval(statsTimer); }
}
onMounted(() => {
  window.addEventListener("click", onDocClick);
  refreshUsage();
  usageTimer = window.setInterval(refreshUsage, 60_000);
});
onBeforeUnmount(() => {
  window.removeEventListener("click", onDocClick);
  clearInterval(statsTimer);
  clearInterval(usageTimer);
});

const isDev = import.meta.env.DEV;
</script>

<style scoped>
.titlebar {
  height: var(--titlebar-height);
  background: var(--bg-panel);
  backdrop-filter: var(--blur-panels, none);
  -webkit-backdrop-filter: var(--blur-panels, none);
  border-bottom: 1px solid var(--border);
  display: flex;
  align-items: center;
  flex-shrink: 0;
  /* backdrop-filter makes this a stacking context; without an explicit
     z-index its dropdowns paint *below* .ide-body's positioned children
     (resize handles, panels) and become unreachable. Lift the whole bar. */
  position: relative;
  z-index: 100;
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

.tb-branch-wrap {
  position: relative;
  -webkit-app-region: no-drag;
}

.branch-btn {
  display: flex;
  align-items: center;
  gap: 3px;
  background: none;
  border: 1px solid color-mix(in srgb, var(--border) 70%, transparent);
  border-radius: 6px;
  color: var(--text-muted);
  cursor: pointer;
  font-family: var(--font-mono);
  font-size: 10px;
  padding: 2px 6px;
  transition: color .12s, border-color .12s, background .12s;
}
.branch-btn:hover {
  color: var(--text-secondary);
  border-color: var(--border);
  background: var(--bg-hover);
}

.tb-branch-picker {
  position: absolute;
  top: calc(100% + 5px);
  left: 50%;
  transform: translateX(-50%);
  width: 220px;
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: 6px;
  overflow: hidden;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.45);
  z-index: 2000;
}

.tb-branch-filter {
  width: 100%;
  background: transparent;
  border: none;
  border-bottom: 1px solid var(--border);
  color: var(--text-primary);
  font-size: 11px;
  outline: none;
  padding: 7px 9px;
  box-sizing: border-box;
  font-family: var(--font-mono);
}
.tb-branch-filter::placeholder { color: var(--text-muted); }

.tb-branch-list { max-height: 180px; overflow-y: auto; }

.tb-branch-item {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 9px;
  font-size: 11px;
  font-family: var(--font-mono);
  color: var(--text-secondary);
  cursor: pointer;
}
.tb-branch-item:hover { background: var(--bg-hover); color: var(--text-primary); }
.tb-branch-current { color: var(--accent); }
.tb-branch-create { color: var(--text-muted); font-style: italic; }
.tb-branch-create:hover { color: var(--text-primary); background: var(--bg-hover); }
.tb-branch-check { margin-left: auto; color: var(--accent); font-style: normal; }
.tb-branch-loading { color: var(--text-muted); font-style: italic; }
.tb-branch-empty { color: var(--text-muted); font-size: 10px; padding: 10px; text-align: center; }

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
  background: var(--bg-dropdown, var(--bg-panel));
  backdrop-filter: var(--blur-dropdown, blur(18px)) saturate(140%);
  -webkit-backdrop-filter: var(--blur-dropdown, blur(18px)) saturate(140%);
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 4px;
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.5);
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

/* Claude plan-usage strip — one bar per limit window */
.usage-strip {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 3px 9px;
  border-radius: 6px;
  border: 1px solid var(--border);
  background: var(--bg-hover);
  cursor: default;
  flex-shrink: 0;
  margin-left: 4px;
  -webkit-app-region: no-drag;
}
.usage-strip.error { opacity: 0.5; }
.usage-icon { color: #d97706; flex-shrink: 0; }

.usage-bar {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 10px;
  font-family: var(--font-mono);
  color: var(--text-secondary);
}
.usage-bar .ub-label {
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: var(--text-muted);
}
.usage-bar .ub-track {
  display: inline-block;
  width: 44px;
  height: 4px;
  background: rgba(255,255,255,0.08);
  border: 1px solid var(--border);
  border-radius: 3px;
  overflow: hidden;
}
.usage-bar .ub-fill {
  display: block;
  height: 100%;
  width: 0%;
  background: var(--green);
  transition: width 0.4s ease, background 0.2s;
}
.usage-bar .ub-pct {
  font-variant-numeric: tabular-nums;
  min-width: 26px;
  text-align: right;
}

.usage-bar.warn { color: var(--yellow); }
.usage-bar.warn .ub-label { color: var(--yellow); }
.usage-bar.warn .ub-fill { background: var(--yellow); }

.usage-bar.crit { color: var(--red); animation: usage-pulse 1.6s ease-in-out infinite; }
.usage-bar.crit .ub-label { color: var(--red); }
.usage-bar.crit .ub-fill { background: var(--red); }
.usage-bar.crit .ub-track { border-color: var(--red); background: rgba(248,81,73,0.18); }
@keyframes usage-pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.45; } }
@media (prefers-reduced-motion: reduce) { .usage-bar.crit { animation: none; } }

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
