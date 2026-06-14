<template>
  <div class="git-panel">

    <!-- Top bar: title + workspace selector + actions -->
    <div class="gp-topbar">
      <span class="gp-title">Git</span>

      <!-- Workspace selector -->
      <div class="ws-select-wrap">
        <PhFolder :size="11" class="ws-select-icon" />
        <select v-model="selectedWsId" class="ws-select">
          <option v-for="w in wsStore.topLevel" :key="w.id" :value="w.id">
            {{ w.name }}
          </option>
        </select>
        <PhCaretDown :size="9" class="ws-select-caret" />
      </div>

      <!-- Branch chip -->
      <button
        class="branch-chip"
        :class="{ open: showBranchDropdown }"
        @click="toggleBranchDropdown"
        :title="`Branch: ${git.branch}`"
        :disabled="!!git.error"
      >
        <PhGitBranch :size="11" />
        <span class="branch-name">{{ git.branch || "—" }}</span>
        <span v-if="git.ahead > 0" class="branch-ahead">↑{{ git.ahead }}</span>
        <span v-if="git.behind > 0" class="branch-behind">↓{{ git.behind }}</span>
        <PhCaretDown :size="8" class="branch-caret" />
      </button>

      <div class="gp-spacer" />

      <!-- Network actions -->
      <button class="gp-action-btn" :disabled="git.fetching || git.loading" @click="git.fetch()" title="Fetch">
        <PhArrowsClockwise :size="13" :class="{ spin: git.fetching }" />
        Fetch
      </button>
      <button
        v-if="!git.error && git.hasUpstream && git.behind > 0"
        class="gp-action-btn"
        :disabled="git.pulling || git.pushing"
        @click="git.pull()"
        title="Pull (ff-only)"
      >
        <PhArrowDown :size="13" :class="{ spin: git.pulling }" />
        Pull <span class="gp-count-badge">{{ git.behind }}</span>
      </button>
      <button
        v-if="!git.error"
        class="gp-action-btn"
        :disabled="git.pushing || git.loading || (git.hasUpstream && git.ahead === 0)"
        @click="git.push()"
        :title="git.hasUpstream ? 'Push' : 'Publish branch'"
      >
        <PhArrowUp :size="13" :class="{ spin: git.pushing }" />
        {{ git.hasUpstream ? "Push" : "Publish" }}
        <span v-if="git.ahead > 0" class="gp-count-badge">{{ git.ahead }}</span>
      </button>
      <button class="gp-icon-btn" :disabled="git.loading" @click="git.refresh()" title="Refresh">
        <PhArrowClockwise :size="14" :class="{ spin: git.loading }" />
      </button>
    </div>

    <!-- Branch dropdown -->
    <template v-if="showBranchDropdown">
      <div class="branch-overlay" @click="showBranchDropdown = false" />
      <div class="branch-dropdown">
        <div
          v-for="b in git.branches"
          :key="b"
          class="bd-item"
          :class="{ current: b === git.branch }"
          @click="selectBranch(b)"
        >
          <PhCheck v-if="b === git.branch" :size="10" class="bd-check-icon" />
          <span v-else class="bd-check-icon" />
          {{ b }}
        </div>
        <div class="bd-sep" />
        <div v-if="!newBranchMode" class="bd-new" @click.stop="startNewBranch">
          <PhPlus :size="10" /> New branch…
        </div>
        <div v-else class="bd-new-input">
          <input
            ref="newBranchInputRef"
            v-model="newBranchName"
            class="bd-input"
            placeholder="branch-name"
            @keydown.enter.prevent="confirmNewBranch"
            @keydown.esc="newBranchMode = false"
          />
        </div>
      </div>
    </template>

    <!-- Push/pull progress bar -->
    <div v-if="git.pushing || git.pulling" class="gp-progress">
      <div class="gp-progress-bar" />
      <span class="gp-progress-label">{{ git.pushing ? "Pushing…" : "Pulling…" }}</span>
    </div>

    <!-- No repo error -->
    <div v-if="git.error" class="gp-no-repo">
      <PhWarning :size="20" />
      <span>Not a git repository</span>
      <button class="gp-init-btn" :disabled="git.loading" @click="git.gitInit()">
        <PhGitBranch :size="12" /> Git Init
      </button>
    </div>

    <!-- Main two-column layout -->
    <div v-else class="gp-body">

      <!-- LEFT: file lists + commit -->
      <div class="gp-left">
        <div class="gp-scroll">

          <!-- STAGED -->
          <div class="gp-section-row">
            <span class="gp-section-label">Staged <span v-if="git.staged.length" class="gp-badge">{{ git.staged.length }}</span></span>
            <button v-if="git.staged.length" class="gp-sec-btn" @click="git.unstageAll()">−All</button>
          </div>
          <div v-if="git.staged.length === 0" class="gp-empty">Nothing staged</div>
          <div
            v-for="f in git.staged"
            :key="'s:' + f.path"
            class="gp-file"
            :class="{ active: activeDiff?.path === f.path && activeDiff?.staged }"
            @click="toggleDiff(f.path, true)"
          >
            <span class="gp-status staged">{{ f.x }}</span>
            <span class="gp-file-path" :title="f.path">{{ f.path }}</span>
            <button class="gp-file-btn" @click.stop="git.unstageFile(f.path)" title="Unstage">−</button>
          </div>

          <!-- CHANGES -->
          <div class="gp-section-row" style="margin-top: 10px">
            <span class="gp-section-label">Changes <span v-if="changesCount" class="gp-badge">{{ changesCount }}</span></span>
            <button v-if="changesCount" class="gp-sec-btn" @click="git.stageAll()">+All</button>
          </div>
          <div v-if="!changesCount" class="gp-empty">Working tree clean</div>
          <div
            v-for="f in git.unstaged"
            :key="'u:' + f.path"
            class="gp-file"
            :class="{ active: activeDiff?.path === f.path && !activeDiff?.staged }"
            @click="toggleDiff(f.path, false)"
          >
            <span class="gp-status modified">{{ f.y }}</span>
            <span class="gp-file-path" :title="f.path">{{ f.path }}</span>
            <div class="gp-file-btns">
              <button class="gp-file-btn stage" @click.stop="git.stageFile(f.path)" title="Stage">+</button>
              <button class="gp-file-btn discard" @click.stop="git.discardFile(f.path)" title="Discard">✕</button>
            </div>
          </div>
          <div
            v-for="f in git.untracked"
            :key="'t:' + f.path"
            class="gp-file"
          >
            <span class="gp-status untracked">?</span>
            <span class="gp-file-path" :title="f.path">{{ f.path }}</span>
            <div class="gp-file-btns">
              <button class="gp-file-btn stage" @click.stop="git.stageFile(f.path)" title="Stage">+</button>
            </div>
          </div>
        </div>

        <!-- Commit area (pinned to bottom of left column) -->
        <div class="gp-commit">
          <textarea
            v-model="git.commitMsg"
            class="gp-commit-input"
            placeholder="Commit message…"
            rows="3"
            @keydown.ctrl.enter="git.commit()"
            @keydown.meta.enter="git.commit()"
          />
          <div class="gp-type-chips">
            <button
              v-for="t in COMMIT_TYPES"
              :key="t"
              class="gp-chip"
              :class="{ active: activeType === t }"
              @click="applyType(t)"
            >{{ t }}</button>
          </div>
          <div class="gp-commit-btns">
            <button
              class="gp-commit-btn"
              :disabled="!git.commitMsg.trim() || git.staged.length === 0"
              @click="git.commit()"
              title="⌘↵"
            >
              <PhGitCommit :size="12" /> Commit
            </button>
            <button
              class="gp-commit-btn primary"
              :disabled="!git.commitMsg.trim() || git.staged.length === 0 || git.pushing"
              @click="commitAndPush()"
            >
              <PhArrowUp :size="12" /> Commit & Push
            </button>
          </div>
        </div>
      </div>

      <!-- RIGHT: diff + history -->
      <div class="gp-right">

        <!-- Diff view -->
        <div v-if="git.diffFile" class="gp-diff">
          <div class="gp-diff-header">
            <span class="gp-diff-title">{{ git.diffFile }}</span>
            <span class="gp-diff-mode">{{ git.diffStaged ? "staged" : "unstaged" }}</span>
            <button class="gp-icon-btn" @click="git.clearDiff(); activeDiff = null" title="Close diff"><PhX :size="11" /></button>
          </div>
          <pre class="gp-diff-view"><span
            v-for="(line, idx) in git.diff.split('\n')"
            :key="idx"
            :class="diffLineClass(line)"
          >{{ line }}
