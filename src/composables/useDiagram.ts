import { ref } from "vue";

const diagramContent = ref<string | null>(null);

export function useDiagram() {
  function showDiagram(content: string) {
    diagramContent.value = content;
  }
  function closeDiagram() {
    diagramContent.value = null;
  }
  return { diagramContent, showDiagram, closeDiagram };
}
