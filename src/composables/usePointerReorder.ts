import { ref } from "vue";

// Pointer-based list reordering with a floating drag ghost.
//
// Why not HTML5 drag-and-drop? Tauri's WKWebView keeps its native drag-drop
// handler ON (we rely on it for dropping image files into the terminal — see
// XTerm.vue's onDragDropEvent). That native handler swallows the webview's own
// HTML5 `drop` events, so `draggable` + @dragstart/@drop silently never
// completes a reorder. Pointer events are immune to it.
//
// Items must carry `data-reorder-idx` (their index). When several independent
// lists share a page (e.g. one tab list per workspace in the Sidebar), give each
// item a `data-reorder-group` so a drag can only drop within its own group.
//
// Behaviour that makes it feel native:
//  - text selection is killed for the whole gesture (set on pointerdown)
//  - a small movement threshold must be crossed before a drag starts, so a plain
//    click never reorders or shows a ghost
//  - the pointer is captured, so move/up keep firing even over iframes/xterm
//  - a cloned "ghost" of the row follows the cursor; the source row dims
//    (`dragIdx`) and the hovered slot highlights (`overIdx`)
const THRESHOLD = 5; // px the pointer must travel before a drag begins

export function usePointerReorder(
  commit: (from: number, to: number, group: string | null) => void,
) {
  const dragIdx = ref<number | null>(null);
  const overIdx = ref<number | null>(null);
  const dragGroup = ref<string | null>(null);

  // Gesture-local state (not reactive — never rendered).
  let pendingIdx: number | null = null; // armed in down(), promoted to dragIdx once the threshold is crossed
  let pendingGroup: string | null = null;
  let srcEl: HTMLElement | null = null;
  let ghost: HTMLElement | null = null;
  let startX = 0, startY = 0; // pointerdown origin (for the threshold)
  let offX = 0, offY = 0;     // pointer offset inside the grabbed row (keeps the ghost under the cursor)
  let active = false;         // true once the drag has actually started
  let pointerId = -1;

  function targetAt(x: number, y: number): number | null {
    // The ghost is pointer-events:none, so elementFromPoint sees through it.
    const el = (document.elementFromPoint(x, y) as HTMLElement | null)?.closest(
      "[data-reorder-idx]",
    ) as HTMLElement | null;
    if (!el) return null;
    if (dragGroup.value != null && el.dataset.reorderGroup !== dragGroup.value) return null;
    const i = Number(el.dataset.reorderIdx);
    return Number.isNaN(i) ? null : i;
  }

  // Promote the armed press into a live drag: dim the source, build the ghost.
  function begin(clientX: number, clientY: number) {
    active = true;
    dragIdx.value = pendingIdx;
    dragGroup.value = pendingGroup;
    if (!srcEl) return;

    const r = srcEl.getBoundingClientRect();
    offX = clientX - r.left;
    offY = clientY - r.top;

    const g = srcEl.cloneNode(true) as HTMLElement;
    g.classList.add("reorder-ghost");
    Object.assign(g.style, {
      position: "fixed",
      left: "0px",
      top: "0px",
      width: `${r.width}px`,
      height: `${r.height}px`,
      margin: "0",
      pointerEvents: "none",
      zIndex: "9999",
      opacity: "0.9",
      transform: `translate(${r.left}px, ${r.top}px)`,
      transition: "none",
      boxShadow: "0 10px 28px rgba(0, 0, 0, 0.5)",
      borderRadius: "8px",
      background: "var(--bg-panel, #1a1a1a)",
      cursor: "grabbing",
    } as Partial<CSSStyleDeclaration>);
    document.body.appendChild(g);
    ghost = g;
  }

  function move(e: PointerEvent) {
    if (pendingIdx == null) return;
    if (!active) {
      if (Math.hypot(e.clientX - startX, e.clientY - startY) < THRESHOLD) return;
      begin(e.clientX, e.clientY);
    }
    e.preventDefault();
    if (ghost) ghost.style.transform = `translate(${e.clientX - offX}px, ${e.clientY - offY}px)`;
    overIdx.value = targetAt(e.clientX, e.clientY);
  }

  function finish(e: PointerEvent, cancelled: boolean) {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", onUp);
    window.removeEventListener("pointercancel", onCancel);
    try { srcEl?.releasePointerCapture?.(pointerId); } catch { /* already released */ }
    document.body.style.userSelect = "";
    document.body.style.cursor = "";
    ghost?.remove();
    ghost = null;

    const from = dragIdx.value;
    const to = active && !cancelled ? targetAt(e.clientX, e.clientY) : null;
    const group = dragGroup.value;

    pendingIdx = null;
    pendingGroup = null;
    srcEl = null;
    active = false;
    dragIdx.value = null;
    overIdx.value = null;
    dragGroup.value = null;

    if (from != null && to != null && from !== to) commit(from, to, group);
  }

  const onUp = (e: PointerEvent) => finish(e, false);
  const onCancel = (e: PointerEvent) => finish(e, true);

  // Attach to a row's @pointerdown. `group` scopes the drop (optional).
  function down(idx: number, e: PointerEvent, group: string | null = null) {
    if (e.button !== 0) return; // left button only
    pendingIdx = idx;
    pendingGroup = group;
    srcEl = e.currentTarget as HTMLElement;
    startX = e.clientX;
    startY = e.clientY;
    pointerId = e.pointerId;
    active = false;
    // Kill text selection for the whole gesture (restored in finish()).
    document.body.style.userSelect = "none";
    document.body.style.cursor = "grabbing";
    try { srcEl.setPointerCapture?.(e.pointerId); } catch { /* capture unsupported */ }
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onCancel);
  }

  return { dragIdx, overIdx, dragGroup, down };
}
