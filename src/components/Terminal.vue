<template>
  <div class="terminal-pane" @click="focusActive">
    <AgentToolbar @launch="spawnAgent" />

    <div v-if="tabs.length > 0" class="terminal-tabs">
      <button
        v-for="tab in tabs"
        :key="tab.id"
        class="tab"
        :class="{ active: activeTabId === tab.id }"
        @click.stop="activateTab(tab.id)"
      >
        <PhRobot v-if="tabIsAgent(tab)" :size="12" class="tab-agent-icon" />
        <PhTerminal v-else :size="12" class="tab-term-icon" />
        <span
          v-if="tabStatus(tab) !== 'idle'"
          class="status-dot"
          :class="`status-${tabStatus(tab)}`"
        >{{ tabStatus(tab) === 'running' ? spinnerFrame : '' }}</span>
        <span class="tab-label">{{ tabTitle(tab) }}</span>
        <PhArrowSquareOut
          :size="10"
          class="tab-float"
          title="Pop out as floating window"
          @click.stop="popOutTab(tab)"
        />
        <PhX
          :size="10"
          weight="bold"
          class="tab-close"
          @click.stop="closeTab(tab.id)"
        />
      </button>
      <button class="tab tab-add" @click="addTab()" title="New terminal">
        <PhPlus :size="12" />
      </button>
    </div>

    <div v-if="tabs.length > 0" class="terminal-body">
      <div
        v-for="tab in tabs"
        :key="tab.id"
        class="terminal-tab-content"
        v-show="activeTabId === tab.id"
      >
        <div
          v-for="pane in paneLayout(tab)"
          :key="pane.leaf.id"
          class="pane"
          :class="{ focused: focusedLeafId === pane.leaf.id && isTabSplit(tab) }"
          :style="rectStyle(pane.rect)"
          @mousedown.capture="onLeafFocus(pane.leaf.id)"
        >
          <div v-if="isTabSplit(tab)" class="pane-titlebar" @mousedown.stop>
            <PhRobot v-if="pane.leaf.isAgent" :size="10" class="pane-title-icon agent" />
            <PhTerminal v-else :size="10" class="pane-title-icon" />
            <span
              v-if="pane.leaf.status !== 'idle'"
              class="status-dot"
              :class="`status-${pane.leaf.status}`"
            >{{ pane.leaf.status === 'running' ? spinnerFrame : '' }}</span>
            <span class="pane-title-text">{{ pane.leaf.title }}</span>
            <button class="pane-title-close" @click.stop="closePane(pane.leaf.id)" title="Close pane">
              <PhX :size="9" weight="bold" />
            </button>
          </div>
          <DiffTab
            v-if="pane.leaf.leafType === 'diff'"
            :diff-file="pane.leaf.diffFile!"
            :diff-staged="pane.leaf.diffStaged ?? false"
            :diff="pane.leaf.diff || ''"
          />
          <XTerm
            v-else
            :pty-id="pane.leaf.id"
            :cwd="pane.leaf.cwd ?? cwd"
            :initial-cmd="pane.leaf.initialCmd"
            :result-token="pane.leaf.resultToken"
            :ref="(el) => registerLeaf(pane.leaf.id, el)"
            @title="(t) => onLeafTitle(pane.leaf.id, t)"
            @busy="(b) => onLeafBusy(pane.leaf.id, b)"
            @agent="(b) => onLeafAgent(pane.leaf.id, b)"
            @agent-state="(s) => onAgentState(pane.leaf.id, s)"
            @needs-input="(b) => onLeafNeedsInput(pane.leaf.id, b)"
            @interrupt="() => onLeafInterrupt(pane.leaf.id)"
            @spawn="(req) => addTab(req.cmd, { cwd: req.cwd || undefined, resultToken: req.token || undefined })"
          />
        </div>
      </div>
    </div>
    <div v-else class="terminal-welcome">
      <PhTerminalWindow :size="40" weight="thin" class="welcome-icon" />
      <p class="welcome-title">No terminals open</p>
      <p class="welcome-sub">Launch an agent above or open a new terminal</p>
      <button class="welcome-btn" @click="addTab()">
        <PhPlus :size="13" /> New Terminal
      </button>
    </div>

    <div v-if="confirm" class="confirm-overlay" @mousedown.self="answerClose(false)">
      <div class="confirm-modal">
        <div class="confirm-title">Close terminal</div>
        <div class="confirm-body">
          "{{ confirm.name }}" has a running process. Close anyway?
        </div>
        <div class="confirm-actions">
          <button class="confirm-btn" @click="answerClose(false)">Cancel</button>
          <button class="confirm-btn danger" @click="answerClose(true)">Close <span class="confirm-kbd">⌘↵</span></button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, nextTick, onMounted, onBeforeUnmount } from "vue";
