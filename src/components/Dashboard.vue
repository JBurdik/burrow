<!--
  Dashboard.vue — the "home" landing view (first activity-bar item).

  A full main-pane mode (ui.mode === 'dashboard'), same slot as GitPanel /
  MissionControl. Four panels:
    1. Agent activity   — cross-workspace roll-up of terminal-tab statuses
                          (running/waiting/review/done) + an "attention" list.
    2. Quick actions    — launcher buttons (new terminal/chat/workspace, Mission
                          Control, settings).
    3. Workspaces       — card grid of every top-level repo + its worktrees, with
                          live git branch / dirty count, click to open.
  Reads the workspace + terminalTabs stores; pulls per-workspace git state via the
  `run_git` Tauri command directly (the git store only tracks the active cwd).
-->
<template>
  <div class="dash">
    <header class="dash-head">
      <PhSquaresFour :size="20" weight="bold" class="brand-mark" />
      <h1>Dashboard</h1>
      <span class="spacer" />
      <button class="btn ghost xs" @click="refreshGit" :disabled="gitBusy">
        <PhArrowsClockwise :size="13" :class="{ spin: gitBusy }" /> refresh
      </button>
    </header>

    <div class="dash-grid">
      <!-- ── Agent activity ──────────────────────────────────────────── -->
      <section class="card activity">
        <h2>Agent activity</h2>
        <div class="summary">
          <span class="chip" :class="{ on: counts.running }"><em class="dot running" />{{ counts.running }}<small>running</small></span>
          <span class="chip" :class="{ on: counts.waiting }"><em class="dot waiting" />{{ counts.waiting }}<small>waiting</small></span>
          <span class="chip" :class="{ on: counts.review }"><em class="dot review" />{{ counts.review }}<small>review</small></span>
          <span class="chip" :class="{ on: counts.done }"><em class="dot done" />{{ counts.done }}<small>done</small></span>
        </div>

        <div v-if="attention.length" class="attn-list">
          <button
            v-for="a in attention"
            :key="a.wsId + ':' + a.tabId"
            class="attn-row"
            @click="goToTab(a)"
          >
            <em class="dot" :class="a.status" />
            <span class="attn-title">{{ a.title }}</span>
            <span class="attn-ws">{{ a.wsName }}</span>
          </button>
        </div>
        <p v-else class="empty">No agents need attention.</p>
      </section>

      <!-- ── Quick actions ───────────────────────────────────────────── -->
      <section class="card actions">
        <h2>Quick actions</h2>
        <div class="action-grid">
          <button class="action" :disabled="!ws.active" @click="newTerminal">
            <PhTerminal :size="18" /><span>New terminal</span>
          </button>
          <button class="action" :disabled="!ws.active" @click="newChat">
            <ClaudeIcon :size="18" /><span>New chat</span>
          </button>
          <button class="action" @click="$emit('new-workspace')">
            <PhFolderPlus :size="18" /><span>New workspace</span>
          </button>
          <button class="action" @click="ui.setMode('mission')">
            <PhRocketLaunch :size="18" /><span>Mission Control</span>
          </button>
          <button class="action" @click="ui.openSettings()">
            <PhGear :size="18" /><span>Settings</span>
          </button>
        </div>
      </section>

      <!-- ── Workspaces ──────────────────────────────────────────────── -->
      <section class="card workspaces">
        <h2>Workspaces <small>{{ ws.topLevel.length }}</small></h2>
        <div v-if="!ws.topLevel.length" class="empty">No workspaces yet.</div>
        <div class="ws-grid">
          <button
            v-for="w in ws.topLevel"
            :key="w.id"
            class="ws-card"
            :class="{ active: ws.active?.id === w.id }"
            @click="openWs(w)"
          >
            <div class="ws-card-head">
              <img v-if="ws.icons[w.id]" class="ws-ico-img" :src="ws.icons[w.id]" alt="" />
              <PhFolder v-else :size="16" weight="fill" class="ws-ico" />
              <span class="ws-name">{{ w.name }}</span>
              <em v-if="wsAgg(w.id) !== 'idle'" class="dot" :class="wsAgg(w.id)" :title="wsAgg(w.id)" />
            </div>
            <div class="ws-meta">
              <span class="ws-branch" v-if="gitByWs[w.id]?.branch">
                <PhGitBranch :size="12" />{{ gitByWs[w.id].branch }}
              </span>
              <span class="ws-dirty" v-if="gitByWs[w.id]?.dirty" :title="gitByWs[w.id].dirty + ' changed files'">
                ●{{ gitByWs[w.id].dirty }}
              </span>
              <span class="ws-sync" v-if="gitByWs[w.id]?.ahead">↑{{ gitByWs[w.id].ahead }}</span>
              <span class="ws-sync" v-if="gitByWs[w.id]?.behind">↓{{ gitByWs[w.id].behind }}</span>
            </div>
            <div class="ws-path">{{ shortCwd(w.path) }}</div>
            <!-- worktrees nested under the repo -->
            <ul v-if="(ws.worktreesByParent[w.id] || []).length" class="wt-list" @click.stop>
              <li
                v-for="wt in ws.worktreesByParent[w.id]"
                :key="wt.id"
                class="wt-row"
                @click="openWs(wt)"
              >
                <PhGitBranch :size="11" />
                <span class="wt-name">{{ wt.worktree_branch || wt.name }}</span>
                <em v-if="wsAgg(wt.id) !== 'idle'" class="dot" :class="wsAgg(wt.id)" />
              </li>
            </ul>
          </button>
        </div>
      </section>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, reactive, onMounted, onBeforeUnmount, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import {
  PhSquaresFour, PhTerminal, PhFolder, PhFolderPlus, PhGitBranch,
  PhRocketLaunch, PhGear, PhArrowsClockwise,
} from "@phosphor-icons/vue";
import ClaudeIcon from "@/components/icons/ClaudeIcon.vue";
import { useWorkspaceStore, type Workspace } from "@/stores/workspace";
import { useTerminalTabsStore } from "@/stores/terminalTabs";
import { useUIStore } from "@/stores/ui";
import { aggregateStatus, type TermStatus } from "@/lib/terminalStatus";

