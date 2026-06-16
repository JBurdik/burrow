/**
 * terminalStatus.ts
 *
 * Single source of truth for terminal status: the TermStatus type, the priority
 * ordering, the aggregation function used by Terminal.vue + Sidebar.vue, and the
 * hook→state reducer that replaces the duplicated inline handlers.
 *
 * Pure lib — no Vue/Tauri imports. All side effects (timers, sound, notify) are
 * wired through ReducerCtx by the caller, keeping this unit-testable.
 */

// ── Types ─────────────────────────────────────────────────────────────────────

export type TermStatus = "idle" | "running" | "waiting" | "permission" | "done" | "review" | "error";

/** Priority high→low. Single definition consumed by Terminal.tabStatus, Sidebar.aggStatus,
 *  and FloatBubble — no more separate hard-coded priority lists.
 *  `error` is the MOST urgent: a turn that failed (StopFailure: rate_limit, overloaded,
 *  auth, billing…) outranks even a permission prompt — the user must see it first. */
export const STATUS_PRIORITY: readonly TermStatus[] = [
  "error",
  "permission",
  "waiting",
  "running",
  "review",
  "done",
  "idle",
] as const;

/** Semantic agent hook event forwarded from XTerm.vue → Terminal.vue → here. */
export type AgentEvent = "running" | "waiting" | "permission" | "done" | "error";

/** Minimal leaf view the reducer needs. The full Leaf type from TerminalSplitView.vue
 *  satisfies this — no cast needed. */
export interface StatusLeaf {
  id: number;
  status: TermStatus;
  busy: boolean;
  isAgent: boolean;
}

/** Side-effect callbacks supplied by Terminal.vue/FloatBubble.vue. */
export interface ReducerCtx {
  /** True when the user is actively looking at this tab (ws active, tab active, window focused). */
  watching: boolean;
  /** Schedule the 4-second done→idle timer. Implementation manages doneTimers Map. */
  setDoneTimer(id: number): void;
  /** Cancel a pending done→idle timer. */
  clearDoneTimer(id: number): void;
  playSound(kind: "waiting" | "done"): void;
  /** Called when a turn is truly settled (done or review): fires notifyDone + git refresh. */
  onSettled(leaf: StatusLeaf): void;
}

// ── Aggregation ───────────────────────────────────────────────────────────────

/**
 * Reduce a collection to the highest-priority status. Works for both leaves of
 * a tab (Terminal.tabStatus) and tab summaries of a workspace (Sidebar.aggStatus).
 */
export function aggregateStatus<T>(
  items: T[],
  pick: (i: T) => TermStatus,
): TermStatus {
  for (const s of STATUS_PRIORITY) {
    if (items.some((i) => pick(i) === s)) return s;
  }
  return "idle";
}

// ── Reducer ───────────────────────────────────────────────────────────────────

/**
 * Apply an agent hook event (running | waiting | done) to a leaf.
 * Replaces onAgentState + settleDone in Terminal.vue.
 */
/** A turn is "in flight" only in these states — the window during which a
 *  `waiting`/`permission` hook is real. done/review/idle = the turn is over. */
function isTurnActive(s: TermStatus): boolean {
  return s === "running" || s === "waiting" || s === "permission";
}

export function applyAgentEvent(
  leaf: StatusLeaf,
  ev: AgentEvent,
  ctx: ReducerCtx,
): void {
  if (ev === "running") {
    ctx.clearDoneTimer(leaf.id);
    leaf.busy = true;
    leaf.status = "running";
  } else if (ev === "waiting") {
    // A `waiting`/`permission` is only meaningful while a turn is in flight. After
    // Stop settles a leaf (done/review) or before any turn starts (idle), Claude's
    // delayed "waiting for your input" Notification (or any stray waiting) must NOT
    // drag the leaf back out of done. A genuine new turn always arrives as `running`
    // first, so guarding on an active status can't suppress a real transition.
    if (!isTurnActive(leaf.status)) return;
    const enteringWait = leaf.status !== "waiting";
    leaf.busy = true;
    leaf.status = "waiting";
    if (enteringWait) ctx.playSound("waiting");
  } else if (ev === "permission") {
    if (!isTurnActive(leaf.status)) return;
    const entering = leaf.status !== "permission";
    leaf.busy = true;
    leaf.status = "permission";
    if (entering) ctx.playSound("waiting");
  } else if (ev === "done") {
    _settle(leaf, ctx);
  } else if (ev === "error") {
    _settleError(leaf, ctx);
  }
}

