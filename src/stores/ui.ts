import { ref, computed, watch } from "vue";
import { defineStore } from "pinia";
import { invoke } from "@tauri-apps/api/core";
import { THEMES, DEFAULT_THEME_KEY, findTheme } from "@/themes";

const PREFS_KEY = "agentic-ide.prefs";

// Font family presets. `value` is the CSS font-family stack applied.
export interface FontPreset {
  label: string;
  value: string;
}

export const UI_FONTS: FontPreset[] = [
  { label: "System Default", value: '-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif' },
  { label: "Inter", value: 'Inter, -apple-system, sans-serif' },
  { label: "Roboto", value: 'Roboto, -apple-system, sans-serif' },
  { label: "Helvetica Neue", value: '"Helvetica Neue", Helvetica, Arial, sans-serif' },
  { label: "Georgia (serif)", value: 'Georgia, "Times New Roman", serif' },
];

export const TERMINAL_FONTS: FontPreset[] = [
  { label: "JetBrains Mono", value: '"JetBrains Mono", monospace' },
  { label: "Fira Code", value: '"Fira Code", monospace' },
  { label: "Cascadia Code", value: '"Cascadia Code", monospace' },
  { label: "SF Mono", value: '"SF Mono", ui-monospace, monospace' },
  { label: "Menlo", value: 'Menlo, Monaco, monospace' },
  { label: "Courier New", value: '"Courier New", monospace' },
];

interface Prefs {
  uiFont: string;
  uiFontSize: number;
  uiScale: number;
  terminalFont: string;
  terminalFontSize: number;
  swapPanels: boolean;
  rightPanelVisible: boolean;
  theme: string;
  soundEnabled: boolean;
  soundDoneEnabled: boolean;
  soundWaitingEnabled: boolean;
  soundDoneId: string; // builtin id or "custom"
  soundDoneCustomPath: string;
  soundWaitingId: string;
  soundWaitingCustomPath: string;
  soundVolume: number; // 0-100
  maxAgents: number; // soft per-workspace sub-agent cap for the /burrow skill
  debugOverlay: boolean; // show the per-terminal diagnostic overlay (XTerm.vue)
}

// The px sizes in the stylesheets are authored at this baseline. `zoom` scales
// the whole UI relative to it, so the default uiFontSize being above the
// baseline makes the default UI render slightly larger.
const BASE_FONT_SIZE = 13;

const DEFAULT_PREFS: Prefs = {
  uiFont: UI_FONTS[0].value,
  uiFontSize: 16,
  uiScale: 1,
  terminalFont: TERMINAL_FONTS[0].value,
  terminalFontSize: 13,
  swapPanels: false,
  rightPanelVisible: true,
  theme: DEFAULT_THEME_KEY,
  soundEnabled: true,
  soundDoneEnabled: true,
  soundWaitingEnabled: true,
  soundDoneId: "soft-1",
  soundDoneCustomPath: "",
  soundWaitingId: "need-you-1",
  soundWaitingCustomPath: "",
  soundVolume: 70,
  maxAgents: 3,
  debugOverlay: false,
};

function loadPrefs(): Prefs {
  try {
    const raw = localStorage.getItem(PREFS_KEY);
    if (raw) {
      const stored = { ...DEFAULT_PREFS, ...JSON.parse(raw) };
      // Migrate installs saved below the current default up to it.
      if (stored.uiFontSize < DEFAULT_PREFS.uiFontSize) stored.uiFontSize = DEFAULT_PREFS.uiFontSize;
      return stored;
    }
  } catch {
    /* ignore */
  }
  return { ...DEFAULT_PREFS };
}

