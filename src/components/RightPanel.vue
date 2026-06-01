<template>
  <aside class="right-panel">
    <!-- Tab bar -->
    <div class="panel-tabs">
      <button
        v-for="tab in tabs"
        :key="tab.id"
        class="panel-tab"
        :class="{ active: activeTab === tab.id }"
        @click="activeTab = tab.id"
      >
        <component :is="tab.icon" :size="12" />
        {{ tab.label }}
      </button>
    </div>

    <!-- Explorer tab -->
    <div v-if="activeTab === 'explorer'" class="panel-content">
      <div v-if="!props.cwd" class="hint">No workspace open</div>
      <div v-else-if="fileTree.rootError" class="hint error">{{ fileTree.rootError }}</div>
      <div v-else class="file-tree">
        <FileTreeNode v-for="node in fileTree.tree" :key="node.id" :node="node" :depth="0" />
      </div>
    </div>

    <!-- Git tab -->
    <div v-else-if="activeTab === 'git'" class="panel-content git-panel">
      <!-- Header -->
      <div class="git-header">
        <div class="branch-tag">
          <PhGitBranch :size="12" />
          <span>{{ git.branch || "—" }}</span>
        </div>
        <button class="icon-btn" :disabled="git.loading" @click="git.refresh()" title="Refresh">
          <PhArrowClockwise :size="13" :class="{ spin: git.loading }" />
        </button>
      </div>

      <div class="git-body">
        <!-- Error -->
        <div v-if="git.error" class="git-error">
          <PhWarning :size="13" />
          Not a git repository
          <button class="git-init-btn" :disabled="git.loading" @click="git.gitInit()">
            <PhGitBranch :size="12" />
            Git Init
          </button>
        </div>

        <template v-else>
          <!-- Staged -->
          <div class="section-label">Staged</div>
          <div v-if="git.staged.length === 0" class="empty-hint">Nothing staged</div>
          <div
            v-for="f in git.staged"
            :key="'s:' + f.path"
            class="git-file staged"
            @click="git.showDiff(f.path, true)"
          >
            <span class="file-status">{{ f.x }}</span>
            <span class="file-path" :title="f.path">{{ f.path }}</span>
            <button class="file-btn" @click.stop="git.unstageFile(f.path)" title="Unstage">−</button>
          </div>

          <!-- Unstaged + untracked -->
          <div class="section-label" style="margin-top: 8px;">Changes</div>
          <div v-if="git.unstaged.length === 0 && git.untracked.length === 0" class="empty-hint">
            Working tree clean
          </div>
          <div
            v-for="f in git.unstaged"
            :key="'u:' + f.path"
            class="git-file unstaged"
            @click="git.showDiff(f.path, false)"
          >
            <span class="file-status">{{ f.y }}</span>
            <span class="file-path" :title="f.path">{{ f.path }}</span>
            <button class="file-btn add" @click.stop="git.stageFile(f.path)" title="Stage">+</button>
          </div>
          <div
            v-for="f in git.untracked"
            :key="'t:' + f.path"
            class="git-file untracked"
          >
            <span class="file-status">?</span>
            <span class="file-path" :title="f.path">{{ f.path }}</span>
            <button class="file-btn add" @click.stop="git.stageFile(f.path)" title="Stage">+</button>
          </div>

          <!-- Commit -->
          <div class="commit-section">
            <textarea
              v-model="git.commitMsg"
              class="commit-input"
              placeholder="Commit message…"
              rows="2"
              @keydown.ctrl.enter="git.commit()"
            />
            <button
              class="commit-btn"
              :disabled="!git.commitMsg.trim() || git.staged.length === 0"
              @click="git.commit()"
            >
              <PhGitCommit :size="12" />
              Commit
            </button>
          </div>

          <!-- Diff -->
          <div v-if="git.diffFile" class="diff-section">
            <div class="diff-header">
              <span class="diff-title">{{ git.diffFile }}</span>
              <span class="diff-mode">{{ git.diffStaged ? "staged" : "unstaged" }}</span>
              <button class="icon-btn" @click="git.clearDiff()" title="Close">
                <PhX :size="11" />
              </button>
            </div>
            <pre class="diff-view"><span
              v-for="(line, i) in git.diff.split('\n')"
              :key="i"
              :class="diffLineClass(line)"
            >{{ line }}
</span></pre>
          </div>
        </template>
      </div>
    </div>
  </aside>
</template>

<script setup lang="ts">
import { ref, watch } from "vue";
import {
  PhFiles, PhGitBranch, PhGitCommit,
  PhArrowClockwise, PhWarning, PhX,
} from "@phosphor-icons/vue";
import { useGitStore } from "@/stores/git";
import { useFileTreeStore } from "@/stores/fileTree";
import FileTreeNode from "./FileTreeNode.vue";

const props = defineProps<{ cwd: string }>();
const git = useGitStore();
const fileTree = useFileTreeStore();
const activeTab = ref("explorer");

const tabs = [
  { id: "explorer", label: "Explorer", icon: PhFiles },
  { id: "git",      label: "Git",      icon: PhGitBranch },
];

watch(() => props.cwd, (p) => {
  if (p) {
    git.setCwd(p);
    fileTree.loadRoot(p);
  } else {
    fileTree.clearTree();
  }
}, { immediate: true });