/**
 * Apply a foreground-process busy change from the poll.
 * No-op for agent leaves — hooks are the sole authority for agents.
 * Replaces onLeafBusy's status transitions in Terminal.vue.
 */
export function applyBusy(
  leaf: StatusLeaf,
  busy: boolean,
  wasBusy: boolean,
  ctx: ReducerCtx,
): void {
  // Agent session → poll must not fabricate status. Hooks own it.
  if (leaf.isAgent) return;
  leaf.busy = busy;
  if (busy) {
    ctx.clearDoneTimer(leaf.id);
    if (leaf.status !== "waiting") leaf.status = "running";
  } else if (wasBusy) {
    _settle(leaf, ctx);
  }
}

/**
 * Apply a needs-input signal from the output buffer heuristic (poll path, plain cmds).
 */
export function applyNeedsInput(
  leaf: StatusLeaf,
  needs: boolean,
  ctx: ReducerCtx,
): void {
  if (!leaf.busy) return;
  const enteringWait = needs && leaf.status !== "waiting";
  leaf.status = needs ? "waiting" : "running";
  if (enteringWait) ctx.playSound("waiting");
}

/**
 * Handle ESC / Ctrl+C interrupt — settle straight to idle (turn cancelled, not done).
 * No sound, no review badge.
 */
export function applyInterrupt(leaf: StatusLeaf, ctx: ReducerCtx): void {
  if (leaf.status !== "running" && leaf.status !== "waiting" && leaf.status !== "permission") return;
  ctx.clearDoneTimer(leaf.id);
  leaf.busy = false;
  leaf.status = "idle";
}

/**
 * Mark a done/review/error leaf as seen (user opened/returned to the tab) → idle.
 * `error` persists exactly like `review` — it must NOT silently clear while the
 * user is away; only seeing the tab dismisses it.
 */
export function markSeen(leaf: StatusLeaf, ctx: ReducerCtx): void {
  if (leaf.status !== "done" && leaf.status !== "review" && leaf.status !== "error") return;
  ctx.clearDoneTimer(leaf.id);
  leaf.busy = false;
  leaf.status = "idle";
}

/** Internal: settle a finished turn. Dedup guard: if nothing is active (busy=false),
 *  any late `done` event is a no-op — Stop already fired, timer may have reset status
 *  to idle, but the turn is definitively over. */
function _settle(leaf: StatusLeaf, ctx: ReducerCtx): void {
  if (!leaf.busy) return;
  leaf.busy = false;
  ctx.clearDoneTimer(leaf.id);
  if (ctx.watching) {
    leaf.status = "done";
    ctx.setDoneTimer(leaf.id);
  } else {
    leaf.status = "review";
    ctx.playSound("done");
  }
  ctx.onSettled(leaf);
}

/** Internal: settle a FAILED turn (Claude StopFailure: rate_limit, overloaded,
 *  authentication_failed, billing_error, server_error…). Unlike `done`, an error
 *  is urgent and ALWAYS persists (never the 4s auto-clear, regardless of whether
 *  the user is watching) — only markSeen dismisses it. No onSettled: this is NOT a
 *  "task complete" — firing the done notification/git-refresh would mislead. */
function _settleError(leaf: StatusLeaf, ctx: ReducerCtx): void {
  ctx.clearDoneTimer(leaf.id);
  leaf.busy = false;
  leaf.status = "error";
  ctx.playSound("done");
}

// ── Name derivation ───────────────────────────────────────────────────────────

/** True when the title is a generic auto-generated default (e.g. "Terminal 3"). */
export function isDefaultTitle(t: string): boolean {
  return /^Terminal \d+$/.test(t);
}

/**
 * Derive a consistent display name for a tab, regardless of which leaf is focused
 * or whether the tab is active/inactive.
 *
 * Priority:
 * 1. Focused leaf's title, if meaningful (non-default)
 * 2. First leaf with a meaningful title
 * 3. Focused leaf's title (even if default)
 * 4. First leaf's title
 *
 * This fixes the inconsistency where active tabs used focusedLeafId and inactive
 * tabs used getFirstLeaf, causing a split tab to show different names based on
 * which pane the user last clicked.
 */
export function deriveTabTitle(
  leaves: { title: string }[],
  focused?: { title: string },
): string {
  if (focused && !isDefaultTitle(focused.title)) return focused.title;
  const meaningful = leaves.find((l) => !isDefaultTitle(l.title));
  if (meaningful) return meaningful.title;
  return (focused ?? leaves[0])?.title ?? "";
}
