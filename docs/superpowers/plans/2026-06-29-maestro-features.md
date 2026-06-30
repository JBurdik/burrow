# Maestro Features (prevent_sleep, draft auto-save, session browser) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add three quality-of-life features inspired by Maestro: prevent Mac sleep while agent is busy, auto-save chat input drafts, and a session browser to resume prior Claude sessions.

**Architecture:**
- **prevent_sleep**: Rust `#[tauri::command] fn set_sleep_inhibit(active: bool)` calling `IOPMAssertionCreateWithName` via `libc` FFI (already a dep); watcher in `App.vue` fires whenever any tab/chat flips busy.
- **draft auto-save**: `localStorage` key `burrow.draft.<chatId>` read on mount + written on `inputText` change in `ClaudeChat.vue` and cleared on send; same for `ManagerBar.vue`.
- **session browser**: new Tauri command `list_claude_sessions(cwd, config_dir)` scans `~/.claude/projects/<slug>/*.jsonl`, returns `[{sessionId, firstMessage, updatedAt}]`; modal in `ClaudeChat.vue` (reuse existing `acpHistoryOpen` pattern) lets user pick → passes to `resumeSessionId`.

**Tech Stack:** Rust/Tauri 2, Vue 3 + Pinia, `libc` crate (already in Cargo.toml), `localStorage`.

## Global Constraints

- macOS only for prevent_sleep (IOKit); guard with `#[cfg(target_os = "macos")]`.
- No new npm dependencies.
- No new Rust crates (libc already present).
- Follow existing code style: camelCase Tauri commands on JS side, snake_case on Rust side.
- No test suite exists; manual testing instructions replace unit tests.

---

### Task 1: prevent_sleep — Rust command

**Files:**
- Modify: `src-tauri/src/lib.rs` — add `set_sleep_inhibit` command + IOKit FFI

**Interfaces:**
- Produces: `set_sleep_inhibit(active: bool) -> Result<(), String>` Tauri command
- `invoke("set_sleep_inhibit", { active: true/false })`

- [ ] **Step 1: Add IOKit FFI declarations in lib.rs**

Find the macOS-specific imports section (around line 3469, where `cocoa` + `objc` are used) and add the following **above** the first `#[tauri::command]` at the top of the file, after existing `use` statements:

```rust
#[cfg(target_os = "macos")]
mod sleep_inhibit {
    use std::sync::Mutex;
    use libc::{c_char, c_uint};

    // IOKit types (not exposed by any crate we already have — declare manually)
    type IOPMAssertionID = u32;
    type IOReturn = c_uint;
    const kIOReturnSuccess: IOReturn = 0;

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOPMAssertionCreateWithName(
            assertion_type: *const c_char,
            assertion_level: u32,
            assertion_name: *const c_char,
            assertion_id: *mut IOPMAssertionID,
        ) -> IOReturn;
        fn IOPMAssertionRelease(assertion_id: IOPMAssertionID) -> IOReturn;
    }

    // kIOPMAssertionLevelOn = 255
    const kIOPMAssertionLevelOn: u32 = 255;

    static ASSERTION_ID: Mutex<Option<IOPMAssertionID>> = Mutex::new(None);

    pub fn set(active: bool) -> Result<(), String> {
        let mut guard = ASSERTION_ID.lock().unwrap();
        if active {
            if guard.is_some() { return Ok(()); } // already held
            let assertion_type = c"PreventUserIdleSystemSleep\0";
            let assertion_name = c"Burrow agent running\0";
            let mut id: IOPMAssertionID = 0;
            let ret = unsafe {
                IOPMAssertionCreateWithName(
                    assertion_type.as_ptr(),
                    kIOPMAssertionLevelOn,
                    assertion_name.as_ptr(),
                    &mut id,
                )
            };
            if ret == kIOReturnSuccess {
                *guard = Some(id);
                Ok(())
            } else {
                Err(format!("IOPMAssertionCreateWithName failed: {ret}"))
            }
        } else {
            if let Some(id) = guard.take() {
                unsafe { IOPMAssertionRelease(id) };
            }
            Ok(())
        }
    }
}
```

