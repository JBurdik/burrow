/**
 * XState v5 state machine for agent tab status.
 *
 * Mirrors the logic in src/lib/terminalStatus.ts (applyAgentEvent, _settle, etc.)
 * but expressed as an explicit state machine — useful for future visual tooling
 * and as a drop-in replacement for per-leaf status management.
 *
 * States: idle → running ⇄ waiting/permission → done/review/error
 *
 * Usage:
 *   const { snapshot, send } = useAgentStatus()
 *   send({ type: 'START' })
 *   send({ type: 'STOP', watching: isWatching() })
 */

import { setup, assign } from "xstate";
import { useMachine } from "@xstate/vue";
import type { ComputedRef } from "vue";
import { computed } from "vue";
import type { TermStatus } from "../lib/terminalStatus";

// ── Types ──────────────────────────────────────────────────────────────────────

export type AgentStatusEvent =
  | { type: "START" }
  | { type: "WAIT" }
  | { type: "RESUME" }
  | { type: "PERMISSION_REQUEST" }
  | { type: "STOP"; watching: boolean }
  | { type: "MARK_SEEN" }
  | { type: "FAIL"; detail?: string }
  | { type: "RETRY" }
  | { type: "INTERRUPT" };

interface AgentStatusContext {
  detail?: string;
}

// ── Machine ────────────────────────────────────────────────────────────────────

export const agentStatusMachine = setup({
  types: {
    context: {} as AgentStatusContext,
    events: {} as AgentStatusEvent,
  },
  actions: {
    clearError: assign({ detail: undefined }),
    setDetail: assign({
      detail: ({ event }: { event: AgentStatusEvent }) =>
        event.type === "FAIL" ? event.detail : undefined,
    }),
  },
  guards: {
    // `watching` is carried on the STOP event (evaluated at transition time).
    isWatching: ({ event }: { event: AgentStatusEvent }) => event.type === "STOP" && event.watching,
    notWatching: ({ event }: { event: AgentStatusEvent }) => event.type === "STOP" && !event.watching,
  },
}).createMachine({
  id: "agentStatus",
  initial: "idle",
  context: { detail: undefined },

  states: {
    idle: {
      on: {
        START: "running",
      },
    },

    running: {
      on: {
        WAIT: "waiting",
        PERMISSION_REQUEST: "permission",
        STOP: [
          { guard: "isWatching", target: "done" },
          { target: "review" },
        ],
        FAIL: { target: "error", actions: "setDetail" },
        INTERRUPT: "idle",
      },
    },

    waiting: {
      on: {
        RESUME: "running",
        START: "running",
        PERMISSION_REQUEST: "permission",
        STOP: [
          { guard: "isWatching", target: "done" },
          { target: "review" },
        ],
        FAIL: { target: "error", actions: "setDetail" },
        INTERRUPT: "idle",
      },
    },

    permission: {
      on: {
        RESUME: "running",
        START: "running",
        STOP: [
          { guard: "isWatching", target: "done" },
          { target: "review" },
        ],
        FAIL: { target: "error", actions: "setDetail" },
        INTERRUPT: "idle",
      },
    },

    // Transient — auto-clears after 4 s when the user is watching.
    done: {
      after: {
        4000: "idle",
      },
      on: {
        MARK_SEEN: "idle",
        START: { target: "running", actions: "clearError" },
      },
    },

    // Persists until the user opens the tab (markTabSeen).
    review: {
      on: {
        MARK_SEEN: "idle",
        START: { target: "running", actions: "clearError" },
      },
    },

    // Persists until MARK_SEEN — never auto-clears (a failed turn must be seen).
    error: {
      on: {
        RETRY: { target: "running", actions: "clearError" },
        MARK_SEEN: { target: "idle", actions: "clearError" },
        START: { target: "running", actions: "clearError" },
      },
    },
  },
});

// ── Vue composable ─────────────────────────────────────────────────────────────

export interface AgentStatusComposable {
  status: ComputedRef<TermStatus>;
  detail: ComputedRef<string | undefined>;
  send: (event: AgentStatusEvent) => void;
  /** Raw XState snapshot — for advanced consumers. */
  snapshot: ReturnType<typeof useMachine<typeof agentStatusMachine>>["snapshot"];
}

/**
 * Vue composable wrapping agentStatusMachine.
 *
 * @example
 * const { status, send } = useAgentStatus()
 * send({ type: 'START' })
 * watch(hasFocus, (f) => { if (!f && status.value === 'done') send({ type: 'STOP', watching: false }) })
 */
export function useAgentStatus(): AgentStatusComposable {
  const { snapshot, send } = useMachine(agentStatusMachine);

  const status = computed(() => snapshot.value.value as TermStatus);
  const detail = computed(() => snapshot.value.context.detail);

  return { status, detail, send, snapshot };
}
