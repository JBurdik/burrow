# VS Code Extension Usage Tracking — Reverse Engineering

Sourced from: `~/.vscode/extensions/anthropic.claude-code-2.1.181-darwin-arm64/extension.js` (minified bundle, ~2MB)  
and `webview/index.js`.

---

## 1. The Endpoint — Identical to Burrow

```
GET https://api.anthropic.com/api/oauth/usage
Headers:
  Authorization: Bearer <accessToken>
  anthropic-beta: oauth-2025-04-20
  Content-Type: application/json
Timeout: 5000ms
```

**Same URL Burrow already hits.** No secret sauce here.

---

## 2. Auth Token Source

The extension reads the OAuth token from:

1. **macOS Keychain** (primary) — via `security find-generic-password -a <account> -w -s <service>`. Key name derived from `CLAUDE_SECURESTORAGE_CONFIG_DIR` or a SHA-256 hash of `~/.claude`.
2. **Plaintext fallback** — `~/.claude/.credentials.json` (the `Pg` object).
3. **Windows** — Credential Manager (`VO`).

Token structure in storage:
```json
{
  "claudeAiOauth": {
    "accessToken": "...",
    "refreshToken": "...",
    "expiresAt": 1234567890,
    "scopes": ["user:inference", "user:profile", "user:sessions:claude_code", ...],
    "subscriptionType": "max" | "team" | null,
    "rateLimitTier": "..."
  }
}
```

This is the exact same file Burrow's Rust code reads (`~/.claude/.credentials.json` or Keychain on macOS). Burrow already does this correctly.

---

## 3. Auth Guard — How Org Accounts Are Detected

```js
async function ioe(authManager, logger) {
  if (authManager.getAuthStatus()?.authMethod !== "claudeai") {
    if (authManager.isAuthLoginDisabled())
      return { unavailableReason: 'Usage tracking requires Claude AI authentication...' };
    return { unavailableReason: "Usage tracking is only available for Claude AI subscribers." };
  }
  // fetch...
}
```

`authMethod` is determined by:
- `"claudeai"` — OAuth token present AND scopes include `"user:inference"` → full subscriber
- `"console"` — OAuth token present but NOT `"user:inference"` scope → console billing
- `"3p"` — `ANTHROPIC_API_KEY` / Bedrock / Vertex / Foundry env vars set
- `"api-key"` — API key in Keychain (no OAuth)

**For org/team accounts using Claude.ai OAuth**: `authMethod` is still `"claudeai"`, so the API is called. The difference is in the **API response itself** — it either returns a permission error OR the `rate_limits_available: false` field. The extension shows "Plan limits aren't available for this login method." in that case. There is no pre-call org detection.

---

## 4. Raw API Response Shape

The endpoint returns (inferred from parser + renderer):

```json
{
  "five_hour": {
    "utilization": 0.42,
    "resets_at": "2025-06-19T14:30:00Z"
  },
  "seven_day": {
    "utilization": 0.18,
    "resets_at": "2025-06-22T00:00:00Z"
  },
  "seven_day_sonnet": {
    "utilization": 0.05,
    "resets_at": "2025-06-22T00:00:00Z"
  },
  "seven_day_opus": {
    "utilization": 0.0,
    "resets_at": "2025-06-22T00:00:00Z"
  },
  "seven_day_oauth_apps": {
    "utilization": 0.0,
    "resets_at": "..."
  },
  "extra_usage": {
    "is_enabled": false,
    "monthly_limit": 100.0,
    "used_credits": 4.5,
    "utilization": 0.045
  },
  "cinder_cove": { ... },
  "subscription_type": "max",
  "rate_limits_available": true
}
```

Fields `seven_day_oauth_apps` and `cinder_cove` are **new** — not present in Burrow's current parser. `rate_limits_available` is a top-level boolean the UI uses to decide whether to render bars at all.

---

## 5. Response Parser (JS)

```js
function parseWindow(w) {
  if (!w || w.utilization === null) return undefined;
  return { utilization: w.utilization, resetsAt: w.resets_at ?? undefined };
}

function parseUsageResponse(raw) {
  let result = {};
  let fh = parseWindow(raw.five_hour);
  if (fh) result.fiveHour = fh;
  let sd = parseWindow(raw.seven_day);
  if (sd) result.sevenDay = sd;
  let ss = parseWindow(raw.seven_day_sonnet);
  if (ss) result.sevenDaySonnet = ss;
  if (raw.extra_usage) result.extraUsage = {
    isEnabled: raw.extra_usage.is_enabled,
    monthlyLimit: raw.extra_usage.monthly_limit ?? undefined,
    usedCredits: raw.extra_usage.used_credits ?? undefined,
    utilization: raw.extra_usage.utilization ?? undefined,
  };
  return result;
}
```

Note: `seven_day_opus` is rendered directly from `raw` in the webview (not parsed here). Parser is conservative — skips any window where `utilization === null`.

---

## 6. Transport: Pull Not Push

The extension does NOT poll on a timer from the extension host side. Usage is fetched **on-demand**:

1. Webview sends `{ type: "request_usage_update" }` to the extension host
2. Extension host calls `handleUsageUpdateRequest()` → `fetchUsageData()` → hits the API
3. Extension host sends back `{ type: "usage_update", utilization: ..., error: ... }` to the webview
4. Webview stores in reactive `utilization` ref

The webview triggers `requestUsageUpdate()` — likely on mount and after each completed turn. This is a **pull model**: UI asks for fresh data, extension fetches and pushes back.