defineEmits<{ (e: "new-workspace"): void }>();

const ws = useWorkspaceStore();
const termTabs = useTerminalTabsStore();
const ui = useUIStore();

// ── Agent activity ──────────────────────────────────────────────────────────
// Flatten every workspace's tab summaries into one list (tagged with workspace).
const allTabs = computed(() =>
  Object.entries(termTabs.tabsByWs).flatMap(([wsId, tabs]) =>
    (tabs || []).map((t) => ({ ...t, wsId: Number(wsId) })),
  ),
);

const counts = computed(() => {
  const c = { running: 0, waiting: 0, review: 0, done: 0 };
  for (const t of allTabs.value) {
    if (t.status in c) (c as Record<string, number>)[t.status]++;
  }
  return c;
});

// Tabs that want the user's eyes, highest-priority first.
const ATTN_ORDER: TermStatus[] = ["permission", "waiting", "review", "done"];
const attention = computed(() => {
  const wsName = (id: number) => ws.workspaces.find((w) => w.id === id)?.name ?? "?";
  return allTabs.value
    .filter((t) => ATTN_ORDER.includes(t.status))
    .map((t) => ({ wsId: t.wsId, tabId: t.id, title: t.title, status: t.status, wsName: wsName(t.wsId) }))
    .sort((a, b) => ATTN_ORDER.indexOf(a.status) - ATTN_ORDER.indexOf(b.status));
});

// Per-workspace aggregate status for the card dot.
function wsAgg(wsId: number): TermStatus {
  const tabs = termTabs.tabsByWs[wsId] || [];
  return aggregateStatus(tabs, (t) => t.status);
}

function goToTab(a: { wsId: number; tabId: number }) {
  const target = ws.workspaces.find((w) => w.id === a.wsId);
  if (target) ws.open(target);
  ui.setMode("terminal");
  setTimeout(() => termTabs.activate(a.wsId, a.tabId), 60);
}

// ── Quick actions ─────────────────────────────────────────────────────────────
function newTerminal() {
  if (!ws.active) return;
  termTabs.add(ws.active.id);
  ui.setMode("terminal");
}
function newChat() {
  if (!ws.active) return;
  termTabs.openChat(ws.active.id);
  ui.setMode("terminal");
}
function openWs(w: Workspace) {
  ws.open(w);
  ui.setMode("terminal");
}