import { PhRobot, PhTerminal, PhTerminalWindow, PhX, PhPlus, PhArrowSquareOut } from "@phosphor-icons/vue";
import { invoke } from "@tauri-apps/api/core";
import XTerm from "./XTerm.vue";
import DiffTab from "./DiffTab.vue";
import AgentToolbar from "./AgentToolbar.vue";
import { type Leaf, type TreeNode } from "./TerminalSplitView.vue";
import { nextPtyId, initPtyCounter } from "@/lib/ptyId";
import { spinnerFrame } from "@/lib/spinner";
import { playSound } from "@/lib/sounds";
import { useWorkspaceStore } from "@/stores/workspace";
import { useTerminalTabsStore } from "@/stores/terminalTabs";
import { useNotificationsStore } from "@/stores/notifications";
import { useGitStore } from "@/stores/git";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";

const props = defineProps<{ cwd: string; workspaceId: number }>();
const wsStore = useWorkspaceStore();
const tabsStore = useTerminalTabsStore();
const notifStore = useNotificationsStore();
const gitStore = useGitStore();

interface Tab {
  id: number;
  root: TreeNode;
}

interface PersistedTab {
  title: string | null;
  initial_cmd: string | null;
  pty_id: number | null;
  cwd: string | null;
}

interface DaemonSession {
  pty_id: number;
  cwd: string;
  title: string;
  alive: boolean;
}

const tabs = ref<Tab[]>([]);
const activeTabId = ref(0);
const focusedLeafId = ref(0);
const xtermRefs = new Map<number, InstanceType<typeof XTerm>>();
let terminalCounter = 0;

function registerLeaf(id: number, el: unknown) {
  if (el) xtermRefs.set(id, el as InstanceType<typeof XTerm>);
  else xtermRefs.delete(id);
}

// ── layout ──────────────────────────────────────────────────────────────────
// Flatten the split tree into absolutely-positioned panes (in %). Splitting
// only changes these rects, so existing XTerm instances/PTYs are reused — no
// remount, no blink.
interface Rect { left: number; top: number; width: number; height: number; }

function paneLayout(tab: Tab): { leaf: Leaf; rect: Rect }[] {
  const out: { leaf: Leaf; rect: Rect }[] = [];
  walkLayout(tab.root, { left: 0, top: 0, width: 100, height: 100 }, out);
  return out;
}

function walkLayout(node: TreeNode, rect: Rect, out: { leaf: Leaf; rect: Rect }[]) {
  if (node.type === "leaf") {
    out.push({ leaf: node, rect });
    return;
  }
  if (node.direction === "h") {
    const w = rect.width / 2;
    walkLayout(node.first, { ...rect, width: w }, out);
    walkLayout(node.second, { ...rect, left: rect.left + w, width: w }, out);
  } else {
    const h = rect.height / 2;
    walkLayout(node.first, { ...rect, height: h }, out);
    walkLayout(node.second, { ...rect, top: rect.top + h, height: h }, out);
  }
}

function rectStyle(r: Rect) {
  // 1px insets create thin gaps that show the body background as dividers.
  return {
    left: `calc(${r.left}% + ${r.left > 0 ? 1 : 0}px)`,
    top: `calc(${r.top}% + ${r.top > 0 ? 1 : 0}px)`,
    width: `calc(${r.width}% - ${r.left > 0 ? 1 : 0}px)`,
    height: `calc(${r.height}% - ${r.top > 0 ? 1 : 0}px)`,
  };
}

// ── tree helpers ────────────────────────────────────────────────────────────

function findLeaf(node: TreeNode, id: number): Leaf | null {
  if (node.type === "leaf") return node.id === id ? node : null;
  return findLeaf(node.first, id) || findLeaf(node.second, id);
}

function getFirstLeaf(node: TreeNode): Leaf {
  if (node.type === "leaf") return node;
  return getFirstLeaf(node.first);
}

function getAllLeaves(node: TreeNode): Leaf[] {
  if (node.type === "leaf") return [node];
  return [...getAllLeaves(node.first), ...getAllLeaves(node.second)];
}

