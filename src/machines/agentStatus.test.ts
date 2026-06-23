/**
 * Unit tests for agentStatusMachine.
 * Uses XState's createActor directly — no Vue, no DOM.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createActor } from "xstate";
import { agentStatusMachine } from "./agentStatus";

function actor() {
  const a = createActor(agentStatusMachine);
  a.start();
  return a;
}

describe("agentStatusMachine", () => {
  describe("basic transitions", () => {
    it("starts in idle", () => {
      const a = actor();
      expect(a.getSnapshot().value).toBe("idle");
    });

    it("idle → START → running", () => {
      const a = actor();
      a.send({ type: "START" });
      expect(a.getSnapshot().value).toBe("running");
    });

    it("running → WAIT → waiting", () => {
      const a = actor();
      a.send({ type: "START" });
      a.send({ type: "WAIT" });
      expect(a.getSnapshot().value).toBe("waiting");
    });

    it("running → PERMISSION_REQUEST → permission", () => {
      const a = actor();
      a.send({ type: "START" });
      a.send({ type: "PERMISSION_REQUEST" });
      expect(a.getSnapshot().value).toBe("permission");
    });

    it("waiting → RESUME → running", () => {
      const a = actor();
      a.send({ type: "START" });
      a.send({ type: "WAIT" });
      a.send({ type: "RESUME" });
      expect(a.getSnapshot().value).toBe("running");
    });

    it("permission → RESUME → running", () => {
      const a = actor();
      a.send({ type: "START" });
      a.send({ type: "PERMISSION_REQUEST" });
      a.send({ type: "RESUME" });
      expect(a.getSnapshot().value).toBe("running");
    });
  });

  describe("STOP guard: isWatching", () => {
    it("STOP watching=true → done", () => {
      const a = actor();
      a.send({ type: "START" });
      a.send({ type: "STOP", watching: true });
      expect(a.getSnapshot().value).toBe("done");
    });

    it("STOP watching=false → review", () => {
      const a = actor();
      a.send({ type: "START" });
      a.send({ type: "STOP", watching: false });
      expect(a.getSnapshot().value).toBe("review");
    });

    it("STOP from waiting watching=true → done", () => {
      const a = actor();
      a.send({ type: "START" });
      a.send({ type: "WAIT" });
      a.send({ type: "STOP", watching: true });
      expect(a.getSnapshot().value).toBe("done");
    });

    it("STOP from permission watching=false → review", () => {
      const a = actor();
      a.send({ type: "START" });
      a.send({ type: "PERMISSION_REQUEST" });
      a.send({ type: "STOP", watching: false });
      expect(a.getSnapshot().value).toBe("review");
    });
  });

  describe("done: 4s auto-clear", () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });
    afterEach(() => {
      vi.useRealTimers();
    });

    it("done → idle after 4 s", () => {
      const a = actor();
      a.send({ type: "START" });
      a.send({ type: "STOP", watching: true });
      expect(a.getSnapshot().value).toBe("done");
      vi.advanceTimersByTime(4000);
      expect(a.getSnapshot().value).toBe("idle");
    });

    it("done → MARK_SEEN → idle (before timer)", () => {
      const a = actor();
      a.send({ type: "START" });
      a.send({ type: "STOP", watching: true });
      a.send({ type: "MARK_SEEN" });
      expect(a.getSnapshot().value).toBe("idle");
    });
  });

  describe("review: persists until MARK_SEEN", () => {
    beforeEach(() => vi.useFakeTimers());
    afterEach(() => vi.useRealTimers());

    it("review stays after 10 s", () => {
      const a = actor();
      a.send({ type: "START" });
      a.send({ type: "STOP", watching: false });
      vi.advanceTimersByTime(10_000);
      expect(a.getSnapshot().value).toBe("review");
    });

    it("review → MARK_SEEN → idle", () => {
      const a = actor();
      a.send({ type: "START" });
      a.send({ type: "STOP", watching: false });
      a.send({ type: "MARK_SEEN" });
      expect(a.getSnapshot().value).toBe("idle");
    });
  });

  describe("error", () => {
    it("running → FAIL → error with detail", () => {
      const a = actor();
      a.send({ type: "START" });
      a.send({ type: "FAIL", detail: "rate_limit" });
      expect(a.getSnapshot().value).toBe("error");
      expect(a.getSnapshot().context.detail).toBe("rate_limit");
    });

    it("error → RETRY → running, detail cleared", () => {
      const a = actor();
      a.send({ type: "START" });
      a.send({ type: "FAIL", detail: "overloaded" });
      a.send({ type: "RETRY" });
      expect(a.getSnapshot().value).toBe("running");
      expect(a.getSnapshot().context.detail).toBeUndefined();
    });

    it("error → MARK_SEEN → idle", () => {
      const a = actor();
      a.send({ type: "START" });
      a.send({ type: "FAIL" });
      a.send({ type: "MARK_SEEN" });
      expect(a.getSnapshot().value).toBe("idle");
    });
  });

  describe("INTERRUPT", () => {
    it("running → INTERRUPT → idle", () => {
      const a = actor();
      a.send({ type: "START" });
      a.send({ type: "INTERRUPT" });
      expect(a.getSnapshot().value).toBe("idle");
    });

    it("waiting → INTERRUPT → idle", () => {
      const a = actor();
      a.send({ type: "START" });
      a.send({ type: "WAIT" });
      a.send({ type: "INTERRUPT" });
      expect(a.getSnapshot().value).toBe("idle");
    });

    it("permission → INTERRUPT → idle", () => {
      const a = actor();
      a.send({ type: "START" });
      a.send({ type: "PERMISSION_REQUEST" });
      a.send({ type: "INTERRUPT" });
      expect(a.getSnapshot().value).toBe("idle");
    });
  });

  describe("new turn from terminal states", () => {
    it("review → START → running", () => {
      const a = actor();
      a.send({ type: "START" });
      a.send({ type: "STOP", watching: false });
      a.send({ type: "START" });
      expect(a.getSnapshot().value).toBe("running");
    });

    it("error → START → running, detail cleared", () => {
      const a = actor();
      a.send({ type: "START" });
      a.send({ type: "FAIL", detail: "billing_error" });
      a.send({ type: "START" });
      expect(a.getSnapshot().value).toBe("running");
      expect(a.getSnapshot().context.detail).toBeUndefined();
    });
  });
});