</span></pre>
        </div>
        <div v-else class="gp-diff-empty">
          <PhArrowLeft :size="16" />
          Click a file to view diff
        </div>

        <!-- History -->
        <div class="gp-history">
          <div class="gp-history-header" @click="showHistory = !showHistory">
            <PhCaretRight :size="10" :class="{ open: showHistory }" />
            <span class="gp-section-label">History</span>
            <span v-if="git.ahead > 0" class="gp-badge" style="margin-left: 4px">{{ git.ahead }} unpushed</span>
          </div>
          <div v-if="showHistory" class="gp-history-list">
            <div v-if="git.log.length === 0" class="gp-empty" style="padding: 6px 12px">No commits</div>
            <div
              v-for="(c, i) in git.log"
              :key="c.hash"
              class="gp-log-row"
              :class="{ unpushed: i < git.ahead }"
              :title="c.subject + '\n' + c.author + (i < git.ahead ? '\n↑ Not pushed' : '')"
              @click="openCommitDiff(c)"
            >
              <span class="gp-log-hash">{{ c.shortHash }}</span>
              <span v-if="i < git.ahead" class="gp-log-up">↑</span>
              <span class="gp-log-subject">{{ c.subject }}</span>
              <span class="gp-log-author">{{ c.author }}</span>
              <span class="gp-log-time">{{ c.relTime }}</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick, inject } from "vue";
