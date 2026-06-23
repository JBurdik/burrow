# Burrow Per-Project Config (`.burrow/`)

**Date:** 2026-06-23  
**Status:** Approved

## Goal

Each workspace/repo in Burrow gets a `.burrow/` folder with per-project config: Manager system prompt, scripts, and local Burrow skill injection. Replaces global-only scripts and hardcoded Manager primer.

---

## Disk Layout

```
<repo>/
  .burrow/
    manager.md        # Manager append-system-prompt (editable, scaffolded from default)
    config.toml       # Scripts + project metadata
  .claude/
    skills/
      burrow/
        SKILL.md      # Per-project Burrow skill (global ~/.claude/skills/ kept as fallback)
```

`.burrow/` should be added to `.gitignore` if private, or committed if team-shared — user's choice. Burrow never auto-adds it to `.gitignore`.

---

## `config.toml` Schema

```toml
[project]
name = "My App"   # optional display name override for the workspace

[[scripts]]
name = "dev"
command = "pnpm dev"

[[scripts]]
name = "build"
command = "pnpm build"
```

---

## Behavior Changes

### Scaffold on workspace open
When a workspace is opened/created, Burrow checks for `.burrow/`. If missing, scaffolds:
- `manager.md` — copy of the current hardcoded Manager primer (`MC_PRIMER` content from `FloatChat.vue`)
- `config.toml` — minimal template with empty scripts array

Scaffold is non-destructive: never overwrites existing files.

### Manager system prompt
`managerPrimer` computed in `FloatChat.vue` becomes async: reads `.burrow/manager.md` via a new `read_text_file` Tauri command, appends to the hardcoded primer base. If file missing or unreadable, falls back to hardcoded primer only.

### Scripts
`scripts.ts` store reads from `.burrow/config.toml` for the active workspace instead of global `localStorage`. On save, writes back to `.burrow/config.toml`. Global scripts in settings are removed.

### Burrow skill injection
`install_agent_docs` in `lib.rs` continues writing `BURROW_SKILL_MD` to `~/.claude/skills/burrow/SKILL.md` (global fallback preserved). Additionally, on workspace create/open, writes `BURROW_SKILL_MD` to `<workspace>/.claude/skills/burrow/SKILL.md` (per-project). Claude Code loads both; per-project takes precedence.

---

## UI

### Project config button
New gear icon button in `ManagerBar.vue`, placed after the existing Claude UI button.

Opens a modal (`WorkspaceConfig.vue`) with two tabs:

**Tab: Manager Prompt**
- Textarea displaying `.burrow/manager.md` content
- Save button → `write_text_file(<workspace>/.burrow/manager.md, content)`
- "Reset to default" → overwrites with hardcoded primer

**Tab: Scripts**
- List editor (name + command rows, add/remove)
- Save → serializes to TOML, writes `.burrow/config.toml`

---

## Tauri Commands

| Command | Purpose |
|---------|---------|
| `read_text_file(path)` | New — read any file, returns `String` or error |
| `write_text_file(path, content)` | Already exists |
| `scaffold_burrow_dir(workspace_path)` | New — creates `.burrow/` with defaults if absent; also writes per-project skill |

`read_text_file` is the only truly new Tauri command. `scaffold_burrow_dir` can be a thin wrapper calling existing FS primitives.

---

## Files Changed

| File | Change |
|------|--------|
| `src-tauri/src/lib.rs` | Add `read_text_file`, `scaffold_burrow_dir`; update `install_agent_docs` to also write per-project skill |
| `src/stores/scripts.ts` | Read/write from `.burrow/config.toml` instead of localStorage |
| `src/components/FloatChat.vue` | `managerPrimer` reads `.burrow/manager.md`, appends to hardcoded base |
| `src/components/ManagerBar.vue` | Add gear button |
| `src/components/WorkspaceConfig.vue` | New modal with Manager Prompt + Scripts tabs |
| `src/components/Terminal.vue` | Call `scaffold_burrow_dir` on workspace open |

---

## Out of Scope

- TOML parser in Rust: use `toml` crate (already common in Rust ecosystem) or parse minimally with serde.
- Per-project agent command override (e.g. use `aider` for this repo) — defer to later.
- Auto-add `.burrow/` to `.gitignore` — user decides.
