import { ref } from "vue";

export interface SplitZone {
  leafId: number;
  dir: "h" | "v";
  side: "first" | "second";
}

export function useDragSplit(opts: {
  onSplit: (fromTabIdx: number, zone: SplitZone) => void;
}) {
  const active = ref(false);
  const fromTabIdx = ref<number | null>(null);
  const hoveredZone = ref<SplitZone | null>(null);
  let ghost: HTMLElement | null = null;

  function hitTestZone(x: number, y: number): SplitZone | null {
    const panes = document.querySelectorAll<HTMLElement>(".pane[data-leaf-id]");
    for (const pane of panes) {
      const r = pane.getBoundingClientRect();
      if (r.width === 0 || r.height === 0) continue;
      if (x < r.left || x > r.right || y < r.top || y > r.bottom) continue;
      const leafId = Number(pane.dataset.leafId);
      if (!leafId) continue;
      const relX = x - r.left;
      const relY = y - r.top;
      const zoneW = r.width * 0.25;
      const zoneH = r.height * 0.25;
      if (relX < zoneW)               return { leafId, dir: "h", side: "first" };
      if (relX > r.width - zoneW)     return { leafId, dir: "h", side: "second" };
      if (relY < zoneH)               return { leafId, dir: "v", side: "first" };
      if (relY > r.height - zoneH)    return { leafId, dir: "v", side: "second" };
      return null;
    }
    return null;
  }

  function onMove(e: PointerEvent) {
    if (ghost) ghost.style.transform = `translate(${e.clientX + 14}px, ${e.clientY + 14}px)`;
    hoveredZone.value = hitTestZone(e.clientX, e.clientY);
  }

  function onUp(_e: PointerEvent) {
    const zone = hoveredZone.value;
    const idx = fromTabIdx.value;
    deactivate();
    if (zone !== null && idx !== null) opts.onSplit(idx, zone);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") deactivate();
  }

  function activate(tabIdx: number, e: PointerEvent, title: string) {
    fromTabIdx.value = tabIdx;
    active.value = true;
    hoveredZone.value = null;

    const g = document.createElement("div");
    g.textContent = title;
    Object.assign(g.style, {
      position: "fixed",
      left: "0",
      top: "0",
      pointerEvents: "none",
      zIndex: "9999",
      background: "var(--bg-panel, #1a1a1a)",
      color: "var(--text, #e0e0e0)",
      border: "1px solid var(--accent, #3b82f6)",
      borderRadius: "6px",
      padding: "4px 10px",
      fontSize: "12px",
      fontFamily: "inherit",
      whiteSpace: "nowrap",
      boxShadow: "0 4px 16px rgba(0,0,0,0.5)",
      transform: `translate(${e.clientX + 14}px, ${e.clientY + 14}px)`,
    } as Partial<CSSStyleDeclaration>);
    document.body.appendChild(g);
    ghost = g;

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("keydown", onKey);
  }

  function deactivate() {
    active.value = false;
    fromTabIdx.value = null;
    hoveredZone.value = null;
    ghost?.remove();
    ghost = null;
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    window.removeEventListener("keydown", onKey);
  }

  return { active, hoveredZone, activate, deactivate };
}