function removeLeaf(node: TreeNode, id: number): TreeNode | null {
  if (node.type === "leaf") return node.id === id ? null : node;
  const first = removeLeaf(node.first, id);
  const second = removeLeaf(node.second, id);
  if (!first) return second;
  if (!second) return first;
  return { ...node, first, second };
}

function insertSplit(
  node: TreeNode,
  targetId: number,
  direction: "h" | "v",
  newLeaf: Leaf,
): TreeNode {
  if (node.type === "leaf") {
    if (node.id === targetId)
      return { type: "split", direction, first: node, second: newLeaf };
    return node;
  }
  return {
    ...node,
    first: insertSplit(node.first, targetId, direction, newLeaf),
    second: insertSplit(node.second, targetId, direction, newLeaf),
  };
}

// ── tab helpers ─────────────────────────────────────────────────────────────

function tabTitle(tab: Tab): string {
  const leaf =
    activeTabId.value === tab.id
      ? findLeaf(tab.root, focusedLeafId.value) ?? getFirstLeaf(tab.root)
      : getFirstLeaf(tab.root);
  return leaf.title;
}

function tabIsAgent(tab: Tab): boolean {
  const leaf =
    activeTabId.value === tab.id
      ? findLeaf(tab.root, focusedLeafId.value) ?? getFirstLeaf(tab.root)
      : getFirstLeaf(tab.root);
  return leaf.isAgent;
}

const doneTimers = new Map<number, ReturnType<typeof setTimeout>>();

function makeLeaf(initialCmd?: string, extra?: { cwd?: string; resultToken?: string; id?: number }): Leaf {
  terminalCounter++;
  return {
    type: "leaf",
    id: extra?.id ?? nextPtyId(),
    title: `Terminal ${terminalCounter}`,
    defaultTitle: `Terminal ${terminalCounter}`,
    isAgent: false,
    busy: false,
    status: "idle",
    initialCmd,
    cwd: extra?.cwd,
    resultToken: extra?.resultToken,
  };
}

// ── events from split tree ──────────────────────────────────────────────────

function onLeafFocus(id: number) {
  focusedLeafId.value = id;
  nextTick(() => xtermRefs.get(id)?.focus());
}

function onLeafTitle(id: number, title: string) {
  for (const tab of tabs.value) {
    const leaf = findLeaf(tab.root, id);
    if (!leaf) continue;
    // Title is purely the display name now. Empty → back to "Terminal N". A
    // stray leading robot emoji (older seeds) is stripped. isAgent is NOT derived
    // from the title — it comes from the foreground poll via onLeafAgent, so the
    // robot icon survives even when Claude sets a title that doesn't say "Claude".
    leaf.title = title ? title.replace(/^🤖\s*/, "") : leaf.defaultTitle;
    break;
  }
}

// Whether this leaf is currently running an agent — driven by the foreground
// poll (authoritative), independent of the title text.
function onLeafAgent(id: number, isAgent: boolean) {
  for (const tab of tabs.value) {
    const leaf = findLeaf(tab.root, id);
    if (!leaf) continue;
    leaf.isAgent = isAgent;
    break;
  }
}

// busy comes from the foreground-process poll only — NOT from OSC titles
// (the shell sets the title to the cwd, which must not count as "running").
function onLeafBusy(id: number, busy: boolean) {
  for (const tab of tabs.value) {
    const leaf = findLeaf(tab.root, id);
    if (!leaf) continue;
    const wasBusy = leaf.busy;
    leaf.busy = busy;
    if (busy) {
      clearTimeout(doneTimers.get(id));
      doneTimers.delete(id);
      if (leaf.status !== "waiting") leaf.status = "running";
    } else if (wasBusy) {
      settleDone(leaf, tab);
    }
    break;
  }
}

// True when the user is actively looking at this tab: its workspace is the
// visible one, this tab is the active tab, and the window has OS focus.
function isWatching(tab: Tab): boolean {
  return (
    wsStore.active?.id === props.workspaceId &&
    activeTabId.value === tab.id &&
    document.hasFocus()
  );
}

