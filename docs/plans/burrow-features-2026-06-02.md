# Burrow Feature Plan — 2026-06-02

## Context

Six improvements to Burrow's terminal tab UX, keyboard navigation, project management, and git integration.

---

## 1. Bug: Status Dots Incorrect

**Root cause:** Two competing status sources race each other:
- Hook state polling (500ms) via temp file in `XTerm.vue:111-121`
- Foreground process polling (2s) in `XTerm.vue:187-216`

When hook fires "done", 4-second idle timer starts (`Terminal.vue:299`). If foreground poll fires within that window with a matching process name, it can re-set `busy=true` and corrupt state.

**Fix:**
- In `XTerm.vue`, when `hookStateActive=true`, skip the foreground polling fallback entirely — hook takes priority.
- Guard: only fall back to foreground polling if no hook state file exists (`hookStateFilePath` empty or file absent).

**Files:** `src/components/XTerm.vue` (lines 187-216), `src/components/Terminal.vue` (lines 288-341)

---

## 2. Reorder Terminal Tabs

**Approach:** HTML5 drag-and-drop on `.ws-term` items in sidebar.

**Changes:**
1. `src/components/Sidebar.vue` — add `draggable="true"` + `@dragstart`/`@dragover`/`@drop` handlers on `.ws-term` divs; track drag source index in local ref
2. `src/stores/terminalTabs.ts` — add `reorder(wsId, fromIdx, toIdx)` that splices array in-place
3. `src/components/Terminal.vue` — watch for reorder requests, call `save_terminal_tabs` to persist new order to SQLite

**Files:** `Sidebar.vue`, `terminalTabs.ts`, `Terminal.vue`

---

## 3. Ctrl+1-9: Switch Terminal Tabs

**Approach:** Add Ctrl+digit handler in `Terminal.vue` keydown listener (lines 461-475).

```typescript
} else if (e.ctrlKey && !e.metaKey && !e.altKey && /^[1-9]$/.test(e.key)) {
  e.preventDefault();
  const idx = parseInt(e.key) - 1;
  const tabs = termTabs.tabsByWs[props.workspaceId] ?? [];
  if (tabs[idx]) activateTab(tabs[idx].id);
}
```

Only fires when the workspace is active (existing guard at line 461).

**Files:** `src/components/Terminal.vue`

---

## 4. Cmd+1-9: Switch Workspace/Project

**Note:** `⌘1-⌘5` are currently documented in cheatsheet as agent-launch shortcuts but **never wired** (agents store defines them, no handler exists). Repurpose these as workspace switchers; remove the phantom agent shortcuts from cheatsheet.

**Approach:** Add to `App.vue` `onKeydown` (lines 193-214):

```typescript
} else if (/^[1-9]$/.test(e.key)) {
  e.preventDefault();
  const idx = parseInt(e.key) - 1;
  const ws = wsStore.workspaces[idx];
  if (ws) wsStore.open(ws);
}
```

Also update `CHEATSHEET_GROUPS` to replace `⌘1-5 agent` entries with `⌘1-9 Switch project`.

**Files:** `src/App.vue`

---

## 5. Project Icons (Pick from Disk)

**Current state:** `workspace.ts` already has `setIcon(id, dataUrl)` + `icons` ref (localStorage). Sidebar.vue renders icon if present (lines 11-35).

**Missing:** No UI to pick an image + no Tauri command to read binary file as base64.

**Changes:**
1. `src-tauri/src/lib.rs` — add `read_file_base64(path: String) -> Result<String>` Tauri command (reads file bytes, returns base64-encoded string)
2. `src/components/Sidebar.vue` — on icon area click (or right-click context menu option "Change icon"), open Tauri `open` dialog filtered to `["png","jpg","jpeg","svg","ico"]`, call `read_file_base64`, prepend `data:image/...;base64,`, call `wsStore.setIcon(id, dataUrl)`

**Files:** `src-tauri/src/lib.rs`, `src/components/Sidebar.vue`, `src/stores/workspace.ts` (already ready)

---

## 6. Show Git Branch in TitleBar / Sidebar

**Current state:** `git.ts` has `branch` ref, populated via `git branch --show-current`. TitleBar.vue shows workspace name via prop.

**Approach:** Show branch in TitleBar next to project name.

1. `src/components/TitleBar.vue` — accept `branch` prop (string), render as `<span class="branch-name"><PhGitBranch :size="11" /> {{ branch }}</span>` next to project name
2. `src/components/WorkspaceScreen.vue` (or wherever TitleBar is used) — pass `gitStore.branch` as prop
3. `src/stores/git.ts` — ensure `refresh()` is called when active workspace changes (watch `wsStore.active`)

**Files:** `src/components/TitleBar.vue`, `src/components/WorkspaceScreen.vue`, `src/stores/git.ts`

---

## Verification

1. Run `pnpm tauri:dev`
2. Status dots: start Claude in tab, verify orange→blue→green→idle sequence with no false flickers
3. Tab reorder: drag tab in sidebar, verify new order persists after workspace close/reopen
4. Ctrl+1-3: open 3 tabs, press Ctrl+1/2/3, verify correct tab activates
5. Cmd+1-2: open 2 workspaces, press ⌘1/⌘2, verify workspace switches
6. Icons: right-click workspace, pick PNG, verify icon appears in sidebar
7. Branch: open a git repo workspace, verify branch name shows in title bar

---

## Delegation

Each feature is independent — delegate to parallel Burrow sub-agents:
- Agent A: features 1+3 (XTerm status bug + Ctrl shortcuts) — same file area
- Agent B: feature 2 (tab reorder)
- Agent C: feature 4 (Cmd+1-9 workspace switch)
- Agent D: features 5+6 (icons + git branch) — both touch Sidebar + need new Tauri command
