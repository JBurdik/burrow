import { ref } from "vue";

// Pointer-based list reordering.
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
export function usePointerReorder(
  commit: (from: number, to: number, group: string | null) => void,
) {
  const dragIdx = ref<number | null>(null);
  const overIdx = ref<number | null>(null);
  const dragGroup = ref<string | null>(null);
  let moved = false;

  function targetAt(x: number, y: number): number | null {
    const el = (document.elementFromPoint(x, y) as HTMLElement | null)?.closest(
      "[data-reorder-idx]",
    ) as HTMLElement | null;
    if (!el) return null;
    if (dragGroup.value != null && el.dataset.reorderGroup !== dragGroup.value) return null;
    const i = Number(el.dataset.reorderIdx);
    return Number.isNaN(i) ? null : i;
  }

  function move(e: PointerEvent) {
    if (dragIdx.value == null) return;
    moved = true;
    overIdx.value = targetAt(e.clientX, e.clientY);
  }

  function up(e: PointerEvent) {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    document.body.style.userSelect = "";
    const from = dragIdx.value;
    const to = targetAt(e.clientX, e.clientY);
    const group = dragGroup.value;
    dragIdx.value = null;
    overIdx.value = null;
    dragGroup.value = null;
    if (moved && from != null && to != null && from !== to) commit(from, to, group);
  }

  // Attach to a drag handle's @pointerdown. `group` scopes the drop (optional).
  function down(idx: number, e: PointerEvent, group: string | null = null) {
    if (e.button !== 0) return; // left button only
    dragIdx.value = idx;
    dragGroup.value = group;
    moved = false;
    document.body.style.userSelect = "none";
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  return { dragIdx, overIdx, dragGroup, down };
}
