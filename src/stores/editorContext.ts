import { ref } from "vue";
import { defineStore } from "pinia";

// Lightweight bridge so the in-app Claude chat can pull the active editor's
// file + current selection (VS Code "share selection" parity). CodeEditor.vue
// publishes here on focus / selection change; ClaudeChat.vue reads it.
export interface EditorSelection {
  path: string;
  startLine: number; // 1-based, inclusive
  endLine: number;
  text: string;
}

export const useEditorContextStore = defineStore("editorContext", () => {
  const activePath = ref<string>("");
  const selection = ref<EditorSelection | null>(null);

  function setActivePath(path: string) {
    activePath.value = path;
  }
  // text empty / zero-length range → clear (caret only, nothing selected).
  function setSelection(sel: EditorSelection | null) {
    selection.value = sel && sel.text.trim() ? sel : null;
  }
  function clear() {
    selection.value = null;
  }

  return { activePath, selection, setActivePath, setSelection, clear };
});
