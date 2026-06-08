import type { Terminal, ITerminalAddon } from "@xterm/xterm";
import { WebglAddon } from "@xterm/addon-webgl";
import { CanvasAddon } from "@xterm/addon-canvas";

// Attach a GPU/2D accelerated renderer to a terminal, falling back gracefully:
// Canvas (default) → DOM, or WebGL → Canvas → DOM when opted in.
//
// The default DOM renderer is the slowest; agents like Claude stream large
// bursts of bytes, and DOM reflow is the bottleneck there. Both Canvas and WebGL
// rasterize the grid off the DOM and are a big win for scroll/flood.
//
// We default to CANVAS, not WebGL. WebGL rasterizes glyphs into a GPU texture
// atlas and mishandles COMBINING DIACRITICAL MARKS (Czech háčky/čárky) and
// reverse-video/selected cells — it drops them or picks a fallback font, so the
// text "looks like a different font" exactly where accents or selection appear.
// Canvas renders combining marks + selection correctly via the 2D context font
// stack while still being far faster than the DOM renderer. WebGL stays opt-in
// for users who hit a heavy-flood bottleneck and don't type accented text.
//
// Opt into WebGL with localStorage `burrow.renderer = "webgl"`.
//
// MUST be called AFTER term.open() — the addons need a live render layer.
// Returns the active addon so the caller can dispose it before term.dispose().
export function attachRenderer(term: Terminal): ITerminalAddon | null {
  const prefersWebgl = (() => {
    try { return localStorage.getItem("burrow.renderer") === "webgl"; }
    catch { return false; }
  })();

  if (prefersWebgl) {
    try {
      const webgl = new WebglAddon();
      // Context loss (GPU reset, tab backgrounded too long): swap to Canvas.
      webgl.onContextLoss(() => {
        webgl.dispose();
        try { term.loadAddon(new CanvasAddon()); }
        catch { /* fall through to DOM renderer */ }
      });
      term.loadAddon(webgl);
      return webgl;
    } catch { /* fall through to Canvas */ }
  }

  try {
    const canvas = new CanvasAddon();
    term.loadAddon(canvas);
    return canvas;
  } catch {
    return null; // DOM renderer — still works, just slower
  }
}