import { invoke } from "@tauri-apps/api/core";
import {
  PhGitBranch, PhGitCommit, PhFolder,
  PhArrowUp, PhArrowDown, PhArrowLeft, PhArrowClockwise, PhArrowsClockwise,
  PhCaretDown, PhCaretRight,
  PhWarning, PhX, PhCheck, PhPlus,
} from "@phosphor-icons/vue";
import { useGitStore, type GitCommit } from "@/stores/git";
import { useWorkspaceStore } from "@/stores/workspace";

const git = useGitStore();
const wsStore = useWorkspaceStore();
const activeTerm = inject<() => any>("activeTerm", () => undefined);

const selectedWsId = ref<number | null>(wsStore.active?.id ?? null);
const showBranchDropdown = ref(false);
const newBranchMode = ref(false);
const newBranchName = ref("");
const newBranchInputRef = ref<HTMLInputElement | null>(null);
const showHistory = ref(true);
const activeDiff = ref<{ path: string; staged: boolean } | null>(null);

const COMMIT_TYPES = ["feat", "fix", "docs", "chore", "refactor", "test", "style"] as const;

const changesCount = computed(() => git.unstaged.length + git.untracked.length);

const activeType = computed(() => {
  const m = git.commitMsg.match(/^([a-z]+)(\([^)]+\))?:\s/);
  return m ? m[1] : null;
});

// When workspace selection changes, point git store at that workspace
watch(selectedWsId, (id) => {
  const w = wsStore.topLevel.find((w) => w.id === id);
  if (w) git.setCwd(w.path);
}, { immediate: true });

// When active workspace changes externally, follow it
watch(() => wsStore.active?.id, (id) => {
  if (id != null) selectedWsId.value = id;
});

function toggleBranchDropdown() {
  showBranchDropdown.value = !showBranchDropdown.value;
  if (showBranchDropdown.value) newBranchMode.value = false;
}

async function selectBranch(name: string) {
  showBranchDropdown.value = false;
  if (name === git.branch) return;
  await git.switchBranch(name);
}

