// Match a KeyboardEvent against a human-written shortcut string like "⌘⇧1".
// Modifiers: ⌘ meta, ⌥ alt, ⌃ ctrl, ⇧ shift. The trailing char is the key.
// Digits are matched via e.code (Digit1…) so Shift/Option key remapping on
// macOS (Shift+1 → "!", Option+1 → "¡") doesn't break the binding.
const MODS = /[⌘⌥⌃⇧]/g;

export function matchesShortcut(e: KeyboardEvent, sc: string | undefined): boolean {
  if (!sc) return false;
  const key = sc.replace(MODS, "").trim();
  if (!key) return false;
  if (e.metaKey !== sc.includes("⌘")) return false;
  if (e.altKey !== sc.includes("⌥")) return false;
  if (e.ctrlKey !== sc.includes("⌃")) return false;
  if (e.shiftKey !== sc.includes("⇧")) return false;
  if (/^[0-9]$/.test(key)) return e.code === `Digit${key}`;
  return e.key.toLowerCase() === key.toLowerCase();
}