export const useUIStore = defineStore("ui", () => {
  const settingsOpen = ref(false);

  const loaded = loadPrefs();
  const uiFont = ref(loaded.uiFont);
  const uiFontSize = ref(loaded.uiFontSize);
  const uiScale = ref(loaded.uiScale);
  const terminalFont = ref(loaded.terminalFont);
  const terminalFontSize = ref(loaded.terminalFontSize);
  const swapPanels = ref(loaded.swapPanels);
  const rightPanelVisible = ref(loaded.rightPanelVisible);
  const theme = ref(loaded.theme);
  const soundEnabled = ref(loaded.soundEnabled);
  const soundDoneEnabled = ref(loaded.soundDoneEnabled);
  const soundWaitingEnabled = ref(loaded.soundWaitingEnabled);
  const soundDoneId = ref(loaded.soundDoneId);
  const soundDoneCustomPath = ref(loaded.soundDoneCustomPath);
  const soundWaitingId = ref(loaded.soundWaitingId);
  const soundWaitingCustomPath = ref(loaded.soundWaitingCustomPath);
  const soundVolume = ref(loaded.soundVolume);
  const maxAgents = ref(loaded.maxAgents);
  const debugOverlay = ref(loaded.debugOverlay);

  // Publish the soft sub-agent cap to a file the `burrow` CLI can read (it can't
  // see localStorage). No-op in browser-only dev where Tauri invoke is absent.
  watch(
    maxAgents,
    (n) => { invoke("set_max_agents", { n }).catch(() => {}); },
    { immediate: true },
  );

  // The full Theme object for the active key — consumed by xterm (XTerm.vue)
  // and the diff viewer (DiffTab.vue), which can't read CSS vars.
  const activeTheme = computed(() => findTheme(theme.value));

  // Whole-UI zoom factor applied to #app. The terminal must counter-zoom by 1/this
  // and scale its own font by this instead — CSS `zoom` on an ancestor breaks
  // xterm.js mouse-selection coordinate math (selection lands on the wrong rows).
  const effectiveScale = computed(() => uiScale.value * (uiFontSize.value / BASE_FONT_SIZE));

  // Apply the active theme's colors as CSS custom properties on :root, so all
  // chrome styled via var(--bg-base) etc. repaints. Font + layout vars are left
  // alone (they're not part of a theme).
  function applyTheme() {
    const t = findTheme(theme.value);
    const root = document.documentElement;
    for (const [k, v] of Object.entries(t.vars)) {
      root.style.setProperty(`--${k}`, v);
    }
    // Match the terminal frame/pane exactly to the xterm canvas background, so
    // there's no tonal "border" around the terminal content.
    if (t.xterm.background) root.style.setProperty("--terminal-bg", t.xterm.background);
    // Optional full-window meme wallpaper (joke themes); `none` clears it.
    root.style.setProperty("--bg-image", t.bgImage ?? "none");
    // Frosted-glass backdrop for translucent themes; else none. (No bundled theme
    // sets this — transparent/vibrancy themes were removed for causing lag.)
    root.style.setProperty("--backdrop-blur", t.backdropBlur ?? "none");
    root.style.colorScheme = t.isDark ? "dark" : "light";
  }

  // Persist + apply UI font, base font size and overall scale (zoom).
  watch(
    [uiFont, uiFontSize, uiScale, terminalFont, terminalFontSize, swapPanels, theme,
     soundEnabled, soundDoneEnabled, soundWaitingEnabled, soundDoneId, soundDoneCustomPath,
     soundWaitingId, soundWaitingCustomPath, soundVolume, rightPanelVisible, maxAgents, debugOverlay],
    () => {
      localStorage.setItem(
        PREFS_KEY,
        JSON.stringify({
          uiFont: uiFont.value,
          uiFontSize: uiFontSize.value,
          uiScale: uiScale.value,
          terminalFont: terminalFont.value,
          terminalFontSize: terminalFontSize.value,
          swapPanels: swapPanels.value,
          rightPanelVisible: rightPanelVisible.value,
          theme: theme.value,
          soundEnabled: soundEnabled.value,
          soundDoneEnabled: soundDoneEnabled.value,
          soundWaitingEnabled: soundWaitingEnabled.value,
          soundDoneId: soundDoneId.value,
          soundDoneCustomPath: soundDoneCustomPath.value,
          soundWaitingId: soundWaitingId.value,
          soundWaitingCustomPath: soundWaitingCustomPath.value,
          soundVolume: soundVolume.value,
          maxAgents: maxAgents.value,
          debugOverlay: debugOverlay.value,
        } satisfies Prefs),
      );
      applyTheme();
      document.documentElement.style.setProperty("--font-ui", uiFont.value);
      // The UI uses fixed px sizes, so the effective scale combines the explicit
      // scale with the font-size ratio (relative to the baseline). Use CSS `zoom`
      // (not `transform: scale`) so text re-rasterizes crisply at the real DPI —
      // `transform` scales a 1x bitmap and looks blurry on macOS WKWebView.
      applyAppScale();
    },
    { immediate: true },
  );

  // Counter-size #app so `zoom` lands exactly on the window. `zoom` magnifies layout,
  // so a plain 100vw box would overflow by `scale` on the right; we shrink it by
  // 1/scale first. Size in real CSS-px read from window.innerWidth/Height — NOT vw/vh
  // or %: a descendant's `zoom` leaves window.innerWidth untouched, so
  // `(innerWidth/scale)px * zoom(scale) === innerWidth` holds on every WebKit build.
  // Viewport/percentage units get re-evaluated against the *zoomed* viewport
  // inconsistently across macOS WKWebView versions, which left empty bands on the
  // right + bottom for some users. px must be recomputed on resize (vw/% wouldn't).
  function applyAppScale() {
    const scale = effectiveScale.value;
    const app = document.getElementById("app");
    if (!app) return;
    app.style.setProperty("zoom", scale === 1 ? "" : String(scale));
    app.style.width = scale === 1 ? "" : `${window.innerWidth / scale}px`;
    app.style.height = scale === 1 ? "" : `${window.innerHeight / scale}px`;
  }
  window.addEventListener("resize", applyAppScale);

  function openSettings() {
    settingsOpen.value = true;
  }
  function closeSettings() {
    settingsOpen.value = false;
  }
  function toggleSettings() {
    settingsOpen.value = !settingsOpen.value;
  }
  function toggleRightPanel() {
    rightPanelVisible.value = !rightPanelVisible.value;
  }

  function setTheme(key: string) {
    theme.value = key;
  }

  function resetFonts() {
    uiFont.value = DEFAULT_PREFS.uiFont;
    uiFontSize.value = DEFAULT_PREFS.uiFontSize;
    uiScale.value = DEFAULT_PREFS.uiScale;
    terminalFont.value = DEFAULT_PREFS.terminalFont;
    terminalFontSize.value = DEFAULT_PREFS.terminalFontSize;
  }

  return {
    settingsOpen,
    uiFont,
    uiFontSize,
    uiScale,
    effectiveScale,
    terminalFont,
    terminalFontSize,
    swapPanels,
    rightPanelVisible,
    toggleRightPanel,
    theme,
    activeTheme,
    themes: THEMES,
    setTheme,
    soundEnabled,
    soundDoneEnabled,
    soundWaitingEnabled,
    soundDoneId,
    soundDoneCustomPath,
    soundWaitingId,
    soundWaitingCustomPath,
    soundVolume,
    maxAgents,
    debugOverlay,
    openSettings,
    closeSettings,
    toggleSettings,
    resetFonts,
  };
});
