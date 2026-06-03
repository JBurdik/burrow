# Burrow — task runner.  Install `just`:  brew install just
# Run `just` with no args to list recipes.

set shell := ["bash", "-uc"]

# Apple notarization identity (NOT secret — the app-specific password lives in
# the login Keychain item BURROW_NOTARY_PWD, never here).
export APPLE_ID      := "bc.jakubgal@email.cz"
export APPLE_TEAM_ID := "9QY36KZ8JP"

app := "src-tauri/target/release/bundle/macos/Burrow.app"
dmg := "src-tauri/target/release/bundle/dmg/Burrow_0.1.0_aarch64.dmg"
profile := "BURROW_NOTARY"

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

# Full native build — notarizes + staples the .app (password from Keychain)
build:
    APPLE_PASSWORD="$(security find-generic-password -s BURROW_NOTARY_PWD -w)" \
        pnpm tauri:build

# Build + notarize/staple the .dmg + verify — full distributable release
release: build notarize-dmg verify
    @echo "✅ release ready — distributable, Gatekeeper-clean:"
    @echo "   {{dmg}}"

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
