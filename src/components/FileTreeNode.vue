<template>
  <div>
    <div
      class="tree-row"
      :class="{ selected: store.selectedId === node.id }"
      :style="{ paddingLeft: `${8 + depth * 12}px` }"
      @click="handleClick"
    >
      <PhSpinner    v-if="node.loading" class="row-arrow spin" :size="10" />
      <PhCaretRight v-else-if="node.type === 'folder' && !node.expanded" class="row-arrow" :size="10" weight="bold" />
      <PhCaretDown  v-else-if="node.type === 'folder' && node.expanded" class="row-arrow" :size="10" weight="bold" />
      <span v-else class="row-arrow placeholder" />

      <PhFolderOpen v-if="node.type === 'folder' && node.expanded" class="row-icon folder" :size="14" weight="fill" />
      <PhFolder     v-else-if="node.type === 'folder'"             class="row-icon folder" :size="14" weight="fill" />
      <component    v-else :is="fileIconComponent(node.name)"      class="row-icon file"   :size="14" weight="regular" />

      <span class="row-name">{{ node.name }}</span>
    </div>

    <template v-if="node.type === 'folder' && node.expanded && node.children">
      <FileTreeNode v-for="child in node.children" :key="child.id" :node="child" :depth="depth + 1" />
    </template>
  </div>
</template>

<script setup lang="ts">
import {
  PhCaretRight, PhCaretDown,
  PhFolder, PhFolderOpen,
  PhFileVue, PhFileTs, PhFileJs, PhFileCode,
  PhGear, PhFile, PhSpinner,
} from "@phosphor-icons/vue";
import { useFileTreeStore, type FileNode } from "@/stores/fileTree";

const props = defineProps<{ node: FileNode; depth: number }>();
const store = useFileTreeStore();

function handleClick() {
  if (props.node.type === "folder") store.toggle(props.node.id);
  else store.select(props.node.id);
}

function fileIconComponent(name: string) {
  if (name.endsWith(".vue"))  return PhFileVue;
  if (name.endsWith(".ts"))   return PhFileTs;
  if (name.endsWith(".js"))   return PhFileJs;
  if (name.endsWith(".rs"))   return PhFileCode;
  if (name.endsWith(".json") || name.endsWith(".toml")) return PhGear;
  return PhFile;
}
</script>

<style scoped>
.tree-row {
  display: flex;
  align-items: center;
  gap: 5px;
  height: 22px;
  font-size: 12px;
  color: var(--text-primary);
  cursor: pointer;
  white-space: nowrap;
  border-radius: 3px;
  margin: 0 4px;
}
.tree-row:hover   { background: var(--bg-hover); }
.tree-row.selected { background: var(--bg-selected); }

.row-arrow {
  width: 10px;
  flex-shrink: 0;
  color: var(--text-secondary);
}
.row-arrow.placeholder { opacity: 0; }
@keyframes spin { to { transform: rotate(360deg); } }
.row-arrow.spin { animation: spin 1s linear infinite; }

.row-icon {
  flex-shrink: 0;
}
.row-icon.folder { color: #60a5fa; }
.row-icon.file   { color: var(--text-secondary); }

.row-name {
  overflow: hidden;
  text-overflow: ellipsis;
  flex: 1;
  font-size: 12px;
}
</style>
