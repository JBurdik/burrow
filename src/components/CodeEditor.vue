<template>
  <div class="code-editor">
    <div v-if="placeholder" class="editor-placeholder">
      <PhFileX :size="34" weight="thin" />
      <p>{{ placeholder }}</p>
    </div>
    <div v-else ref="host" class="editor-host" />
  </div>
</template>

<script setup lang="ts">
import { ref, watch, onMounted, onBeforeUnmount } from "vue";
import { PhFileX } from "@phosphor-icons/vue";
import { invoke } from "@tauri-apps/api/core";
import { EditorState, Compartment, Prec } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { indentWithTab } from "@codemirror/commands";
import { basicSetup } from "codemirror";
import { oneDark } from "@codemirror/theme-one-dark";
import { javascript } from "@codemirror/lang-javascript";
import { rust } from "@codemirror/lang-rust";
import { json } from "@codemirror/lang-json";
import { html } from "@codemirror/lang-html";
import { css } from "@codemirror/lang-css";
import { markdown } from "@codemirror/lang-markdown";
import { python } from "@codemirror/lang-python";
import { lspExtension } from "@/lib/lsp";
import { useUIStore } from "@/stores/ui";

const props = defineProps<{ leafId: number; path: string; cwd: string }>();
const emit = defineEmits<{
  title: [t: string];
  dirty: [d: boolean];
  saved: [];
  error: [msg: string];
}>();

const ui = useUIStore();
const host = ref<HTMLElement | null>(null);
const placeholder = ref<string>("");

// CM owns its own DOM + state. NEVER wrap in ref/reactive — Vue's proxy corrupts
// CM internals. Bare module-local handle.
let view: EditorView | null = null;
let savedDoc = "";
let lastDirty = false;
let saving = false;

const themeCompartment = new Compartment();

function basename(p: string): string {
  return p.split("/").pop() || p;
}

// Pick a CodeMirror language by file extension. Unknown → no language (plain).
function detectLanguage(p: string) {
  const ext = p.split(".").pop()?.toLowerCase() ?? "";
  switch (ext) {
    case "ts":
    case "tsx":
    case "mts":
    case "cts":
      return javascript({ typescript: true, jsx: ext === "tsx" });
    case "js":
    case "jsx":
    case "mjs":
    case "cjs":
      return javascript({ jsx: ext === "jsx" });
    case "rs":
      return rust();
    case "json":
      return json();
    case "html":
    case "htm":
    case "vue":
      return html();
    case "css":
    case "scss":
    case "less":
      return css();
    case "md":
    case "markdown":
      return markdown();
    case "py":
      return python();
    default:
      return [];
  }
}

function editorTheme() {
  return EditorView.theme({
    "&": { height: "100%", fontSize: `${ui.terminalFontSize}px` },
    ".cm-scroller": { fontFamily: ui.terminalFont, overflow: "auto" },
    ".cm-content": { fontFamily: ui.terminalFont },
  });
}

function recomputeDirty() {
  if (!view) return;
  const dirty = view.state.doc.toString() !== savedDoc;
  if (dirty !== lastDirty) {
    lastDirty = dirty;
    emit("dirty", dirty);
  }
}

async function save() {
  if (!view || saving) return;
  saving = true;
  const content = view.state.doc.toString();
  try {
    await invoke("write_text_file", { path: props.path, content });
    savedDoc = content;
    if (lastDirty) {
      lastDirty = false;
      emit("dirty", false);
    }
    emit("saved");
  } catch (e) {
    emit("error", String(e));
  } finally {
    saving = false;
  }
}

function isDirty(): boolean {
  return lastDirty;
}

function focus() {
  view?.focus();
}

onMounted(async () => {
  emit("title", basename(props.path));

  let content: string;
  try {
    content = await invoke<string>("read_text_file_checked", { path: props.path });
  } catch (e) {
    const msg = String(e);
    if (msg.includes("binary")) placeholder.value = "Binary file — cannot edit";
    else if (msg.includes("too-large")) placeholder.value = "File too large to open";
    else {
      placeholder.value = "Could not open file";
      emit("error", msg);
    }
    return;
  }

  savedDoc = content;
  if (!host.value) return;

  // LSP (completion/hover/diagnostics/go-to-def) for supported languages. Async
  // server startup; resolves to [] when unsupported or the server is missing, so
  // the editor always mounts.
  const lspExt = await lspExtension(props.path, props.cwd);
  if (!host.value) return; // leaf closed while we awaited

  const state = EditorState.create({
    doc: content,
    extensions: [
      basicSetup,
      detectLanguage(props.path),
      lspExt,
      oneDark,
      themeCompartment.of(editorTheme()),
      EditorView.updateListener.of((u) => {
        if (u.docChanged) recomputeDirty();
      }),
      // High precedence so ⌘S beats CM defaults and never reaches the OS
      // "save page" dialog (run returns true → preventDefault).
      Prec.highest(
        keymap.of([{ key: "Mod-s", run: () => { void save(); return true; } }]),
      ),
      keymap.of([indentWithTab]),
    ],
  });
  view = new EditorView({ state, parent: host.value });
});

// Live font/size changes — reconfigure the theme compartment, mirroring XTerm.
watch(
  () => [ui.terminalFont, ui.terminalFontSize],
  () => view?.dispatch({ effects: themeCompartment.reconfigure(editorTheme()) }),
);

onBeforeUnmount(() => {
  view?.destroy();
  view = null;
});

defineExpose({ focus, save, isDirty });
</script>

<style scoped>
.code-editor {
  flex: 1;
  display: flex;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
}
.editor-host {
  flex: 1;
  min-height: 0;
  overflow: hidden;
}
.editor-host :deep(.cm-editor) {
  height: 100%;
}
.editor-placeholder {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  color: var(--text-muted);
  font-size: 12px;
  background: var(--terminal-bg);
}
</style>