// An agent turn finished. If the user is watching → transient "done" badge that
// auto-clears after 4s. If they're on another tab/workspace/window → persistent
// "review" badge (Superset-style) that survives until the tab is actually seen,
// so cross-workspace completions aren't missed.
function settleDone(leaf: Leaf, tab: Tab) {
  // A single finished turn can emit two "done" signals back-to-back: a spawned
  // sub-agent's per-launch `capture` Stop hook AND the global `burrow hook` Stop
  // (both map to done), or the idle Notification ping that follows the Stop.
  // Settle only once — if the leaf is already settled (not busy, already
  // done/review) and no new turn has resurrected it via "running", ignore the
  // duplicate so the sound + notification don't fire twice.
  if (!leaf.busy && (leaf.status === "done" || leaf.status === "review")) return;
  leaf.busy = false;
  clearTimeout(doneTimers.get(leaf.id));
  if (isWatching(tab)) {
    leaf.status = "done";
    const t = setTimeout(() => { leaf.status = "idle"; doneTimers.delete(leaf.id); }, 4000);
    doneTimers.set(leaf.id, t);
  } else {
    leaf.status = "review";
    doneTimers.delete(leaf.id);
    // Audible cue only when the user is away (the review case) — not while watching.
    playSound("done");
  }
  notifyDone(leaf.title);
  // Agent turn finished → likely touched files. Refresh git panel silently if
  // it's showing this workspace's repo.
  if (gitStore.cwd === props.cwd) gitStore.refresh(true);
}

// Mark every finished leaf in a tab as seen (user opened/returned to it).
function markTabSeen(tab: Tab) {
  for (const leaf of getAllLeaves(tab.root)) {
    if (leaf.status === "done" || leaf.status === "review") {
      clearTimeout(doneTimers.get(leaf.id));
      doneTimers.delete(leaf.id);
      leaf.status = "idle";
    }
  }
}

// The agent's hook state (running | waiting | done), forwarded verbatim from
// XTerm. ONE semantic event → one clean transition, so a trailing "waiting" can
// never clobber a fresh "done". A new turn arrives as "running", which is the
// only thing that resurrects a finished leaf — exactly right.
function onAgentState(id: number, s: string) {
  for (const tab of tabs.value) {
    const leaf = findLeaf(tab.root, id);
    if (!leaf) continue;
    if (s === "running") {
      clearTimeout(doneTimers.get(id));
      doneTimers.delete(id);
      leaf.busy = true;
      leaf.status = "running";
    } else if (s === "waiting") {
      leaf.busy = true;
      if (leaf.status !== "waiting") playSound("waiting"); // once per transition
      leaf.status = "waiting";
    } else if (s === "done") {
      settleDone(leaf, tab); // sets busy=false, done (watching) or review (away)
    }
    break;
  }
}

async function notifyDone(leafTitle: string) {
  const toastTitle = "Task complete";
  const body = leafTitle || "Agent finished";
  // In-app toast always
  notifStore.push({ type: "done", title: toastTitle, body });
  // System notification when window not focused.
  // Title = "Burrow" so the app name is visible even in dev mode
  // (where macOS shows the terminal emulator name instead of the bundle name).
  if (!document.hasFocus()) {
    let granted = await isPermissionGranted();
    if (!granted) {
      const perm = await requestPermission();
      granted = perm === "granted";
    }
    if (granted) sendNotification({ title: "Burrow", body: `✓ ${body}` });
  }
}

// User pressed ESC / Ctrl+C in the PTY — an agent interrupt. Agents emit no Stop
// hook when a turn is cancelled and the foreground poll never clears an agent's
// "running" (it stays foreground at its prompt), so without this the dot sticks
// orange. The turn was CANCELLED, not completed → settle straight to idle (no
// "done"/"review" badge, no sound). Only act on a live running/waiting leaf so a
// stray ESC at an idle prompt is a harmless no-op.
function onLeafInterrupt(id: number) {
  for (const tab of tabs.value) {
    const leaf = findLeaf(tab.root, id);
    if (!leaf) continue;
    if (leaf.status === "running" || leaf.status === "waiting") {
      clearTimeout(doneTimers.get(id));
      doneTimers.delete(id);
      leaf.busy = false;
      leaf.status = "idle";
    }
    break;
  }
}

function onLeafNeedsInput(id: number, needs: boolean) {
  for (const tab of tabs.value) {
    const leaf = findLeaf(tab.root, id);
    if (!leaf) continue;
    if (leaf.busy) {
      const enteringWait = needs && leaf.status !== "waiting";
      leaf.status = needs ? "waiting" : "running";
      if (enteringWait) playSound("waiting"); // once per transition into waiting
    }
    break;
  }
}

function tabStatus(tab: Tab): "idle" | "running" | "waiting" | "done" | "review" {
  const leaves = getAllLeaves(tab.root);
  if (leaves.some((l) => l.status === "waiting")) return "waiting";
  if (leaves.some((l) => l.status === "running")) return "running";
  if (leaves.some((l) => l.status === "review")) return "review";
  if (leaves.some((l) => l.status === "done")) return "done";
  return "idle";
}