async function startNewBranch() {
  newBranchMode.value = true;
  newBranchName.value = "";
  await nextTick();
  newBranchInputRef.value?.focus();
}

async function confirmNewBranch() {
  const name = newBranchName.value.trim();
  if (!name) return;
  newBranchMode.value = false;
  showBranchDropdown.value = false;
  await git.createBranch(name);
}

function toggleDiff(path: string, staged: boolean) {
  if (activeDiff.value?.path === path && activeDiff.value?.staged === staged) {
    activeDiff.value = null;
    git.clearDiff();
  } else {
    activeDiff.value = { path, staged };
    git.showDiff(path, staged);
  }
}

function diffLineClass(line: string) {
  if (line.startsWith("+") && !line.startsWith("+++")) return "diff-add";
  if (line.startsWith("-") && !line.startsWith("---")) return "diff-del";
  if (line.startsWith("@@")) return "diff-hunk";
  return "diff-ctx";
}

function applyType(t: string) {
  const current = git.commitMsg;
  const typePrefix = /^[a-z]+(\([^)]+\))?:\s/;
  if (typePrefix.test(current)) {
    git.commitMsg = current.replace(typePrefix, `${t}: `);
  } else {
    git.commitMsg = `${t}: ${current}`;
  }
}

async function commitAndPush() {
  await git.commit();
  await git.push();
}

async function openCommitDiff(c: GitCommit) {
  const w = wsStore.topLevel.find((w) => w.id === selectedWsId.value);
  if (!w) return;
  const out = await invoke<{ stdout: string; stderr: string; code: number }>("run_git", {
    cwd: w.path,
    args: ["show", c.hash],
  });
  if (out.code !== 0 || !out.stdout) return;
  activeTerm()?.openDiffInTab(`${c.shortHash} ${c.subject}`, false, out.stdout);
}
</script>

<style scoped>
.git-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
  background: var(--bg-base);
  font-size: 12px;
}

/* Top bar */
.gp-topbar {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 12px;
  height: 38px;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
  background: var(--bg-panel);
}

.gp-title {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.05em;
  text-transform: uppercase;
  color: var(--text-muted);
  opacity: 0.7;
  flex-shrink: 0;
}

/* Workspace selector */
.ws-select-wrap {
  position: relative;
  display: flex;
  align-items: center;
  background: color-mix(in srgb, var(--border) 25%, var(--bg-panel));
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 0 6px 0 7px;
  height: 24px;
  gap: 4px;
}
.ws-select-icon { color: var(--text-muted); flex-shrink: 0; }
.ws-select {
  background: none;
  border: none;
  color: var(--text-secondary);
  font-family: var(--font-ui);
  font-size: 11px;
  outline: none;
  cursor: pointer;
  padding: 0;
  padding-right: 14px;
  appearance: none;
  -webkit-appearance: none;
}
.ws-select:focus { color: var(--text-primary); }
.ws-select-caret {
  position: absolute;
  right: 5px;
  color: var(--text-muted);
  pointer-events: none;
}

/* Branch chip */
.branch-chip {
  display: flex;
  align-items: center;
  gap: 4px;
  background: none;
  border: 1px solid var(--border);
  border-radius: 4px;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 11px;
  font-family: var(--font-mono);
  padding: 3px 7px;
  height: 24px;
  transition: background 0.1s, border-color 0.1s, color 0.1s;
  max-width: 220px;
}
.branch-chip:hover:not(:disabled),
.branch-chip.open { background: var(--bg-hover); color: var(--text-primary); }
.branch-chip:disabled { opacity: 0.4; cursor: default; }
.branch-chip :deep(svg:first-child) { color: var(--yellow); flex-shrink: 0; }
.branch-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.branch-ahead { color: var(--green); font-size: 10px; flex-shrink: 0; }
.branch-behind { color: var(--yellow); font-size: 10px; flex-shrink: 0; }
.branch-caret { color: var(--text-muted); flex-shrink: 0; transition: transform 0.12s; }
.branch-chip.open .branch-caret { transform: rotate(180deg); }

.gp-spacer { flex: 1; }

