import { ref, reactive, onUnmounted, type Ref } from "vue";

type Breakpoints = Record<string, number>;

/**
 * Element-level responsive layout via ResizeObserver.
 *
 * Usage in RightPanel.vue:
 *
 *   <aside ref="panelEl" class="right-panel">
 *     <!-- under 200px: icon-only tab labels -->
 *     <span v-if="cq.is.sm">Git</span>
 *
 *     <!-- under 350px: hide git diff panel -->
 *     <div v-if="cq.is.md" class="diff-section">…</div>
 *
 *     <!-- over 500px: full history section -->
 *     <div v-if="cq.is.lg" class="history-section">…</div>
 *   </aside>
 *
 *   const panelEl = ref<HTMLElement | null>(null)
 *   const cq = useContainerQuery(panelEl, { sm: 200, md: 350, lg: 500 })
 *   // cq.width, cq.height, cq.is.sm (width >= 200), cq.is.md, cq.is.lg
 */
export function useContainerQuery<T extends Breakpoints>(
  el: Ref<HTMLElement | null | undefined>,
  breakpoints: T,
) {
  const width = ref(0);
  const height = ref(0);

  const is = reactive(
    Object.fromEntries(Object.keys(breakpoints).map((k) => [k, false])),
  ) as Record<keyof T, boolean>;

  const ro = new ResizeObserver(([entry]) => {
    const w = entry.contentRect.width;
    const h = entry.contentRect.height;
    width.value = w;
    height.value = h;
    for (const [k, min] of Object.entries(breakpoints)) {
      (is as Record<string, boolean>)[k] = w >= min;
    }
  });

  // watch can't be used here because el may be set after setup;
  // caller must pass a template ref — ResizeObserver.observe is called lazily
  // via a MutationObserver watching document until the element appears.
  let mo: MutationObserver | null = null;

  function tryObserve() {
    if (!el.value) return false;
    ro.observe(el.value);
    return true;
  }

  if (!tryObserve()) {
    mo = new MutationObserver(() => {
      if (tryObserve()) {
        mo!.disconnect();
        mo = null;
      }
    });
    mo.observe(document.body, { childList: true, subtree: true });
  }

  onUnmounted(() => {
    ro.disconnect();
    mo?.disconnect();
  });

  return { width, height, is };
}