// ── in-app close confirmation ───────────────────────────────────────────────

const confirm = ref<{ name: string; resolve: (ok: boolean) => void } | null>(null);

function confirmClose(name: string): Promise<boolean> {
  return new Promise((resolve) => {
    confirm.value = { name, resolve };
    window.addEventListener("keydown", onConfirmKey);
  });
}

function answerClose(ok: boolean) {
  window.removeEventListener("keydown", onConfirmKey);
  confirm.value?.resolve(ok);
  confirm.value = null;
}

function onConfirmKey(e: KeyboardEvent) {
  if (!confirm.value) return;
  if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
    e.preventDefault();
    answerClose(true);
  } else if (e.key === "Escape") {
    e.preventDefault();
    answerClose(false);
  }
}

// ── tab management ──────────────────────────────────────────────────────────

function activateTab(id: number) {
  activeTabId.value = id;
  const tab = tabs.value.find((t) => t.id === id);
  if (!tab) return;
  // User is now looking at this tab — clear any done/review badge.
  markTabSeen(tab);
  const leaf = getFirstLeaf(tab.root);
  focusedLeafId.value = leaf.id;
  nextTick(() => xtermRefs.get(leaf.id)?.focus());
}

function addTab(initialCmd?: string, extra?: { cwd?: string; resultToken?: string }) {
  const leaf = makeLeaf(initialCmd, extra);
  const tab: Tab = { id: leaf.id, root: leaf };
  tabs.value.push(tab);
  activeTabId.value = tab.id;
  focusedLeafId.value = leaf.id;
  nextTick(() => xtermRefs.get(leaf.id)?.focus());
}

function spawnAgent(cmd: string) {
  addTab(cmd);
}

// Inject a file/folder path from the explorer into the focused leaf's PTY as an
// "@path " context reference (relative to the workspace cwd when possible) so the
// active agent picks it up. User reviews + hits Enter.
function insertContext(absPath: string) {
  const tab = tabs.value.find((t) => t.id === activeTabId.value);
  if (!tab) return;
  let rel = absPath;
  const base = props.cwd.replace(/\/+$/, "");
  if (base && absPath.startsWith(base + "/")) rel = absPath.slice(base.length + 1);
  const ref = `@${rel} `;
  const xterm = xtermRefs.get(focusedLeafId.value);
  xterm?.sendText(ref);
}

function openDiffInTab(file: string, staged: boolean, diff: string) {
  terminalCounter++;
  const leaf: Leaf = {
    type: "leaf",
    id: nextPtyId(),
    title: `Diff: ${file}`,
    defaultTitle: `Diff: ${file}`,
    isAgent: false,
    busy: false,
    status: "idle",
    leafType: "diff",
    diffFile: file,
    diffStaged: staged,
    diff,
  };
  const tab: Tab = { id: leaf.id, root: leaf };
  tabs.value.push(tab);
  activeTabId.value = tab.id;
}

function splitFocused(direction: "h" | "v") {
  const tab = tabs.value.find((t) => t.id === activeTabId.value);
  if (!tab) return;
  const newLeaf = makeLeaf();
  tab.root = insertSplit(tab.root, focusedLeafId.value, direction, newLeaf);
  focusedLeafId.value = newLeaf.id;
  nextTick(() => xtermRefs.get(newLeaf.id)?.focus());
}

async function closeTab(tabId: number) {
  const tab = tabs.value.find((t) => t.id === tabId);
  if (!tab) return;

  const leaves = getAllLeaves(tab.root);
  const busyLeaf = leaves.find((l) => l.busy);
  if (busyLeaf) {
    const ok = await confirmClose(busyLeaf.title);
    if (!ok) return;
  }

  // Explicitly kill PTYs so the daemon drops them (not a detach — user closed the tab)
  for (const leaf of leaves) {
    invoke("kill_pty", { id: leaf.id }).catch(() => {});
  }

  const idx = tabs.value.findIndex((t) => t.id === tabId);
  tabs.value.splice(idx, 1);

  if (activeTabId.value === tabId && tabs.value.length > 0) {
    const newTab = tabs.value[Math.max(0, idx - 1)];
    activateTab(newTab.id);
  }
}

function isTabSplit(tab: Tab): boolean {
  return tab.root.type === 'split';
}