function diffLineClass(line: string) {
  if (line.startsWith("+") && !line.startsWith("+++")) return "diff-add";
  if (line.startsWith("-") && !line.startsWith("---")) return "diff-del";
  if (line.startsWith("@@")) return "diff-hunk";
  return "diff-ctx";
}
</script>

<style scoped>
.right-panel {
  width: var(--right-panel-width);
  background: var(--bg-panel);
  border-left: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
  overflow: hidden;
  font-size: 12px;
}

.panel-tabs {
  display: flex;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}

.panel-tab {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 5px;
  background: none;
  border: none;
  border-bottom: 2px solid transparent;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 11px;
  font-family: var(--font-ui);
  padding: 7px 4px;
}
.panel-tab:hover  { color: var(--text-primary); }
.panel-tab.active { color: var(--text-primary); border-bottom-color: var(--accent); }

.panel-content {
  flex: 1;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
}

.hint {
  font-size: 11px;
  color: var(--text-muted);
  padding: 16px;
  text-align: center;
}
.hint.error { color: var(--red); }

.file-tree {
  flex: 1;
  padding: 4px 0;
}

/* Git panel */
.git-panel {
  overflow: hidden;
}

.git-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 10px;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}

.branch-tag {
  display: flex;
  align-items: center;
  gap: 5px;
  color: var(--yellow);
  font-size: 11px;
  font-weight: 600;
}

.icon-btn {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  padding: 2px 4px;
  border-radius: 3px;
  display: flex;
  align-items: center;
}
.icon-btn:hover { color: var(--text-primary); background: var(--bg-hover); }
.icon-btn:disabled { opacity: 0.4; cursor: default; }

@keyframes spin { to { transform: rotate(360deg); } }
.spin { animation: spin 1s linear infinite; }

.git-body {
  flex: 1;
  overflow-y: auto;
  padding: 8px 0;
  display: flex;
  flex-direction: column;
}

.git-error {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px;
  color: var(--text-secondary);
  padding: 16px 12px;
  font-size: 11px;
}

.git-init-btn {
  display: flex;
  align-items: center;
  gap: 4px;
  margin-left: auto;
  padding: 3px 8px;
  border-radius: 4px;
  border: 1px solid var(--border);
  background: var(--bg-hover);
  color: var(--text-primary);
  font-size: 11px;
  cursor: pointer;
}
.git-init-btn:hover { background: var(--yellow); color: var(--bg-primary); border-color: var(--yellow); }
.git-init-btn:disabled { opacity: 0.4; cursor: default; }

.section-label {
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.07em;
  text-transform: uppercase;
  color: var(--text-muted);
  padding: 2px 10px 4px;
}

.empty-hint {
  font-size: 11px;
  color: var(--text-muted);
  padding: 2px 10px 6px;
}

.git-file {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 2px 10px;
  cursor: pointer;
  border-radius: 3px;
  margin: 0 4px;
}
.git-file:hover { background: var(--bg-hover); }

.file-status {
  font-family: var(--font-mono);
  font-size: 11px;
  width: 12px;
  flex-shrink: 0;
  text-align: center;
}
.staged   .file-status { color: var(--green); }
.unstaged .file-status { color: var(--yellow); }
.untracked .file-status { color: var(--text-muted); }

.file-path {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 11px;
  color: var(--text-primary);
}

.file-btn {
  background: none;
  border: none;
  cursor: pointer;
  color: var(--text-muted);
  font-size: 14px;
  line-height: 1;
  padding: 0 2px;
  flex-shrink: 0;
  display: none;
}
.git-file:hover .file-btn { display: block; }
.file-btn:hover { color: var(--text-primary); }
.file-btn.add:hover { color: var(--green); }

/* Commit */
.commit-section {
  margin-top: 10px;
  padding: 10px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  border-top: 1px solid var(--border);
  flex-shrink: 0;
}

.commit-input {
  background: var(--bg-hover);
  border: 1px solid var(--border);
  border-radius: 4px;
  color: var(--text-primary);
  font-family: var(--font-ui);
  font-size: 11px;
  line-height: 1.5;
  outline: none;
  padding: 6px 8px;
  resize: none;
  width: 100%;
}
.commit-input:focus { border-color: var(--accent); }

.commit-btn {
  align-self: flex-end;
  display: flex;
  align-items: center;
  gap: 5px;
  background: var(--accent);
  border: none;
  border-radius: 4px;
  color: #fff;
  cursor: pointer;
  font-size: 11px;
  font-weight: 600;
  padding: 4px 10px;
}
.commit-btn:hover:not(:disabled) { background: var(--accent-dim); }
.commit-btn:disabled { opacity: 0.4; cursor: default; }

/* Diff */
.diff-section {
  border-top: 1px solid var(--border);
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  max-height: 220px;
}

.diff-header {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 10px;
  background: var(--bg-hover);
  flex-shrink: 0;
}

.diff-title {
  font-family: var(--font-mono);
  font-size: 10px;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
}

.diff-mode {
  font-size: 10px;
  color: var(--text-muted);
  flex-shrink: 0;
}

.diff-view {
  overflow: auto;
  font-family: var(--font-mono);
  font-size: 10px;
  line-height: 1.5;
  white-space: pre;
  margin: 0;
  padding: 6px 0;
  flex: 1;
}

.diff-add  { color: var(--green); display: block; }
.diff-del  { color: var(--red); display: block; }
.diff-hunk { color: var(--accent); display: block; }
.diff-ctx  { color: var(--text-secondary); display: block; }
</style>
