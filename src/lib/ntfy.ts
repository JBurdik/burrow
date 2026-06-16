// ntfy.sh integration — publish push notifications when an agent transition
// fires. Uses the JSON publish API (POST to the server base URL with a JSON
// body), so non-ASCII titles/messages need no header encoding. ntfy serves
// `Access-Control-Allow-Origin: *`, so the webview `fetch` works directly with
// no Tauri HTTP plugin or Rust round-trip.
//
// Docs: https://docs.ntfy.sh/publish/#publish-as-json

import type { NtfyEvent } from "@/stores/ui";

export interface NtfyConfig {
  server: string; // base URL, e.g. https://ntfy.sh
  topic: string;
  token?: string; // optional Bearer access token for protected topics
}

// Per-event presentation: ntfy priority (1–5) + emoji tags shown in the push.
const EVENT_META: Record<NtfyEvent, { priority: number; tags: string[]; title: string }> = {
  done: { priority: 3, tags: ["white_check_mark"], title: "Task complete" },
  waiting: { priority: 4, tags: ["speech_balloon"], title: "Waiting for input" },
  permission: { priority: 4, tags: ["lock"], title: "Permission needed" },
  error: { priority: 5, tags: ["rotating_light"], title: "Turn failed" },
};

function publish(cfg: NtfyConfig, body: Record<string, unknown>): Promise<void> {
  const server = (cfg.server || "https://ntfy.sh").replace(/\/+$/, "");
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (cfg.token) headers["Authorization"] = `Bearer ${cfg.token}`;
  return fetch(server, {
    method: "POST",
    headers,
    body: JSON.stringify({ topic: cfg.topic, ...body }),
  }).then((r) => {
    if (!r.ok) throw new Error(`ntfy responded ${r.status}`);
  });
}

/** Fire a notification for an agent transition. Resolves on success, rejects on error. */
export function notifyNtfy(
  cfg: NtfyConfig,
  event: NtfyEvent,
  message: string,
): Promise<void> {
  const meta = EVENT_META[event];
  return publish(cfg, {
    title: `Burrow · ${meta.title}`,
    message: message || meta.title,
    priority: meta.priority,
    tags: meta.tags,
  });
}

/** Send a test notification, used by the Settings "Send test" button. */
export function testNtfy(cfg: NtfyConfig): Promise<void> {
  return publish(cfg, {
    title: "Burrow · Test",
    message: "ntfy integration is wired up correctly 🎉",
    priority: 3,
    tags: ["tada"],
  });
}