async function closePane(leafId: number) {
  const tab = tabs.value.find((t) => findLeaf(t.root, leafId));
  if (!tab) return;
  const leaves = getAllLeaves(tab.root);
  if (leaves.length === 1) {
    await closeTab(tab.id);
    return;
  }
  const leaf = findLeaf(tab.root, leafId);
  if (leaf?.busy) {
    const ok = await confirmClose(leaf.title);
    if (!ok) return;
  }
  invoke("kill_pty", { id: leafId }).catch(() => {});
  const newRoot = removeLeaf(tab.root, leafId)!;
  tab.root = newRoot;
  if (focusedLeafId.value === leafId) {
    const remaining = getAllLeaves(newRoot);
    focusedLeafId.value = remaining[0].id;
    nextTick(() => xtermRefs.get(remaining[0].id)?.focus());
  }
}

function focusActive() {
  xtermRefs.get(focusedLeafId.value)?.focus();
}

function onKeydown(e: KeyboardEvent) {
  if (wsStore.active?.id !== props.workspaceId) return;
  if (e.ctrlKey && !e.metaKey && !e.altKey && /^[1-9]$/.test(e.key)) {
    e.preventDefault();
    const idx = parseInt(e.key) - 1;
    const wsTabs = tabsStore.tabsByWs[props.workspaceId] ?? [];
    if (wsTabs[idx]) activateTab(wsTabs[idx].id);
    return;
  }
  if (!e.metaKey || e.ctrlKey || e.altKey) return;
  const k = e.key.toLowerCase();
  if (k === "t") {
    e.preventDefault();
    addTab();
  } else if (k === "w") {
    e.preventDefault();
    closePane(focusedLeafId.value);
  } else if (k === "d") {
    e.preventDefault();
    splitFocused(e.shiftKey ? "v" : "h");
  }
}

// ── persistence ─────────────────────────────────────────────────────────────

function allLeaves(): Leaf[] {
  return tabs.value.flatMap((t) => getAllLeaves(t.root));
}

function persist() {
  const payload: PersistedTab[] = allLeaves().map((l) => ({
    title: l.defaultTitle,
    initial_cmd: l.initialCmd ?? null,
    pty_id: l.id,
    cwd: l.cwd ?? null,
  }));
  invoke("save_terminal_tabs", { workspaceId: props.workspaceId, tabs: payload });
}

watch(
  () => allLeaves().map((l) => `${l.id}|${l.defaultTitle}`).join(","),
  persist,
);

// ── sidebar mirror ──────────────────────────────────────────────────────────
// Push tab summaries to the shared store so the Sidebar can list terminals
// nested under this workspace, and react to clicks coming back from it.

function syncStore() {
  tabsStore.setTabs(
    props.workspaceId,
    tabs.value.map((t) => ({
      id: t.id,
      title: tabTitle(t),
      isAgent: tabIsAgent(t),
      busy: getAllLeaves(t.root).some((l) => l.busy),
      status: tabStatus(t),
    })),
  );
  tabsStore.setActive(props.workspaceId, activeTabId.value);
}

watch([tabs, activeTabId, focusedLeafId], syncStore, { deep: true });

// When the user switches TO this workspace (with the window focused), treat its
// active tab as seen so a "review" badge earned while it was hidden clears.
watch(
  () => wsStore.active?.id,
  (id) => {
    if (id !== props.workspaceId || !document.hasFocus()) return;
    const tab = tabs.value.find((t) => t.id === activeTabId.value);
    if (tab) markTabSeen(tab);
  },
);

watch(
  () => tabsStore.request,
  (req) => {
    if (!req || req.wsId !== props.workspaceId) return;
    if (req.action === "activate" && req.tabId != null) activateTab(req.tabId);
    else if (req.action === "add") addTab();
    else if (req.action === "close" && req.tabId != null) closeTab(req.tabId);
    else if (req.action === "reorder" && req.fromIdx != null && req.toIdx != null) {
      const moved = tabs.value.splice(req.fromIdx, 1)[0];
      if (moved) tabs.value.splice(req.toIdx, 0, moved);
    }
  },
);

