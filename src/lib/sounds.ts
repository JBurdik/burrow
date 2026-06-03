import { invoke } from "@tauri-apps/api/core";
import { useUIStore } from "@/stores/ui";

// Bundled notification sounds. Vite turns each import into a served URL string.
import soft1 from "@/assets/sounds/soft-1.wav";
import soft2 from "@/assets/sounds/soft-2.wav";
import soft3 from "@/assets/sounds/soft-3.wav";
import whisper1 from "@/assets/sounds/whisper-1.wav";
import whisper2 from "@/assets/sounds/whisper-2.wav";
import voice1 from "@/assets/sounds/voice-1.wav";

export interface BuiltinSound {
  id: string;
  label: string;
  url: string;
}

export const BUILTIN_SOUNDS: BuiltinSound[] = [
  { id: "soft-1", label: "Soft 1", url: soft1 },
  { id: "soft-2", label: "Soft 2", url: soft2 },
  { id: "soft-3", label: "Soft 3", url: soft3 },
  { id: "whisper-1", label: "Whisper 1", url: whisper1 },
  { id: "whisper-2", label: "Whisper 2", url: whisper2 },
  { id: "voice-1", label: "Voice 1", url: voice1 },
];

export type SoundKind = "done" | "waiting";

// Cache object URLs for custom files keyed by their disk path, so we read the
// bytes off disk once per path instead of on every play.
const customUrlCache = new Map<string, string>();

async function customUrl(path: string): Promise<string | null> {
  if (!path) return null;
  const cached = customUrlCache.get(path);
  if (cached) return cached;
  try {
    // Agent subprocesses can't reach arbitrary paths via the fs plugin scope, so
    // read bytes through our own Tauri command (base64) instead.
    const b64 = await invoke<string>("read_file_base64", { path });
    const bin = atob(b64);
    const bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    const url = URL.createObjectURL(new Blob([bytes]));
    customUrlCache.set(path, url);
    return url;
  } catch {
    return null;
  }
}

function builtinUrl(id: string): string | null {
  return BUILTIN_SOUNDS.find((s) => s.id === id)?.url ?? null;
}

// Resolve the configured sound for a kind to a playable URL (built-in or custom).
async function resolveUrl(kind: SoundKind): Promise<string | null> {
  const ui = useUIStore();
  const id = kind === "done" ? ui.soundDoneId : ui.soundWaitingId;
  const path = kind === "done" ? ui.soundDoneCustomPath : ui.soundWaitingCustomPath;
  if (id === "custom") return customUrl(path);
  return builtinUrl(id);
}

// Reuse one element per kind so rapid repeats don't pile up overlapping audio.
const players = new Map<SoundKind, HTMLAudioElement>();

async function playUrl(kind: SoundKind, url: string, volume: number) {
  let el = players.get(kind);
  if (!el) {
    el = new Audio();
    players.set(kind, el);
  }
  if (el.src !== url) el.src = url;
  el.volume = Math.max(0, Math.min(1, volume / 100));
  try {
    el.currentTime = 0;
    await el.play();
  } catch {
    /* autoplay / lifecycle errors — ignore */
  }
}

/**
 * Play the notification sound for `kind`, honouring the user's enable + volume
 * prefs. `force` (used by the Settings "Test" button) bypasses the enable gates.
 */
export async function playSound(kind: SoundKind, force = false): Promise<void> {
  const ui = useUIStore();
  if (!force) {
    if (!ui.soundEnabled) return;
    if (kind === "done" && !ui.soundDoneEnabled) return;
    if (kind === "waiting" && !ui.soundWaitingEnabled) return;
  }
  const url = await resolveUrl(kind);
  if (!url) return;
  await playUrl(kind, url, ui.soundVolume);
}