/* Action buttons */
.gp-action-btn {
  display: flex;
  align-items: center;
  gap: 4px;
  background: none;
  border: 1px solid var(--border);
  border-radius: 4px;
  color: var(--text-muted);
  cursor: pointer;
  font-family: var(--font-ui);
  font-size: 11px;
  font-weight: 500;
  padding: 3px 8px;
  height: 24px;
  transition: background 0.1s, color 0.1s, border-color 0.1s;
  white-space: nowrap;
}
.gp-action-btn:hover:not(:disabled) {
  background: var(--bg-hover);
  color: var(--text-primary);
  border-color: color-mix(in srgb, var(--accent) 35%, var(--border));
}
.gp-action-btn:disabled { opacity: 0.3; cursor: default; }

.gp-count-badge {
  background: color-mix(in srgb, var(--accent) 20%, transparent);
  border-radius: 8px;
  color: var(--accent);
  font-size: 9px;
  font-weight: 600;
  padding: 0 4px;
}

.gp-icon-btn {
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  padding: 4px;
  border-radius: 3px;
  display: flex;
  align-items: center;
  transition: color 0.1s, background 0.1s;
}
.gp-icon-btn:hover:not(:disabled) { color: var(--text-primary); background: var(--bg-hover); }
.gp-icon-btn:disabled { opacity: 0.3; cursor: default; }

@keyframes spin { to { transform: rotate(360deg); } }
.spin { animation: spin 0.9s linear infinite; }

/* Branch dropdown */
.branch-overlay { position: fixed; inset: 0; z-index: 99; }
.branch-dropdown {
  position: absolute;
  top: 42px;
  left: 240px;
  min-width: 220px;
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: 5px;
  box-shadow: 0 6px 20px rgba(0,0,0,0.35);
  z-index: 100;
  padding: 3px 0;
  max-height: 260px;
  overflow-y: auto;
}
.bd-item {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 5px 12px;
  font-size: 12px;
  font-family: var(--font-mono);
  color: var(--text-secondary);
  cursor: pointer;
  transition: background 0.08s;
}
.bd-item:hover { background: var(--bg-hover); color: var(--text-primary); }
.bd-item.current { color: var(--text-primary); }
.bd-check-icon { width: 12px; flex-shrink: 0; color: var(--green); }
.bd-sep { height: 1px; background: var(--border); margin: 3px 0; }
.bd-new {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 5px 12px;
  font-size: 12px;
  color: var(--text-muted);
  cursor: pointer;
  transition: background 0.08s, color 0.08s;
}
.bd-new:hover { background: var(--bg-hover); color: var(--text-primary); }
.bd-new-input { padding: 5px 10px; }
.bd-input {
  width: 100%;
  background: color-mix(in srgb, var(--border) 20%, var(--bg-panel));
  border: 1px solid var(--border);
  border-radius: 3px;
  color: var(--text-primary);
  font-family: var(--font-mono);
  font-size: 12px;
  outline: none;
  padding: 4px 7px;
  box-sizing: border-box;
}
.bd-input:focus { border-color: color-mix(in srgb, var(--accent) 50%, var(--border)); }

/* Progress */
.gp-progress {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 4px 12px;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}
.gp-progress-bar {
  position: relative;
  flex: 1;
  height: 2px;
  background: var(--border);
  overflow: hidden;
  border-radius: 2px;
}
.gp-progress-bar::after {
  content: "";
  position: absolute;
  inset-block: 0;
  left: -40%;
  width: 40%;
  background: var(--accent);
  animation: progress-slide 1.1s ease-in-out infinite;
}
@keyframes progress-slide { to { left: 100%; } }
.gp-progress-label { font-size: 11px; color: var(--text-muted); }