- [ ] **Step 2: Add the Tauri command**

Add after the `sleep_inhibit` mod, near the other small utility commands (around line 3670):

```rust
#[tauri::command]
fn set_sleep_inhibit(active: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    { sleep_inhibit::set(active) }
    #[cfg(not(target_os = "macos"))]
    { Ok(()) }
}
```

- [ ] **Step 3: Register in invoke_handler**

In the `tauri::generate_handler![...]` block (line ~4488), add `set_sleep_inhibit,` alongside the other commands.

- [ ] **Step 4: Build to verify**

```bash
cd src-tauri && cargo check 2>&1 | grep -E "^error"
```
Expected: no errors. Fix any compile errors before continuing.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(sleep): add set_sleep_inhibit Tauri command via IOKit"
```

---

### Task 2: prevent_sleep — Frontend watcher

**Files:**
- Modify: `src/App.vue` — add watcher that calls `set_sleep_inhibit` based on any busy tab/chat

**Interfaces:**
- Consumes: `invoke("set_sleep_inhibit", { active: bool })` from Task 1
- Consumes: `tabsStore.tabsByWs` (all tab summaries with `.busy`)
- Consumes: `chatsStore.sessions` from `useClaudeChatsStore` if available (check if imported in App.vue)

- [ ] **Step 1: Check existing imports in App.vue**

```bash
head -100 src/App.vue | grep "import"
```

Note which stores are already imported. You need `useTerminalTabsStore`. If not imported, add it.

- [ ] **Step 2: Add the watcher in App.vue**

Find the block of `watch(...)` calls (around line 176) and add after the last one:

```typescript
// Prevent Mac sleep while any agent tab is busy
watch(
  () => {
    const allTabs = Object.values(tabsStore.tabsByWs).flat();
    return allTabs.some((t) => t.busy);
  },
  (anyBusy) => {
    import("@tauri-apps/api/core")
      .then(({ invoke }) => invoke("set_sleep_inhibit", { active: anyBusy }))
      .catch(() => {});
  },
);
```

Make sure `tabsStore` is in scope — add near other store inits at the top of `<script setup>`:

```typescript
const tabsStore = useTerminalTabsStore();
```

(Only add if not already present — check first.)

- [ ] **Step 3: Manual test**

Run `pnpm tauri:dev`. Start a Claude chat. While it's running (orange dot visible), open Terminal → run `pmset -g assertions | grep Burrow`. Should see `"Burrow agent running"` assertion. When done, assertion should disappear.

- [ ] **Step 4: Commit**

```bash
git add src/App.vue
git commit -m "feat(sleep): prevent Mac sleep while any agent tab is busy"
```

---

### Task 3: Draft auto-save in ClaudeChat

**Files:**
- Modify: `src/components/ClaudeChat.vue` — persist `inputText` to localStorage per chatId, restore on mount, clear on send

**Interfaces:**
- Key format: `burrow.draft.chat.<chatId>`

- [ ] **Step 1: Add draft key constant and restore on mount**

Find where `inputText` is defined (line ~1039):
```typescript
const inputText = ref("");
```

Replace with:
```typescript
const DRAFT_KEY = computed(() => `burrow.draft.chat.${props.chatId}`);
const inputText = ref(localStorage.getItem(DRAFT_KEY.value) ?? "");
```

- [ ] **Step 2: Watch inputText and save to localStorage**

Find the area around line 1039 and add a watcher right after `inputText` definition:

```typescript
watch(inputText, (val) => {
  if (val) {
    localStorage.setItem(DRAFT_KEY.value, val);
  } else {
    localStorage.removeItem(DRAFT_KEY.value);
  }
});
```

- [ ] **Step 3: Clear draft on send**

Find `sendMessage()` — where `inputText.value = ""` is called (lines ~1742, 1749). The watch above will fire on those clears and call `removeItem` — no extra code needed. Verify by checking that both clear sites set `inputText.value = ""` (they do per grep output).

- [ ] **Step 4: Manual test**

Run `pnpm tauri:dev`. Open a Claude chat, type a message but DON'T send. Switch to another tab. Switch back. Verify text is still there. Send the message. Verify field is empty and stays empty after tab switch.

- [ ] **Step 5: Commit**

```bash
git add src/components/ClaudeChat.vue
git commit -m "feat(draft): auto-save chat input draft per session in localStorage"
```

---

### Task 4: Draft auto-save in ManagerBar

**Files:**
- Modify: `src/components/ManagerBar.vue` — same pattern as Task 3 but for Manager's chat input

**Interfaces:**
- Key format: `burrow.draft.manager.<workspaceId>` (Manager is per-workspace)
- Need to find: how ManagerBar gets its chatId or workspace id, and where inputText lives

- [ ] **Step 1: Find ManagerBar's input field**

```bash
grep -n "inputText\|v-model.*input\|textarea\|chatId\|props\." src/components/ManagerBar.vue | head -30
```

Note the variable name for the input and the prop/id to use as key.

- [ ] **Step 2: Apply same draft pattern**

Using what you found in Step 1, apply the same `DRAFT_KEY` + `ref(localStorage.getItem(...))` + `watch` pattern. Use `burrow.draft.manager.<id>` where `<id>` is the workspace/chat identifier available as a prop.

Example (adjust variable names to match what you found):
```typescript
const DRAFT_KEY = computed(() => `burrow.draft.manager.${props.workspaceId ?? props.chatId}`);
const inputText = ref(localStorage.getItem(DRAFT_KEY.value) ?? "");