// ── Per-workspace git state ─────────────────────────────────────────────────
interface GitInfo { branch: string; dirty: number; ahead: number; behind: number; }
const gitByWs = reactive<Record<number, GitInfo>>({});
const gitBusy = ref(false);

interface GitOutput { stdout: string; stderr: string; code: number; }

async function gitFor(path: string): Promise<GitInfo | null> {
  try {
    const [status, branch, upstream] = await Promise.all([
      invoke<GitOutput>("run_git", { cwd: path, args: ["status", "--porcelain"] }),
      invoke<GitOutput>("run_git", { cwd: path, args: ["branch", "--show-current"] }),
      invoke<GitOutput>("run_git", { cwd: path, args: ["rev-list", "--left-right", "--count", "@{upstream}...HEAD"] }),
    ]);
    if (branch.code !== 0) return null; // not a git repo
    const dirty = status.stdout.split("\n").filter((l) => l.trim().length > 0).length;
    let ahead = 0, behind = 0;
    if (upstream.code === 0) {
      const [b, a] = upstream.stdout.trim().split(/\s+/);
      behind = parseInt(b, 10) || 0;
      ahead = parseInt(a, 10) || 0;
    }
    return { branch: branch.stdout.trim(), dirty, ahead, behind };
  } catch {
    return null; // browser-only dev (no Tauri) or non-repo
  }
}

async function refreshGit() {
  if (gitBusy.value) return;
  gitBusy.value = true;
  try {
    const targets = [
      ...ws.topLevel,
      ...Object.values(ws.worktreesByParent).flat(),
    ];
    await Promise.all(
      targets.map(async (w) => {
        const info = await gitFor(w.path);
        if (info) gitByWs[w.id] = info;
        else delete gitByWs[w.id];
      }),
    );
  } finally {
    gitBusy.value = false;
  }
}

// ── Misc ────────────────────────────────────────────────────────────────────
function shortCwd(p: string): string {
  const home = "/Users/";
  let s = p;
  if (s.startsWith(home)) {
    const rest = s.slice(home.length).split("/").slice(1).join("/");
    s = "~/" + rest;
  }
  return s;
}

// Refresh git on mount and on a slow poll while the dashboard is visible.
let timer: number | undefined;
onMounted(() => {
  refreshGit();
  timer = window.setInterval(() => {
    if (ui.mode === "dashboard") refreshGit();
  }, 8000);
});
onBeforeUnmount(() => { if (timer) clearInterval(timer); });

// Re-scan when workspaces change (created/removed) while open.
watch(() => ws.workspaces.length, () => refreshGit());
</script>

<style scoped>
.dash {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--bg-base);
  color: var(--text-primary);
  overflow: hidden;
}

.dash-head {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 14px 20px;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}
.dash-head h1 { font-size: 16px; font-weight: 650; margin: 0; }
.brand-mark { color: var(--accent); }
.spacer { flex: 1; }

.dash-grid {
  flex: 1;
  overflow-y: auto;
  padding: 20px;
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 16px;
  align-content: start;
}
.workspaces { grid-column: 1 / -1; }

.card {
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: 12px;
  padding: 16px;
}
.card h2 {
  font-size: 12px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: .04em;
  color: var(--text-muted);
  margin: 0 0 12px;
}
.card h2 small { color: var(--text-muted); opacity: .6; font-weight: 500; }

/* Activity summary */
.summary { display: flex; gap: 8px; flex-wrap: wrap; margin-bottom: 12px; }
.chip {
  display: inline-flex; align-items: center; gap: 6px;
  padding: 5px 10px; border-radius: 8px;
  background: var(--bg-hover); font-size: 13px; font-weight: 600;
  opacity: .55;
}
.chip.on { opacity: 1; }
.chip small { color: var(--text-muted); font-weight: 500; font-size: 11px; }