/* No repo */
.gp-no-repo {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 10px;
  flex: 1;
  color: var(--text-muted);
  font-size: 13px;
}
.gp-init-btn {
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 5px 12px;
  border-radius: 5px;
  border: 1px solid var(--border);
  background: var(--bg-hover);
  color: var(--text-primary);
  font-size: 12px;
  cursor: pointer;
}
.gp-init-btn:hover { background: var(--yellow); color: #000; border-color: var(--yellow); }

/* Two-column body */
.gp-body {
  flex: 1;
  display: flex;
  overflow: hidden;
}

/* LEFT column */
.gp-left {
  width: 280px;
  flex: 0 0 280px;
  border-right: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.gp-scroll {
  flex: 1;
  overflow-y: auto;
  padding: 8px 0;
}

/* Section headers */
.gp-section-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 3px 12px;
}
.gp-section-label {
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--text-muted);
  opacity: 0.7;
  display: flex;
  align-items: center;
  gap: 5px;
}
.gp-badge {
  font-size: 10px;
  font-weight: 500;
  letter-spacing: 0;
  text-transform: none;
  background: color-mix(in srgb, var(--accent) 14%, transparent);
  color: var(--accent);
  border-radius: 8px;
  padding: 0 5px;
}
.gp-sec-btn {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 3px;
  border: 1px solid var(--border);
  background: none;
  color: var(--text-muted);
  cursor: pointer;
  transition: background 0.1s, color 0.1s;
}
.gp-sec-btn:hover { background: var(--bg-hover); color: var(--text-primary); }

.gp-empty {
  font-size: 11px;
  color: var(--text-muted);
  opacity: 0.55;
  padding: 2px 12px 5px;
}

/* File rows */
.gp-file {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 3px 12px;
  cursor: pointer;
  transition: background 0.08s;
  border-radius: 0;
}
.gp-file:hover { background: var(--bg-hover); }
.gp-file.active { background: color-mix(in srgb, var(--accent) 9%, transparent); }

.gp-status {
  font-family: var(--font-mono);
  font-size: 10px;
  font-weight: 700;
  width: 12px;
  flex-shrink: 0;
  text-align: center;
}
.gp-status.staged   { color: var(--green); }
.gp-status.modified { color: var(--yellow); }
.gp-status.untracked { color: var(--text-muted); }

.gp-file-path {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 11px;
  font-family: var(--font-mono);
  color: var(--text-secondary);
  transition: color 0.08s;
}
.gp-file:hover .gp-file-path,
.gp-file.active .gp-file-path { color: var(--text-primary); }

.gp-file-btns {
  display: none;
  align-items: center;
  gap: 2px;
}
.gp-file:hover .gp-file-btns { display: flex; }

.gp-file-btn {
  background: none;
  border: none;
  cursor: pointer;
  color: var(--text-muted);
  font-size: 13px;
  line-height: 1;
  padding: 0 3px;
  flex-shrink: 0;
  display: none;
  border-radius: 2px;
  transition: color 0.08s;
}
.gp-file:hover .gp-file-btn { display: block; }
.gp-file-btn.stage:hover { color: var(--green); }
.gp-file-btn.discard:hover { color: var(--red); }

/* Commit area */
.gp-commit {
  border-top: 1px solid var(--border);
  padding: 9px 12px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  flex-shrink: 0;
  background: var(--bg-panel);
}
.gp-commit-input {
  background: color-mix(in srgb, var(--border) 15%, var(--bg-panel));
  border: 1px solid var(--border);
  border-radius: 4px;
  color: var(--text-primary);
  font-family: var(--font-ui);
  font-size: 12px;
  line-height: 1.5;
  outline: none;
  padding: 6px 8px;
  resize: none;
  width: 100%;
  box-sizing: border-box;
  transition: border-color 0.1s;
}
.gp-commit-input::placeholder { color: var(--text-muted); opacity: 0.6; }
.gp-commit-input:focus { border-color: color-mix(in srgb, var(--accent) 55%, var(--border)); }
.gp-commit-input::-webkit-scrollbar { display: none; }

.gp-type-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 3px;
}
.gp-chip {
  background: none;
  border: 1px solid var(--border);
  border-radius: 3px;
  color: var(--text-muted);
  cursor: pointer;
  font-family: var(--font-mono);
  font-size: 9.5px;
  padding: 2px 5px;
  transition: background 0.1s, color 0.1s, border-color 0.1s;
}
.gp-chip:hover {
  background: var(--bg-hover);
  color: var(--text-secondary);
}
.gp-chip.active {
  background: color-mix(in srgb, var(--accent) 14%, transparent);
  border-color: color-mix(in srgb, var(--accent) 45%, var(--border));
  color: var(--accent);
}