watch(inputText, (val) => {
  if (val) localStorage.setItem(DRAFT_KEY.value, val);
  else localStorage.removeItem(DRAFT_KEY.value);
});
```

Clear on send: verify `inputText.value = ""` is called in the send function (same as ClaudeChat).

- [ ] **Step 3: Manual test**

Open Manager (float chat). Type something, close/reopen Manager. Text persists. Send → clears.

- [ ] **Step 4: Commit**

```bash
git add src/components/ManagerBar.vue
git commit -m "feat(draft): auto-save Manager chat draft in localStorage"
```

---

### Task 5: Session browser — Rust command

**Files:**
- Modify: `src-tauri/src/lib.rs` — add `list_claude_sessions` command that scans `~/.claude/projects/<slug>/*.jsonl`

**Interfaces:**
- Produces: `list_claude_sessions(cwd: String, config_dir: Option<String>) -> Vec<ClaudeSessionInfo>`
- `ClaudeSessionInfo { session_id: String, first_message: String, updated_at: String }`
- `invoke("list_claude_sessions", { cwd, configDir })` returns array sorted by `updated_at` desc

- [ ] **Step 1: Add the struct and command**

Find where `TranscriptEntry` is defined (around line 3760) and add nearby:

```rust
#[derive(serde::Serialize, Clone)]
struct ClaudeSessionInfo {
    session_id: String,
    first_message: String,
    updated_at: String, // ISO 8601
}

#[tauri::command]
fn list_claude_sessions(
    app: AppHandle,
    cwd: String,
    config_dir: Option<String>,
) -> Vec<ClaudeSessionInfo> {
    use std::io::{BufRead, BufReader};

    // Resolve projects dir (same logic as claude_usage_5h)
    let projects_dir = if let Some(cd) = config_dir.as_deref().filter(|s| !s.is_empty()) {
        let expanded = if cd.starts_with("~/") {
            app.path().home_dir().ok()
                .map(|h| format!("{}{}", h.display(), &cd[1..]))
                .unwrap_or_else(|| cd.to_string())
        } else { cd.to_string() };
        let base = std::path::Path::new(&expanded);
        let direct = base.join("projects");
        if direct.is_dir() { direct } else { base.join(".claude/projects") }
    } else {
        match app.path().home_dir() {
            Ok(h) => h.join(".claude/projects"),
            Err(_) => return vec![],
        }
    };

    // Derive the project slug from cwd (same convention Claude uses)
    let slug = cwd.replace(['/', '\\', ':'], "-").trim_start_matches('-').to_string();
    let project_dir = projects_dir.join(&slug);
    if !project_dir.is_dir() { return vec![]; }

    let Ok(entries) = std::fs::read_dir(&project_dir) else { return vec![]; };
    let mut results: Vec<ClaudeSessionInfo> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("jsonl") { return None; }
            let session_id = path.file_stem()?.to_str()?.to_string();
            // mtime as updated_at
            let updated_at = path.metadata().ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| {
                    // Simple ISO8601 from unix seconds — no chrono needed
                    let secs = d.as_secs();
                    let s = secs % 60; let m = (secs / 60) % 60;
                    let h = (secs / 3600) % 24; let days = secs / 86400;
                    // Approximate date calculation (good enough for display)
                    format!("{days}d {h:02}:{m:02}:{s:02}") // rough display
                })
                .unwrap_or_default();
            // Read first user message
            let f = std::fs::File::open(&path).ok()?;
            let first_message = BufReader::new(f).lines().flatten()
                .find_map(|line| {
                    let v: serde_json::Value = serde_json::from_str(&line).ok()?;
                    if v["role"].as_str() == Some("user") {
                        let content = &v["content"];
                        if let Some(arr) = content.as_array() {
                            return arr.iter().find_map(|c| {
                                if c["type"].as_str() == Some("text") {
                                    c["text"].as_str().map(|t| t.chars().take(80).collect::<String>())
                                } else { None }
                            });
                        }
                        if let Some(s) = content.as_str() {
                            return Some(s.chars().take(80).collect());
                        }
                    }
                    None
                })
                .unwrap_or_else(|| "(empty)".to_string());
            Some(ClaudeSessionInfo { session_id, first_message, updated_at })
        })
        .collect();

    // Sort by mtime desc — re-read metadata for sorting
    results.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    results.truncate(20); // top 20 recent sessions
    results
}
```

- [ ] **Step 2: Register in invoke_handler**

Add `list_claude_sessions,` to the `tauri::generate_handler![...]` block.

- [ ] **Step 3: Build check**

```bash
cd src-tauri && cargo check 2>&1 | grep -E "^error"
```
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(sessions): add list_claude_sessions Tauri command"
```

---

### Task 6: Session browser — UI modal in ClaudeChat

**Files:**
- Modify: `src/components/ClaudeChat.vue` — add "Browse sessions" button near the existing session dropdown, show modal list, on pick call `resumeAcpSession`

**Interfaces:**
- Consumes: `invoke<ClaudeSessionInfo[]>("list_claude_sessions", { cwd: props.cwd, configDir: props.configDir })` from Task 5
- Consumes: `resumeAcpSession(sid)` already exists at line ~804
- `ClaudeSessionInfo` matches Task 5's struct

- [ ] **Step 1: Add types + state**

Find the `AcpSessionInfo` interface (~line 702) and add:

```typescript
interface ClaudeSessionInfo {
  session_id: string;
  first_message: string;
  updated_at: string;
}
```

After `acpHistoryOpen` ref (~line 789 area), add:

```typescript
const sessionBrowserOpen = ref(false);
const sessionBrowserItems = ref<ClaudeSessionInfo[]>([]);
const sessionBrowserLoading = ref(false);

async function openSessionBrowser() {
  sessionBrowserOpen.value = true;
  sessionBrowserLoading.value = true;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    sessionBrowserItems.value = await invoke<ClaudeSessionInfo[]>("list_claude_sessions", {
      cwd: props.cwd,
      configDir: props.configDir ?? null,
    });
  } catch (e) {
    sessionBrowserItems.value = [];
  } finally {
    sessionBrowserLoading.value = false;
  }
}

async function pickSession(sid: string) {
  sessionBrowserOpen.value = false;
  await resumeAcpSession(sid);
}
```

- [ ] **Step 2: Add the button in the template**

Find the session history dropdown button area in the template (around line 11 where `acpHistoryOpen` is toggled) and add a "Browse" button next to it:

```html
<button
  v-if="effectiveTransport === 'acp'"
  class="session-browse-btn"
  title="Browse past sessions"
  @click="openSessionBrowser"
>⏱</button>
```

- [ ] **Step 3: Add the modal**

Find the end of the main template div and add before the closing tag of `.claude-chat`:

```html
<!-- Session browser modal -->
<div v-if="sessionBrowserOpen" class="session-browser-overlay" @click.self="sessionBrowserOpen = false">
  <div class="session-browser-modal">
    <div class="session-browser-header">
      <span>Recent sessions</span>
      <button @click="sessionBrowserOpen = false">✕</button>
    </div>
    <div v-if="sessionBrowserLoading" class="session-browser-empty">Loading…</div>
    <div v-else-if="!sessionBrowserItems.length" class="session-browser-empty">No sessions found for this project.</div>
    <div
      v-for="s in sessionBrowserItems"
      :key="s.session_id"
      class="session-browser-item"
      @click="pickSession(s.session_id)"
    >
      <span class="session-preview">{{ s.first_message }}</span>
      <span class="session-meta">{{ s.updated_at }} · {{ s.session_id.slice(0, 8) }}</span>
    </div>
  </div>
</div>
```

- [ ] **Step 4: Add minimal styles**

In the `<style scoped>` section at the bottom of `ClaudeChat.vue`, add:

```css
.session-browse-btn {
  background: none;
  border: none;
  cursor: pointer;
  opacity: 0.6;
  font-size: 14px;
  padding: 2px 4px;
}
.session-browse-btn:hover { opacity: 1; }

.session-browser-overlay {
  position: absolute;
  inset: 0;
  background: rgba(0,0,0,0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 200;
}
.session-browser-modal {
  background: var(--bg2, #1e1e1e);
  border: 1px solid var(--border, #333);
  border-radius: 8px;
  width: 420px;
  max-height: 60vh;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
}
.session-browser-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 10px 14px;
  border-bottom: 1px solid var(--border, #333);
  font-weight: 600;
}
.session-browser-header button {
  background: none;
  border: none;
  cursor: pointer;
  color: var(--fg, #ccc);
}
.session-browser-item {
  padding: 10px 14px;
  cursor: pointer;
  border-bottom: 1px solid var(--border-faint, #2a2a2a);
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.session-browser-item:hover { background: var(--bg3, #252525); }
.session-preview { font-size: 13px; color: var(--fg, #ccc); }
.session-meta { font-size: 11px; color: var(--fg-muted, #666); }
.session-browser-empty { padding: 20px 14px; color: var(--fg-muted, #666); }
```

- [ ] **Step 5: Manual test**

Run `pnpm tauri:dev`. Open a Claude chat tab. Click the ⏱ button. Modal should list recent sessions for the project. Click one → chat resumes that session (existing resume logic handles it). Click outside modal → closes.

- [ ] **Step 6: Commit**

```bash
git add src/components/ClaudeChat.vue
git commit -m "feat(sessions): add session browser modal to Claude chat"
```

---

## Self-Review

**Spec coverage:**
- ✅ prevent_sleep via IOKit — Task 1+2
- ✅ Draft auto-save per chatId — Task 3+4
- ✅ Session browser from `~/.claude/projects/` — Task 5+6

**Placeholder scan:** None — all steps have concrete code.

**Type consistency:** `ClaudeSessionInfo` defined in Task 5 (Rust) and Task 6 (TS) match field names. `resumeAcpSession(sid: string)` exists at line ~804.

**Known simplification (ponytail):**
- `updated_at` in `list_claude_sessions` uses a rough "Nd HH:MM:SS" format instead of full ISO8601. Good enough for display; if full ISO needed, add `chrono` crate.
- Session browser only shows sessions for the current project's slug (not global). By design — user is in a project context.
