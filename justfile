# Burrow — task runner.  Install `just`:  brew install just
# Run `just` with no args to list recipes.

set shell := ["bash", "-uc"]

# Apple notarization identity (NOT secret — the app-specific password lives in
# the login Keychain item BURROW_NOTARY_PWD, never here).
export APPLE_ID      := "bc.jakubgal@email.cz"
export APPLE_TEAM_ID := "9QY36KZ8JP"

# Current app version, read live from tauri.conf.json (single source of truth).
version := `node -p "require('./src-tauri/tauri.conf.json').version" 2>/dev/null || echo 0.0.0`
app := "src-tauri/target/release/bundle/macos/Burrow.app"
dmg := "src-tauri/target/release/bundle/dmg/Burrow_" + version + "_aarch64.dmg"
profile := "BURROW_NOTARY"

# GitHub repo that hosts releases + the updater manifest (latest.json).
repo := "JBurdik/burrow"

# List recipes
default:
    @just --list

# ── dev ───────────────────────────────────────────────────────────────────────

# Native dev window (hot-reload)
dev:
    pnpm tauri:dev

# Frontend only, in the browser (no Tauri)
web:
    pnpm dev

# Type-check frontend + backend (no bundle)
check:
    pnpm vue-tsc --noEmit
    cd src-tauri && cargo check

# ── build / release ───────────────────────────────────────────────────────────

# Frontend production build only (vue-tsc + vite)
build-web:
    pnpm build

# Full native build — signs + notarizes/staples the .app, and signs the updater
# artifacts (.app.tar.gz) with the ed25519 key. Passwords come from the Keychain.
build:
    APPLE_PASSWORD="$(security find-generic-password -s BURROW_NOTARY_PWD -w)" \
    TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/burrow_updater.key)" \
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(security find-generic-password -s BURROW_UPDATER_PWD -w)" \
        pnpm tauri:build

# Print the current version
version:
    @echo "{{version}}"

# Bump the version across tauri.conf.json, package.json, Cargo.toml.
# Usage:  just bump            (patch: 0.2.0 → 0.2.1)
#         just bump minor      (0.2.0 → 0.3.0)
#         just bump major      (0.2.0 → 1.0.0)
# Prints the new version on stdout (consumed by `release`).
bump level="patch":
    #!/usr/bin/env bash
    set -euo pipefail
    node -e '
      const fs = require("fs");
      const lvl = process.argv[1] || "patch";
      const confPath = "src-tauri/tauri.conf.json";
      const conf = JSON.parse(fs.readFileSync(confPath, "utf8"));
      let [a, b, c] = conf.version.split(".").map(Number);
      if (lvl === "major") { a++; b = 0; c = 0; }
      else if (lvl === "minor") { b++; c = 0; }
      else { c++; }
      const v = `${a}.${b}.${c}`;
      conf.version = v;
      fs.writeFileSync(confPath, JSON.stringify(conf, null, 2) + "\n");
      const pkg = JSON.parse(fs.readFileSync("package.json", "utf8"));
      pkg.version = v;
      fs.writeFileSync("package.json", JSON.stringify(pkg, null, 2) + "\n");
      let cargo = fs.readFileSync("src-tauri/Cargo.toml", "utf8");
      cargo = cargo.replace(/^version = ".*"$/m, `version = "${v}"`);
      fs.writeFileSync("src-tauri/Cargo.toml", cargo);
      console.error(`bumped ${lvl}: ${v}`);
      process.stdout.write(v);
    ' {{level}}