There is no periodic timer in the extension host code. Burrow's current 60s `setInterval` approach is actually different.

---

## 7. UI Rendering — Sidebar Panel

Usage is shown in a **sidebar panel** (not a status bar item or title bar). Key display logic:

```js
// Which bars to show:
const bars = [
  { label: "Session (5hr)", window: rate_limits.five_hour },
  { label: "Weekly (7 day)", window: rate_limits.seven_day },
  // Only for max/team/null subscription_type:
  ...(isMaxOrTeam ? [{ label: "Weekly Sonnet", window: rate_limits.seven_day_sonnet }] : []),
].flatMap(({ label, window }) => {
  if (!window || window.utilization === null) return [];
  return [{ label, utilization: window.utilization, resetsAt: window.resets_at }];
});
```

Each bar renders:
- Label ("Session (5hr)")
- Percentage number (`Math.floor(utilization * 100) + "%"`)
- Fill bar (red tint at ≥80%)
- Reset time ("resets in 2h 15m")

**State machine** for the whole panel:
- `loading` → spinner
- `rate_limits_available === false` AND `behaviors` exists → "Plan limits aren't available for this login method."
- `rate_limits_available === false` AND no `behaviors` → "Usage tracking is only available for Claude AI subscribers."
- `IIt(rate_limits)` is false (all windows absent/null) → retry button ("Failed to load usage data")
- Otherwise → render bars

`IIt` checks: `"five_hour" in e || "seven_day" in e || "seven_day_sonnet" in e || "seven_day_opus" in e || "seven_day_oauth_apps" in e || "extra_usage" in e || "cinder_cove" in e`

Org accounts with `rate_limits_available: false` but with `behaviors` data (plan behaviors/perks) still show the behaviors section below — just no usage bars.

---

## 8. Second Path: Per-Session SDK Query

There's a second, distinct usage call:
```js
await channel.query.usage_EXPERIMENTAL_MAY_CHANGE_DO_NOT_RELY_ON_THIS_API_YET()
```
This talks to the **running Claude process** (via IPC channel) and returns per-session context usage — not plan-level quota. Completely separate from the OAuth endpoint. Used for `get_usage` requests scoped to an active session channel. Burrow doesn't have this.

---

## 9. Gaps / Unknowns

- **`cinder_cove`** — unknown field; present in type check `IIt` but not parsed/rendered visibly. Possibly a new plan or feature gate.
- **`seven_day_oauth_apps`** — new field; present in `IIt` check but not yet rendered in the bars array. Possibly for API/OAuth app usage separate from interactive sessions.
- **`rate_limits_available`** — must be a top-level API response field. Not explicitly parsed in `jHe` but accessed directly on the raw response in the webview. Burrow doesn't currently read this field.
- **Polling trigger timing** — the webview triggers `requestUsageUpdate()` but exact trigger points (mount? per turn? on focus?) weren't isolated.
- **`extra_usage`** — pay-per-use credits; rendered somewhere in the panel but not found in the bar list; likely a separate credit display below the rate-limit bars.

---

## 10. What Burrow Should Adopt

### A. Add `rate_limits_available` handling

The API response already includes this field. Burrow should read it and show "unavailable for this account type" instead of an error or empty state. This would **auto-detect org accounts** without needing the manual `orgAccount` checkbox.

```rust
// In claude_plan_usage, after parsing:
if let Some(false) = usage.get("rate_limits_available").and_then(|v| v.as_bool()) {
    return json!({ "ok": false, "error": "permission_error", 
                   "message": "Plan limits unavailable for this account" });
}
```

Then `permission_error` is already in `LOCAL_FALLBACK_ERRORS` — auto-fallback to JSONL scan. No user checkbox needed.

### B. Add `seven_day_opus` and `seven_day_oauth_apps` bars

Burrow already shows `seven_day_sonnet` and `seven_day_opus` conditionally. Add `seven_day_oauth_apps` to the bar list.

### C. Add `extra_usage` credit display

For pay-per-use overflow: show `used_credits` / `monthly_limit` as a credit meter when `extra_usage.is_enabled === true`.

### D. Keep the `orgAccount` flag as an escape hatch

Even with auto-detect via `rate_limits_available`, the manual flag is useful for accounts where the API call itself fails vs returns `rate_limits_available: false`. Keep it but make it secondary.

### E. Subscription-type-aware bar visibility

Extension hides "Weekly Sonnet" for non-max/non-team plans. Burrow already does `hideZero: true` which achieves the same effect in practice. No change needed.

---

## Summary Table

| Aspect | VS Code Extension | Burrow (current) | Gap |
|--------|-------------------|------------------|-----|
| Endpoint | `/api/oauth/usage` | Same | None |
| Auth | Keychain → `~/.claude/.credentials.json` | Same | None |
| Org detection | Via `rate_limits_available: false` in response | Manual checkbox | Should read this field |
| Bar fields | 5h, 7d, 7d-sonnet, 7d-opus, 7d-oauth-apps, extra_usage, cinder_cove | 5h, 7d, 7d-sonnet, 7d-opus | Missing oauth_apps + extra_usage |
| Polling | Pull on-demand (webview requests) | 60s timer | Different model; both fine |
| Display location | Sidebar panel | Title bar strip | Different; title bar is more compact |
| Local fallback | None (shows "unavailable") | JSONL scan | Burrow better for org accounts |
