import type { Terminal, ITerminalAddon } from "@xterm/xterm";
import { WebglAddon } from "@xterm/addon-webgl";
import { CanvasAddon } from "@xterm/addon-canvas";

// Attach a GPU/2D accelerated renderer to a terminal, falling back gracefully:
// WebGL (fastest) → Canvas → xterm's default DOM renderer (no addon).
//
// The default DOM renderer is the slowest; agents like Claude stream large
// bursts of bytes, and DOM reflow is the bottleneck there. WebGL renders the
// grid on the GPU and is a big win for scroll/flood. Canvas is the 2D fallback
// for machines where the WebGL context can't be created (rare, but headless /
// software-GL setups fail). On a runtime WebGL context loss we dispose it and
// drop to Canvas so the terminal never goes blank.
//
// MUST be called AFTER term.open() — the addons need a live render layer.
// Returns the active addon so the caller can dispose it before term.dispose().
export function attachRenderer(term: Terminal): ITerminalAddon | null {
  try {
    const webgl = new WebglAddon();
    // Context loss (GPU reset, tab backgrounded too long): swap to Canvas.
    webgl.onContextLoss(() => {
      webgl.dispose();
      try {
        term.loadAddon(new CanvasAddon());
      } catch { /* fall through to DOM renderer */ }
    });
    term.loadAddon(webgl);
    return webgl;
  } catch {
    try {
      const canvas = new CanvasAddon();
      term.loadAddon(canvas);
      return canvas;
    } catch {
      return null; // DOM renderer — still works, just slower
    }
  }
}