# Cut a full release: bump → build (signed) → notarize dmg → generate the updater
# manifest → commit + tag + push → publish a GitHub release with the dmg + updater
# artifacts + latest.json. The updater endpoint points at this release's latest.json.
# Usage:  just release          (patch bump)
#         just release minor
release level="patch":
    #!/usr/bin/env bash
    set -euo pipefail
    command -v gh >/dev/null   || { echo "❌ gh CLI not found (brew install gh)"; exit 1; }
    [ -f ~/.tauri/burrow_updater.key ] || { echo "❌ updater key missing at ~/.tauri/burrow_updater.key"; exit 1; }
    git diff --quiet || { echo "❌ working tree dirty — commit or stash first"; exit 1; }

    NEW="$(just bump {{level}})"
    TAG="v$NEW"
    echo "▶ releasing $TAG"

    # Build signed + notarized .app + signed updater artifacts.
    APPLE_PASSWORD="$(security find-generic-password -s BURROW_NOTARY_PWD -w)" \
    TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/burrow_updater.key)" \
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(security find-generic-password -s BURROW_UPDATER_PWD -w)" \
        pnpm tauri:build

    DMG="src-tauri/target/release/bundle/dmg/Burrow_${NEW}_aarch64.dmg"
    TARGZ="src-tauri/target/release/bundle/macos/Burrow.app.tar.gz"
    [ -f "$DMG" ]   || { echo "❌ dmg not found: $DMG"; exit 1; }
    [ -f "$TARGZ" ] || { echo "❌ updater artifact not found: $TARGZ (is createUpdaterArtifacts true?)"; exit 1; }
    [ -f "$TARGZ.sig" ] || { echo "❌ signature not found: $TARGZ.sig"; exit 1; }

    # Notarize + staple the dmg (Tauri staples only the .app).
    xcrun notarytool submit "$DMG" --keychain-profile "{{profile}}" --wait
    xcrun stapler staple "$DMG"

    # Build the updater manifest. The platform URL points at this tag's assets;
    # the updater endpoint (releases/latest/download/latest.json) always serves
    # the newest release's copy of this file.
    SIG="$(cat "$TARGZ.sig")"
    PUB_DATE="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    NOTES="$(git log --pretty=format:'- %s' "$(git describe --tags --abbrev=0 2>/dev/null || git rev-list --max-parents=0 HEAD)..HEAD" 2>/dev/null | grep -v '^- release v' || echo '- Maintenance release')"
    node -e '
      const fs = require("fs");
      const [version, sig, pubDate, repo, notes] = process.argv.slice(1);
      const manifest = {
        version,
        notes,
        pub_date: pubDate,
        platforms: {
          "darwin-aarch64": {
            signature: sig,
            url: `https://github.com/${repo}/releases/download/v${version}/Burrow.app.tar.gz`,
          },
        },
      };
      fs.writeFileSync("latest.json", JSON.stringify(manifest, null, 2));
    ' "$NEW" "$SIG" "$PUB_DATE" "{{repo}}" "$NOTES"

    # Commit the version bump, tag, push.
    git add package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock
    git commit -m "release $TAG"
    git tag "$TAG"
    git push origin HEAD
    git push origin "$TAG"

    # Publish the GitHub release with installer + updater artifacts + manifest.
    gh release create "$TAG" \
        "$DMG" "$TARGZ" "$TARGZ.sig" latest.json \
        --repo "{{repo}}" --title "$TAG" --notes "$NOTES"

    rm -f latest.json
    echo "✅ released $TAG → https://github.com/{{repo}}/releases/tag/$TAG"
    echo "   in-app updater will pick it up on next check."

# One-time: create the public GitHub repo + remote, push main. Run before the
# first `just release`. Safe to re-run (skips what already exists).
gh-init:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! git remote get-url origin >/dev/null 2>&1; then
      gh repo create "{{repo}}" --public --source=. --remote=origin --push
    else
      echo "remote 'origin' already set → $(git remote get-url origin)"
    fi

# Notarize + staple the .dmg (Tauri staples only the .app, so the dmg needs this).
notarize-dmg:
    xcrun notarytool submit "{{dmg}}" --keychain-profile "{{profile}}" --wait
    xcrun stapler staple "{{dmg}}"

# ── verification ──────────────────────────────────────────────────────────────

# Full signing/notarization/Gatekeeper check on the .app and .dmg.
verify:
    #!/usr/bin/env bash
    set -uo pipefail
    APP="{{app}}"; DMG="{{dmg}}"
    hr(){ printf '\n==== %s ====\n' "$1"; }
    hr "codesign --verify (deep, strict)"
    codesign --verify --deep --strict --verbose=2 "$APP" 2>&1 | tail -2
    hr "signature + hardened runtime"
    codesign -dvvv "$APP" 2>&1 | grep -E "Authority=|TeamIdentifier=|flags=|Runtime Version" | head
    hr "nested binaries"
    for b in agentic-ide burrow-daemon; do
      printf '%-14s ' "$b"; codesign --verify --strict "$APP/Contents/MacOS/$b" && echo ok
    done
    hr "Gatekeeper — app (exec)"
    spctl -a -vvv -t exec "$APP" 2>&1 | head -3
    hr "Gatekeeper — dmg (install)"
    spctl -a -vvv -t install "$DMG" 2>&1 | head -2
    hr "staple tickets"
    xcrun stapler validate "$APP" 2>&1 | tail -1
    xcrun stapler validate "$DMG" 2>&1 | tail -1
    hr "quarantine-sim (a downloaded copy)"
    T=$(mktemp -d); cp -R "$APP" "$T/"; xattr -w com.apple.quarantine "0083;0;Safari;" "$T/Burrow.app"
    spctl -a -vvv -t exec "$T/Burrow.app" 2>&1 | head -2; rm -rf "$T"

# One-time: re-store the app-specific password in the Keychain (prompts for it).
# Pass it inline:  just notary-creds 'xxxx-xxxx-xxxx-xxxx'
notary-creds password:
    security add-generic-password -s BURROW_NOTARY_PWD -a "{{APPLE_ID}}" -w "{{password}}" -U
    xcrun notarytool store-credentials "{{profile}}" \
        --apple-id "{{APPLE_ID}}" --team-id "{{APPLE_TEAM_ID}}" --password "{{password}}"

# Show where the built artifacts are
where:
    @echo "app: {{app}}"
    @echo "dmg: {{dmg}}"
