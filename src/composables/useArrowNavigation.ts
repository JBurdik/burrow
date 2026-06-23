import { ref, type Ref } from 'vue'

/**
 * Arrow-key + Enter navigation for any list.
 *
 * Usage in Spotlight.vue:
 *
 *   // 1. Flatten sections into one array for index math
 *   const flatItems = computed(() => sections.value.flatMap(s => s.items))
 *
 *   // 2. Wire up the composable
 *   const { currentIndex, onKeydown, onMouseover } = useArrowNavigation(flatItems, runItem)
 *
 *   // 3. Replace selectedId with flatItems[currentIndex] in the template:
 *   //    :class="{ selected: flatItems[currentIndex]?.id === item.id }"
 *   //    @mousemove="(e) => onMouseover(e, flatItemIndex)"
 *   //    (compute flatItemIndex as sections[si].startOffset + itemIndex)
 *
 *   // 4. On the <input>: @keydown="onKeydown" (remove the individual @keydown.up/down/enter)
 *
 *   // 5. Put data-nav-index="<flatIndex>" on each .s-row so scrollIntoView finds it.
 */

export function useArrowNavigation<T>(
  items: Ref<T[]>,
  onSelect: (item: T) => void,
  initialIndex = -1,
) {
  const currentIndex = ref(initialIndex)
  // ponytail: plain object, not ref — we only need dedup, not reactivity
  let lastCursorPos = { x: 0, y: 0 }

  function scrollTo(i: number) {
    document.querySelector(`[data-nav-index="${i}"]`)?.scrollIntoView({
      behavior: 'smooth',
      block: 'nearest',
    })
  }

  function onKeydown(e: KeyboardEvent) {
    const len = items.value.length
    if (!len) return

    if (e.key === 'ArrowDown') {
      e.preventDefault()
      currentIndex.value = currentIndex.value < len - 1 ? currentIndex.value + 1 : 0
      scrollTo(currentIndex.value)
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      currentIndex.value = currentIndex.value > 0 ? currentIndex.value - 1 : len - 1
      scrollTo(currentIndex.value)
    } else if (e.key === 'Enter') {
      const item = items.value[currentIndex.value]
      if (item != null) onSelect(item)
    }
  }

  // Dedup: ignore mouseover fired by scroll repositioning the cursor over a new row
  // (clientX/Y stays identical when the element moves under a stationary mouse)
  function onMouseover(e: MouseEvent, index: number) {
    if (e.clientX === lastCursorPos.x && e.clientY === lastCursorPos.y) return
    lastCursorPos = { x: e.clientX, y: e.clientY }
    currentIndex.value = index
  }

  return { currentIndex, onKeydown, onMouseover }
}
