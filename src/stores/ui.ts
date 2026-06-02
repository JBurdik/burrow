import { ref, watch } from "vue";
import { defineStore } from "pinia";

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

  // Persist + apply UI font, base font size and overall scale (zoom).
  watch(
    [uiFont, uiFontSize, uiScale, terminalFont, terminalFontSize, swapPanels],
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
        } satisfies Prefs),
      );
      document.documentElement.style.setProperty("--font-ui", uiFont.value);
      // The UI uses fixed px sizes, so the effective scale combines the explicit
      // scale with the font-size ratio (relative to the baseline). Use CSS `zoom`
      // (not `transform: scale`) so text re-rasterizes crisply at the real DPI —
      // `transform` scales a 1x bitmap and looks blurry on macOS WKWebView.
      const scale = uiScale.value * (uiFontSize.value / BASE_FONT_SIZE);
      const app = document.getElementById("app");
      if (app) {
        // zoom magnifies layout, so shrink the box by 1/scale first — after zoom it
        // lands exactly at the window size (otherwise content overflows on the right).
        app.style.setProperty("zoom", scale === 1 ? "" : String(scale));
        app.style.width = scale === 1 ? "" : `${100 / scale}vw`;
        app.style.height = scale === 1 ? "" : `${100 / scale}vh`;
      }
    },
    { immediate: true },
  );

  function openSettings() {
    settingsOpen.value = true;
  }
  function closeSettings() {
    settingsOpen.value = false;
  }
  function toggleSettings() {
    settingsOpen.value = !settingsOpen.value;
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
    terminalFont,
    terminalFontSize,
    swapPanels,
    openSettings,
    closeSettings,
    toggleSettings,
    resetFonts,
  };
});
