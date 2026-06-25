---
name: burrow-release
description: Release the Burrow desktop app — update docs/changelog.html, commit it, then run `just release [patch|minor|major]` to build, sign, notarize, and publish a GitHub release. Use whenever the user says "release", "udělej release", "vydej verzi", "push release", or asks to bump the version and ship. Always invoke this skill before doing anything related to releasing Burrow.
---

# Burrow Release Skill

Full release flow: changelog → commit → `just release`.

## Step 1 — Understand what changed

Run these in parallel:
```bash
git status
git log --oneline $(git describe --tags --abbrev=0)..HEAD
git diff HEAD  # see uncommitted changes
node -p "require('./src-tauri/tauri.conf.json').version"
```

Also check `git diff` on any modified source files to understand what the uncommitted changes actually do. The goal is to describe all changes (committed since last tag + uncommitted) in the changelog.

## Step 2 — Determine release level

If the user didn't specify `patch` / `minor` / `major`, ask. Default is `patch`.
- **patch** — bug fixes, small improvements (x.y.Z)
- **minor** — new features (x.Y.0)
- **major** — breaking changes (X.0.0)

## Step 3 — Compute new version

Current version is in `src-tauri/tauri.conf.json`. Compute the bumped version yourself (don't run `just bump` yet — that happens inside `just release`). You need it to write the changelog header.

## Step 4 — Update docs/changelog.html

The changelog is Czech-language HTML. Insert a new `<div class="release">` block **after** `<div class="tagline">...</div>` and **before** the first existing `<div class="release">`.

Also: find the previous release's header and **remove** `<span class="tag tag-latest">nejnovější</span>` from it — only the newest version gets that badge.

### Section structure

```html
<!-- vX.Y.Z -->
<div class="release">
  <div class="release-header">
    <span class="version">vX.Y.Z</span>
    <span class="tag tag-latest">nejnovější</span>
    <span class="date">červen 2026</span>
  </div>

  <div class="section feat">
    <div class="section-label">Název skupiny funkce</div>
    <ul>
      <li><strong>Název funkce</strong> — popis co se změnilo a proč.</li>
    </ul>
  </div>

  <div class="section fix">
    <div class="section-label">Opravy</div>
    <ul>
      <li><strong>Root cause nebo popis opravy</strong> — detail. <code>relevantní kód</code> pokud užitečné.</li>
    </ul>
  </div>
</div>

<hr class="divider" />
```

### CSS section classes

| Class | Barva | Kdy použít |
|-------|-------|------------|
| `feat` | modrá | nové funkce |
| `fix` | zelená | opravy chyb |
| `perf` | žlutá | výkon |
| `break` | červená | breaking changes |

### Writing style

- **Czech language** throughout
- Each `<li>` starts with `<strong>název</strong> — vysvětlení`
- Use `<code>` for command names, file paths, function names
- Group related changes under one `section` with a descriptive `section-label`
- Be specific: explain the *root cause* for fixes, the *why* for features
- Look at existing entries in the file for tone and level of detail

### Month in Czech

| Month | Czech |
|-------|-------|
| January | leden |
| February | únor |
| March | březen |
| April | duben |
| May | květen |
| June | červen |
| July | červenec |
| August | srpen |
| September | září |
| October | říjen |
| November | listopad |
| December | prosinec |

Current date is in your system context — use the correct month.

## Step 5 — Commit the changelog

Working tree must be clean before `just release` runs (it checks `git diff --quiet`). Commit changelog first:

```bash
git add docs/changelog.html
git commit -m "docs(changelog): add vX.Y.Z — stručný popis"
```

The commit message should briefly summarize the main changes (1–2 key items).

## Step 6 — Run just release

```bash
just release [level]
```

This takes **10–20 minutes** (Tauri build + Apple notarization). Warn the user. The command will:
1. Bump version in `tauri.conf.json`, `package.json`, `Cargo.toml`
2. Build signed .app + .dmg
3. Notarize + staple dmg
4. Generate `latest.json` updater manifest
5. Commit version files as `"release vX.Y.Z"`
6. Tag + push to GitHub
7. Create GitHub release with dmg, updater artifacts, manifest

## Step 7 — Report

After success, tell the user:
- New version
- GitHub release URL (printed by `just release`)
- In-app updater will pick it up on next check (every 6h or manual)

## Error handling

| Error | Fix |
|-------|-----|
| `working tree dirty` | Uncommitted files exist — commit or stash them first, then try again |
| `dmg not found` | Build failed — check `pnpm tauri:build` output |
| `updater key missing` | `~/.tauri/burrow_updater.key` not found — user needs to restore it |
| Notarization failure | Usually network issue — re-run `just release` (idempotent after bump commit) |
