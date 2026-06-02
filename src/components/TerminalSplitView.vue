<template>
  <div
    v-if="node.type === 'leaf'"
    class="split-leaf"
    :class="{ focused: focusedId === node.id }"
    @mousedown.capture="$emit('focus', (node as Leaf).id)"
  >
    <XTerm
      v-if="(node as Leaf).leafType !== 'diff'"
      :pty-id="(node as Leaf).id"
      :cwd="cwd"
      :initial-cmd="(node as Leaf).initialCmd"
      :ref="(el: unknown) => registerRef((node as Leaf).id, el)"
      @title="(t: string) => $emit('title', (node as Leaf).id, t)"
      @busy="(b: boolean) => $emit('busy', (node as Leaf).id, b)"
    />
    <DiffTab
      v-else
      :diff-file="(node as Leaf).diffFile!"
      :diff-staged="(node as Leaf).diffStaged ?? false"
      :diff="(node as Leaf).diff || ''"
    />
  </div>
  <div
    v-else
    class="split-container"
    :class="node.direction === 'h' ? 'split-h' : 'split-v'"
  >
    <TerminalSplitView
      :node="node.first"
      :cwd="cwd"
      :focused-id="focusedId"
      @focus="$emit('focus', $event)"
      @title="(id, t) => $emit('title', id, t)"
      @busy="(id, b) => $emit('busy', id, b)"
    />
    <div class="split-divider" :class="node.direction === 'h' ? 'divider-v' : 'divider-h'" />
    <TerminalSplitView
      :node="node.second"
      :cwd="cwd"
      :focused-id="focusedId"
      @focus="$emit('focus', $event)"
      @title="(id, t) => $emit('title', id, t)"
      @busy="(id, b) => $emit('busy', id, b)"
    />
  </div>
</template>

<script setup lang="ts">
import { inject } from "vue";
import XTerm from "./XTerm.vue";
import DiffTab from "./DiffTab.vue";
import TerminalSplitView from "./TerminalSplitView.vue";

export interface Leaf {
  type: "leaf";
  id: number;
  title: string;
  defaultTitle: string;
  isAgent: boolean;
  busy: boolean;
  status: "idle" | "running" | "waiting" | "done" | "review";
  initialCmd?: string;
  cwd?: string;          // per-tab cwd override (else workspace cwd)
  resultToken?: string;  // set on tabs spawned via `burrow spawn --token`
  leafType?: "terminal" | "diff";  // default "terminal"
  diffFile?: string;
  diffStaged?: boolean;
  diff?: string;
}

export interface SplitNode {
  type: "split";
  direction: "h" | "v";
  first: TreeNode;
  second: TreeNode;
}

export type TreeNode = Leaf | SplitNode;

defineProps<{
  node: TreeNode;
  cwd: string;
  focusedId: number;
}>();

defineEmits<{
  focus: [id: number];
  title: [id: number, t: string];
  busy: [id: number, b: boolean];
}>();

const registerRef = inject<(id: number, el: unknown) => void>("registerRef")!;
</script>

<style scoped>
.split-leaf {
  flex: 1;
  display: flex;
  overflow: hidden;
  min-width: 0;
  min-height: 0;
  position: relative;
}

.split-leaf.focused::after {
  content: "";
  position: absolute;
  inset: 0;
  pointer-events: none;
  border: 1px solid var(--accent);
  opacity: 0.35;
  z-index: 1;
}

.split-container {
  flex: 1;
  display: flex;
  overflow: hidden;
  min-width: 0;
  min-height: 0;
}

.split-h {
  flex-direction: row;
}

.split-v {
  flex-direction: column;
}

.split-divider {
  background: var(--border);
  flex-shrink: 0;
}

.divider-v {
  width: 1px;
}

.divider-h {
  height: 1px;
}
</style>
