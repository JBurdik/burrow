<template>
  <Teleport to="body">
    <div class="diagram-backdrop" @click.self="close">
      <div class="diagram-panel">
        <button class="diagram-close" @click="close"><PhX :size="16" /></button>
        <div ref="containerRef" class="diagram-svg" />
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, onMounted, watch } from "vue";
import { PhX } from "@phosphor-icons/vue";
import mermaid from "mermaid";
import { useDiagram } from "@/composables/useDiagram";

const { diagramContent, closeDiagram } = useDiagram();
const containerRef = ref<HTMLElement | null>(null);

const isDark = window.matchMedia("(prefers-color-scheme: dark)").matches;

mermaid.initialize({
  startOnLoad: false,
  theme: isDark ? "dark" : "default",
});

async function render(content: string) {
  if (!containerRef.value) return;
  try {
    const id = `burrow-diagram-${Date.now()}`;
    const { svg } = await mermaid.render(id, content);
    containerRef.value.innerHTML = svg;
  } catch (e) {
    containerRef.value.innerHTML = `<pre style="color:red;padding:1em">${String(e)}</pre>`;
  }
}

onMounted(() => {
  if (diagramContent.value) render(diagramContent.value);
});

watch(diagramContent, (val) => {
  if (val) render(val);
});

function close() {
  closeDiagram();
}

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") close();
}
onMounted(() => window.addEventListener("keydown", onKey));
import { onBeforeUnmount } from "vue";
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<style scoped>
.diagram-backdrop {
  position: fixed;
  inset: 0;
  z-index: 1000;
  background: rgba(0, 0, 0, 0.6);
  display: flex;
  align-items: center;
  justify-content: center;
}
.diagram-panel {
  position: relative;
  background: var(--bg-surface, #1e1e2e);
  border-radius: 10px;
  padding: 2rem;
  max-width: 90vw;
  max-height: 85vh;
  overflow: auto;
  box-shadow: 0 24px 64px rgba(0, 0, 0, 0.5);
}
.diagram-close {
  position: absolute;
  top: 0.6rem;
  right: 0.6rem;
  background: none;
  border: none;
  cursor: pointer;
  color: var(--text-muted, #888);
  padding: 4px;
  display: flex;
  align-items: center;
  border-radius: 4px;
}
.diagram-close:hover {
  background: var(--bg-hover, #2a2a3e);
  color: var(--text, #cdd6f4);
}
.diagram-svg :deep(svg) {
  max-width: 80vw;
  max-height: 70vh;
  height: auto;
}
</style>