onMounted(async () => {
  window.addEventListener("keydown", onKeydown);

  const [saved, daemonSessions] = await Promise.all([
    invoke<PersistedTab[]>("list_terminal_tabs", { workspaceId: props.workspaceId }),
    invoke<DaemonSession[]>("list_pty_sessions").catch(() => [] as DaemonSession[]),
  ]);

  // Build set of alive PTY ids from daemon for quick lookup
  const alivePtys = new Set(daemonSessions.filter((s) => s.alive).map((s) => s.pty_id));

  if (saved.length) {
    // Advance counter past max saved id so new tabs don't collide
    const maxSavedId = Math.max(...saved.map((s) => s.pty_id ?? 0));
    initPtyCounter(maxSavedId);

    saved.forEach((s) => {
      // Use saved pty_id when the session is alive in daemon, otherwise get fresh id
      const useSavedId = s.pty_id != null && alivePtys.has(s.pty_id);
      const leaf = makeLeaf(undefined, {
        cwd: s.cwd ?? undefined,
        id: useSavedId ? s.pty_id! : undefined,
      });
      leaf.defaultTitle = s.title || leaf.defaultTitle;
      leaf.title = leaf.defaultTitle;
      const tab: Tab = { id: leaf.id, root: leaf };
      tabs.value.push(tab);
    });
    activateTab(tabs.value[0].id);
  }
  syncStore();
});

// Poll for `burrow spawn` requests routed to this workspace (file-based, since
// agents' Bash/hooks have no controlling tty for the OSC channel).
let spawnPoll: ReturnType<typeof setInterval> | undefined;
onMounted(() => {
  spawnPoll = setInterval(async () => {
    try {
      const reqs = await invoke<{ cmd: string; token: string; cwd: string }[]>(
        "take_spawn_requests",
        { cwd: props.cwd },
      );
      for (const r of reqs) {
        addTab(r.cmd, { cwd: r.cwd || undefined, resultToken: r.token || undefined });
      }
    } catch { /* ignore poll errors */ }
  }, 1000);
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown);
  if (spawnPoll) clearInterval(spawnPoll);
  tabsStore.clear(props.workspaceId);
});

function popOutTab(tab: Tab) {
  const leaf =
    focusedLeafId.value !== 0
      ? findLeaf(tab.root, focusedLeafId.value) ?? getFirstLeaf(tab.root)
      : getFirstLeaf(tab.root);
  invoke("open_float_window", {
    ptyId: leaf.id,
    title: leaf.title,
    wsId: props.workspaceId,
  });
}

// Activate the tab containing ptyId and focus that leaf. Called by App.vue
// when main window regains focus from a float bubble.
function focusLeaf(ptyId: number) {
  for (const tab of tabs.value) {
    const leaf = findLeaf(tab.root, ptyId);
    if (leaf) {
      activeTabId.value = tab.id;
      nextTick(() => {
        focusedLeafId.value = ptyId;
        xtermRefs.get(ptyId)?.focus();
      });
      return;
    }
  }
}

defineExpose({ addTab, spawnAgent, openDiffInTab, insertContext, focusLeaf });
</script>

<style scoped>
.terminal-pane {
  flex: 1;
  display: flex;
  flex-direction: column;
  background: var(--terminal-bg);
  backdrop-filter: var(--backdrop-blur, none);
  -webkit-backdrop-filter: var(--backdrop-blur, none);
  overflow: hidden;
  min-width: 0;
}

.terminal-tabs {
  display: flex;
  align-items: center;
  background: var(--bg-panel);
  border-bottom: 1px solid var(--border);
  padding: 0 4px;
  flex-shrink: 0;
  overflow-x: auto;
}

.tab {
  display: flex;
  align-items: center;
  gap: 6px;
  background: none;
  border: none;
  border-bottom: 2px solid transparent;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 11px;
  font-family: var(--font-ui);
  padding: 6px 10px;
  white-space: nowrap;
  transition: color 0.1s;
  max-width: 200px;
  flex-shrink: 0;
}
.tab:hover { color: var(--text-primary); }
.tab.active { color: var(--text-primary); border-bottom-color: var(--accent); }
.tab-add { color: var(--text-muted); font-size: 14px; max-width: none; }
.tab-add:hover { color: var(--text-secondary); }

.tab-label {
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 140px;
}

.tab-agent-icon { color: var(--accent); flex-shrink: 0; }
.tab-term-icon  { color: var(--text-muted); flex-shrink: 0; }