.attn-list { display: flex; flex-direction: column; gap: 4px; }
.attn-row {
  display: flex; align-items: center; gap: 8px;
  width: 100%; text-align: left;
  background: none; border: none; cursor: pointer;
  padding: 7px 8px; border-radius: 7px; color: var(--text-primary);
}
.attn-row:hover { background: var(--bg-hover); }
.attn-title { flex: 1; font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.attn-ws { font-size: 11px; color: var(--text-muted); }

.empty { font-size: 13px; color: var(--text-muted); margin: 4px 0 0; }

/* Quick actions */
.action-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(120px, 1fr)); gap: 8px; }
.action {
  display: flex; flex-direction: column; align-items: center; gap: 7px;
  padding: 14px 8px; border-radius: 10px;
  background: var(--bg-hover); border: 1px solid transparent;
  color: var(--text-primary); cursor: pointer; font-size: 12px; font-weight: 550;
  transition: border-color .12s, color .12s;
}
.action:hover:not(:disabled) { border-color: var(--accent); color: var(--accent); }
.action:disabled { opacity: .4; cursor: default; }

/* Workspaces grid */
.ws-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 10px; }
.ws-card {
  display: flex; flex-direction: column; gap: 6px;
  text-align: left; padding: 12px 14px; border-radius: 10px;
  background: var(--bg-hover); border: 1px solid var(--border);
  color: var(--text-primary); cursor: pointer;
  transition: border-color .12s;
}
.ws-card:hover { border-color: var(--accent); }
.ws-card.active { border-color: var(--accent); box-shadow: 0 0 0 1px var(--accent) inset; }
.ws-card-head { display: flex; align-items: center; gap: 8px; }
.ws-ico { color: var(--text-muted); flex-shrink: 0; }
.ws-ico-img { width: 16px; height: 16px; border-radius: 4px; object-fit: cover; flex-shrink: 0; }
.ws-name { font-size: 13px; font-weight: 600; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.ws-meta { display: flex; align-items: center; gap: 8px; font-size: 11px; color: var(--text-muted); }
.ws-branch { display: inline-flex; align-items: center; gap: 3px; }
.ws-dirty { color: var(--yellow); font-weight: 600; }
.ws-sync { color: var(--accent); }
.ws-path { font-size: 11px; color: var(--text-muted); opacity: .7; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

.wt-list { list-style: none; margin: 4px 0 0; padding: 6px 0 0; border-top: 1px solid var(--border); display: flex; flex-direction: column; gap: 2px; }
.wt-row { display: flex; align-items: center; gap: 6px; font-size: 11px; color: var(--text-muted); padding: 3px 4px; border-radius: 5px; }
.wt-row:hover { background: var(--bg-panel); color: var(--text-primary); }
.wt-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

/* Status dots (same palette as MissionControl / Sidebar) */
.dot { width: 8px; height: 8px; border-radius: 50%; display: inline-block; background: var(--text-muted); flex-shrink: 0; }
.dot.running { background: var(--yellow); box-shadow: 0 0 8px color-mix(in srgb, var(--yellow) 53%, transparent); animation: pulse 1.4s infinite; }
.dot.waiting { background: var(--accent); box-shadow: 0 0 8px color-mix(in srgb, var(--accent) 53%, transparent); }
.dot.permission { background: var(--accent); box-shadow: 0 0 8px color-mix(in srgb, var(--accent) 53%, transparent); animation: pulse 1.4s infinite; }
.dot.review { background: var(--green); box-shadow: 0 0 8px color-mix(in srgb, var(--green) 53%, transparent); animation: pulse 1.8s infinite; }
.dot.done { background: var(--green); box-shadow: 0 0 8px color-mix(in srgb, var(--green) 53%, transparent); }

.btn.ghost { background: none; border: 1px solid var(--border); color: var(--text-muted); border-radius: 7px; cursor: pointer; display: inline-flex; align-items: center; gap: 5px; }
.btn.ghost:hover { color: var(--text-primary); }
.btn.xs { font-size: 11px; padding: 4px 9px; }
.btn:disabled { opacity: .5; cursor: default; }
.spin { animation: spin 1s linear infinite; }

@keyframes pulse { 0%,100% { opacity: 1; } 50% { opacity: .4; } }
@keyframes spin { to { transform: rotate(360deg); } }
</style>
