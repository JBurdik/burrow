import { defineStore } from "pinia";
import { reactive } from "vue";

export type TurnState = "running" | "waiting" | "permission" | "error";

export interface TurnSegment {
  state: TurnState;
  start: number;
  end?: number;
}

export interface AgentTurn {
  id: number;
  start: number;
  end?: number;
  segments: TurnSegment[];
}

const MAX_TURNS = 50;

interface PtyEntry {
  turns: AgentTurn[];
  nextId: number;
  pendingTurn?: AgentTurn;
  pendingSegment?: TurnSegment;
}

export const useAgentHistoryStore = defineStore("agentHistory", () => {
  // ponytail: reactive() so nested mutations are tracked without manual .value
  const entries = reactive<Record<number, PtyEntry>>({});

  function _get(ptyId: number): PtyEntry {
    if (!entries[ptyId]) entries[ptyId] = { turns: [], nextId: 0 };
    return entries[ptyId];
  }

  function _closePending(e: PtyEntry, ts: number) {
    if (!e.pendingTurn) return;
    if (e.pendingSegment) e.pendingSegment.end = ts;
    e.pendingTurn.end = ts;
    e.turns.push(e.pendingTurn);
    if (e.turns.length > MAX_TURNS) e.turns.shift();
    e.pendingTurn = undefined;
    e.pendingSegment = undefined;
  }

  function addEvent(ptyId: number, state: string, ts: number = Date.now()) {
    const e = _get(ptyId);

    if (state === "running") {
      _closePending(e, ts);
      const seg: TurnSegment = { state: "running", start: ts };
      e.pendingTurn = { id: e.nextId++, start: ts, segments: [seg] };
      e.pendingSegment = seg;
      return;
    }

    if (!e.pendingTurn) return;

    if (state === "waiting" || state === "permission") {
      if (e.pendingSegment) e.pendingSegment.end = ts;
      const seg: TurnSegment = { state: state as TurnState, start: ts };
      e.pendingTurn.segments.push(seg);
      e.pendingSegment = seg;
    } else if (state === "error") {
      if (e.pendingSegment) e.pendingSegment.end = ts;
      const seg: TurnSegment = { state: "error", start: ts };
      e.pendingTurn.segments.push(seg);
      e.pendingTurn.end = ts;
      e.turns.push(e.pendingTurn);
      if (e.turns.length > MAX_TURNS) e.turns.shift();
      e.pendingTurn = undefined;
      e.pendingSegment = undefined;
    } else if (state === "done") {
      if (e.pendingSegment) e.pendingSegment.end = ts;
      e.pendingTurn.end = ts;
      e.turns.push(e.pendingTurn);
      if (e.turns.length > MAX_TURNS) e.turns.shift();
      e.pendingTurn = undefined;
      e.pendingSegment = undefined;
    }
  }

  /** Returns completed turns + the in-progress turn (if any). Newest last. */
  function getTimeline(ptyId: number): AgentTurn[] {
    const e = entries[ptyId];
    if (!e) return [];
    return e.pendingTurn ? [...e.turns, e.pendingTurn] : [...e.turns];
  }

  function clear(ptyId: number) {
    delete entries[ptyId];
  }

  return { addEvent, getTimeline, clear };
});