.status-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
  position: relative;
}
.status-dot.status-running {
  background: transparent;
  border-radius: 0;
  width: auto;
  height: auto;
  color: #fb923c;
  font-size: 14px;
  line-height: 1;
  font-family: monospace;
  font-weight: 700;
  text-shadow: 0 0 6px rgba(249, 115, 22, 0.9), 0 0 12px rgba(249, 115, 22, 0.5);
  animation: running-glow 1.4s ease-in-out infinite;
}
@keyframes running-glow {
  0%, 100% { opacity: 0.7; text-shadow: 0 0 4px rgba(249, 115, 22, 0.7); }
  50%      { opacity: 1;   text-shadow: 0 0 8px rgba(249, 115, 22, 1), 0 0 14px rgba(249, 115, 22, 0.6); }
}
.status-dot.status-waiting { background: #3b82f6; }
.status-dot.status-done    { background: #84cc16; }
/* review = agent finished while you weren't watching; persists until seen */
.status-dot.status-review {
  background: #22c55e;
  box-shadow: 0 0 0 0 rgba(34, 197, 94, 0.6);
  animation: review-pulse 1.8s ease-out infinite;
}
@keyframes review-pulse {
  0%   { box-shadow: 0 0 0 0 rgba(34, 197, 94, 0.6); }
  70%  { box-shadow: 0 0 0 5px rgba(34, 197, 94, 0); }
  100% { box-shadow: 0 0 0 0 rgba(34, 197, 94, 0); }
}

.tab-close,
.tab-float {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  border-radius: 3px;
  flex-shrink: 0;
  cursor: pointer;
  opacity: 0;
  transition: opacity 0.1s, background 0.1s;
}
.tab:hover .tab-close,
.tab:hover .tab-float { opacity: 0.45; }
.tab-close:hover { opacity: 1 !important; background: rgba(239,68,68,0.2); color: var(--red); }
.tab-float:hover { opacity: 1 !important; background: rgba(255,255,255,0.1); }

.terminal-body {
  flex: 1;
  display: flex;
  overflow: hidden;
  position: relative;
}

.terminal-tab-content {
  flex: 1;
  position: relative;
  overflow: hidden;
  min-width: 0;
  min-height: 0;
  background: var(--border);
}

.pane {
  position: absolute;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: var(--terminal-bg);
}

.pane.focused::after {
  content: "";
  position: absolute;
  inset: 0;
  pointer-events: none;
  border: 1px solid var(--accent);
  opacity: 0.35;
  z-index: 1;
}

.confirm-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.6);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 9000;
}

.confirm-modal {
  width: 360px;
  background: #111111;
  border: 1px solid #2a2a2a;
  border-radius: 12px;
  padding: 20px;
  box-shadow: 0 24px 64px rgba(0, 0, 0, 0.6), 0 1px 0 rgba(255, 255, 255, 0.08);
}

.confirm-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 8px;
}

.confirm-body {
  font-size: 12px;
  color: var(--text-secondary);
  line-height: 1.5;
  margin-bottom: 18px;
}

.confirm-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}

.confirm-btn {
  font-family: var(--font-ui);
  font-size: 12px;
  padding: 6px 14px;
  border-radius: 6px;
  border: 1px solid var(--border);
  background: var(--bg-hover);
  color: var(--text-primary);
  cursor: pointer;
}
.confirm-btn:hover { background: #222222; }

.confirm-btn.danger {
  background: rgba(239, 68, 68, 0.15);
  border-color: rgba(239, 68, 68, 0.4);
  color: #f87171;
}
.confirm-btn.danger:hover { background: rgba(239, 68, 68, 0.25); }
.confirm-kbd {
  margin-left: 6px;
  opacity: 0.6;
  font-size: 11px;
}

.terminal-welcome {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  color: var(--text-secondary);
  background: var(--terminal-bg);
}
.welcome-icon { color: var(--text-muted); }
.welcome-title { font-size: 14px; font-weight: 500; color: var(--text-primary); }
.welcome-sub { font-size: 12px; color: var(--text-muted); }
.welcome-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 8px;
  padding: 7px 16px;
  background: var(--accent);
  border: none;
  border-radius: 5px;
  color: #fff;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
}
.welcome-btn:hover { background: var(--accent-dim); }

.pane-titlebar {
  display: flex;
  align-items: center;
  gap: 5px;
  height: 26px;
  padding: 0 8px;
  background: #111111;
  border-bottom: 1px solid #1e1e1e;
  flex-shrink: 0;
  font-size: 11px;
  color: var(--text-secondary);
}
.pane-title-icon { color: var(--text-muted); flex-shrink: 0; }
.pane-title-icon.agent { color: var(--accent); }
.pane-title-text {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 11px;
}
.pane-title-close {
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  padding: 2px;
  border-radius: 3px;
  flex-shrink: 0;
  opacity: 0.4;
}
.pane-titlebar:hover .pane-title-close { opacity: 0.8; }
.pane-title-close:hover { opacity: 1 !important; color: var(--red); background: rgba(239,68,68,0.15); }
</style>
