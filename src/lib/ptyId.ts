// Globally-unique PTY ids. Multiple Terminal components stay mounted at once
// (one per opened workspace), so ids must not collide across instances — the
// Rust backend keys its PTY map by this id.
let counter = 0;

export function nextPtyId(): number {
  return ++counter;
}

// Call after restoring saved PTY ids so new tabs don't collide with restored ones.
export function initPtyCounter(min: number) {
  if (min > counter) counter = min;
}