.gp-commit-btns {
  display: flex;
  gap: 6px;
}
.gp-commit-btn {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 4px;
  background: var(--bg-hover);
  border: 1px solid var(--border);
  border-radius: 4px;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 11px;
  font-weight: 500;
  font-family: var(--font-ui);
  padding: 5px 8px;
  transition: background 0.1s, color 0.1s;
}
.gp-commit-btn:hover:not(:disabled) {
  background: color-mix(in srgb, var(--border) 60%, var(--bg-hover));
  color: var(--text-primary);
}
.gp-commit-btn.primary {
  background: color-mix(in srgb, var(--accent) 80%, transparent);
  border-color: transparent;
  color: #fff;
}
.gp-commit-btn.primary:hover:not(:disabled) { background: var(--accent); }
.gp-commit-btn:disabled { opacity: 0.3; cursor: default; }

/* RIGHT column */
.gp-right {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  min-width: 0;
}

.gp-diff {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  min-height: 0;
}
.gp-diff-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  background: color-mix(in srgb, var(--border) 18%, var(--bg-panel));
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}
.gp-diff-title {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
}
.gp-diff-mode { font-size: 10px; color: var(--text-muted); flex-shrink: 0; }
.gp-diff-view {
  flex: 1;
  overflow: auto;
  font-family: var(--font-mono);
  font-size: 11px;
  line-height: 1.6;
  white-space: pre;
  margin: 0;
  padding: 6px 0;
}
.diff-add  { color: var(--green); display: block; padding: 0 12px; background: color-mix(in srgb, var(--green) 6%, transparent); }
.diff-del  { color: var(--red); display: block; padding: 0 12px; background: color-mix(in srgb, var(--red) 6%, transparent); }
.diff-hunk { color: var(--accent); display: block; padding: 0 12px; opacity: 0.7; }
.diff-ctx  { color: var(--text-secondary); display: block; padding: 0 12px; opacity: 0.6; }

.gp-diff-empty {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  color: var(--text-muted);
  font-size: 12px;
  opacity: 0.5;
}

/* History */
.gp-history {
  border-top: 1px solid var(--border);
  flex-shrink: 0;
  max-height: 220px;
  display: flex;
  flex-direction: column;
  background: var(--bg-panel);
}
.gp-history-header {
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 5px 12px;
  cursor: pointer;
  user-select: none;
  flex-shrink: 0;
}
.gp-history-header :deep(svg) { transition: transform 0.12s; color: var(--text-muted); }
.gp-history-header .open { transform: rotate(90deg); }
.gp-history-list { overflow-y: auto; flex: 1; }

.gp-log-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 3px 12px;
  cursor: pointer;
  transition: background 0.08s;
}
.gp-log-row:hover { background: var(--bg-hover); }
.gp-log-row.unpushed { background: color-mix(in srgb, var(--accent) 5%, transparent); }
.gp-log-row.unpushed:hover { background: color-mix(in srgb, var(--accent) 10%, transparent); }

.gp-log-hash {
  font-family: var(--font-mono);
  font-size: 10px;
  color: var(--yellow);
  flex-shrink: 0;
  min-width: 52px;
}
.gp-log-row.unpushed .gp-log-hash { color: var(--accent); }
.gp-log-up { font-size: 9px; font-weight: 700; color: var(--accent); flex-shrink: 0; }
.gp-log-subject {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 11px;
  color: var(--text-secondary);
  transition: color 0.08s;
}
.gp-log-row:hover .gp-log-subject { color: var(--text-primary); }
.gp-log-author {
  font-size: 10px;
  color: var(--text-muted);
  flex-shrink: 0;
  max-width: 100px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.gp-log-time { font-size: 10px; color: var(--text-muted); flex-shrink: 0; }
</style>
