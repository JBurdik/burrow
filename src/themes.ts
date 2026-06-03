import type { ITheme } from "@xterm/xterm";

// A Theme restyles the whole app: chrome (CSS vars), the terminal (xterm
// palette) and the diff viewer (shiki/@pierre theme name) together. The CSS var
// keys mirror the color set authored in App.vue's `:root` (font + layout vars
// are intentionally NOT part of a theme).
export interface Theme {
  key: string;
  label: string;
  isDark: boolean;
  // CSS custom-property values, keyed WITHOUT the leading `--`.
  vars: {
    "bg-base": string;
    "bg-panel": string;
    "bg-hover": string;
    "bg-selected": string;
    border: string;
    "text-primary": string;
    "text-secondary": string;
    "text-muted": string;
    accent: string;
    "accent-dim": string;
    green: string;
    yellow: string;
    red: string;
  };
  xterm: ITheme;
  // A @pierre/diffs theme name (accepts any shiki BundledTheme).
  shiki: string;
  // Optional full-window background. A CSS `background` value (url(), gradient,
  // etc.) painted on <body> behind the chrome. Panels with rgba()/transparent
  // bg vars let it peek through. Used by joke/meme themes; omit for normal ones.
  bgImage?: string;
}

// Inline meme wallpaper for the "Stonks" theme: tiled neon emoji + ALL-CAPS
// hype text on a deep purple void, baked as a data-URI SVG so it ships offline
// (no network fetch in the desktop app). Crazy on purpose.
const STONKS_BG =
  "url(\"data:image/svg+xml," +
  encodeURIComponent(
    `<svg xmlns='http://www.w3.org/2000/svg' width='340' height='240'>
      <defs>
        <linearGradient id='g' x1='0' y1='0' x2='1' y2='1'>
          <stop offset='0' stop-color='#16001f'/>
          <stop offset='1' stop-color='#04000a'/>
        </linearGradient>
      </defs>
      <rect width='340' height='240' fill='url(#g)'/>
      <text x='14' y='52' font-size='40' opacity='0.10'>🚀</text>
      <text x='250' y='44' font-size='40' opacity='0.10'>💎</text>
      <text x='150' y='150' font-size='44' opacity='0.10'>🐕</text>
      <text x='40' y='200' font-size='40' opacity='0.10'>🔥</text>
      <text x='270' y='210' font-size='40' opacity='0.10'>💸</text>
      <text x='80' y='110' font-family='Impact, sans-serif' font-size='26' fill='#39ff14' opacity='0.12' transform='rotate(-12 80 110)'>STONKS</text>
      <text x='180' y='90' font-family='Impact, sans-serif' font-size='20' fill='#ff2079' opacity='0.12' transform='rotate(8 180 90)'>MUCH WOW</text>
      <text x='30' y='230' font-family='Impact, sans-serif' font-size='18' fill='#00f0ff' opacity='0.12'>TO THE MOON</text>
    </svg>`,
  ) +
  "\")";

export const THEMES: Theme[] = [
  {
    key: "dark",
    label: "Dark",
    isDark: true,
    vars: {
      "bg-base": "#0d0d0d",
      "bg-panel": "#111111",
      "bg-hover": "#1a1a1a",
      "bg-selected": "#1e3a5f",
      border: "#3a3a3a",
      "text-primary": "#f1f5f9",
      "text-secondary": "#aab6c5",
      "text-muted": "#8b97a8",
      accent: "#3b82f6",
      "accent-dim": "#1d4ed8",
      green: "#22c55e",
      yellow: "#eab308",
      red: "#ef4444",
    },
    xterm: {
      background: "#0a0a0a",
      foreground: "#e2e8f0",
      cursor: "#3b82f6",
      cursorAccent: "#0a0a0a",
      selectionBackground: "#1e3a5f",
      black: "#1e293b",
      red: "#ef4444",
      green: "#22c55e",
      yellow: "#eab308",
      blue: "#3b82f6",
      magenta: "#a855f7",
      cyan: "#06b6d4",
      white: "#cbd5e1",
      brightBlack: "#475569",
      brightRed: "#f87171",
      brightGreen: "#4ade80",
      brightYellow: "#fbbf24",
      brightBlue: "#60a5fa",
      brightMagenta: "#c084fc",
      brightCyan: "#22d3ee",
      brightWhite: "#f1f5f9",
    },
    shiki: "github-dark",
  },
  {
    key: "light",
    label: "Light",
    isDark: false,
    vars: {
      "bg-base": "#ffffff",
      "bg-panel": "#f5f5f5",
      "bg-hover": "#e8e8ec",
      "bg-selected": "#cfe3ff",
      border: "#c2c2cc",
      "text-primary": "#0f172a",
      "text-secondary": "#3a4759",
      "text-muted": "#64748b",
      accent: "#2563eb",
      "accent-dim": "#1d4ed8",
      green: "#16a34a",
      yellow: "#ca8a04",
      red: "#dc2626",
    },
    xterm: {
      background: "#ffffff",
      foreground: "#1e293b",
      cursor: "#2563eb",
      cursorAccent: "#ffffff",
      selectionBackground: "#cfe3ff",
      black: "#1e293b",
      red: "#dc2626",
      green: "#16a34a",
      yellow: "#ca8a04",
      blue: "#2563eb",
      magenta: "#9333ea",
      cyan: "#0891b2",
      white: "#64748b",
      brightBlack: "#475569",
      brightRed: "#ef4444",
      brightGreen: "#22c55e",
      brightYellow: "#eab308",
      brightBlue: "#3b82f6",
      brightMagenta: "#a855f7",
      brightCyan: "#06b6d4",
      brightWhite: "#1e293b",
    },
    shiki: "github-light",
  },
  {
    key: "monokai",
    label: "Monokai",
    isDark: true,
    vars: {
      "bg-base": "#1e1f1c",
      "bg-panel": "#272822",
      "bg-hover": "#3e3d32",
      "bg-selected": "#49483e",
      border: "#4d4b3e",
      "text-primary": "#f8f8f2",
      "text-secondary": "#d3d2c6",
      "text-muted": "#9d9883",
      accent: "#fd971f",
      "accent-dim": "#cc7a16",
      green: "#a6e22e",
      yellow: "#e6db74",
      red: "#f92672",
    },
    xterm: {
      background: "#272822",
      foreground: "#f8f8f2",
      cursor: "#fd971f",
      cursorAccent: "#272822",
      selectionBackground: "#49483e",
      black: "#272822",
      red: "#f92672",
      green: "#a6e22e",
      yellow: "#f4bf75",
      blue: "#66d9ef",
      magenta: "#ae81ff",
      cyan: "#a1efe4",
      white: "#f8f8f2",
      brightBlack: "#75715e",
      brightRed: "#f92672",
      brightGreen: "#a6e22e",
      brightYellow: "#e6db74",
      brightBlue: "#66d9ef",
      brightMagenta: "#ae81ff",
      brightCyan: "#a1efe4",
      brightWhite: "#f9f8f5",
    },
    shiki: "monokai",
  },
  {
    key: "cobalt2",
    label: "Cobalt2",
    isDark: true,
    vars: {
      "bg-base": "#193549",
      "bg-panel": "#15232d",
      "bg-hover": "#1f4662",
      "bg-selected": "#0d3a58",
      border: "#2d6088",
      "text-primary": "#ffffff",
      "text-secondary": "#bcd3e0",
      "text-muted": "#8aa6bb",
      accent: "#ffc600",
      "accent-dim": "#cc9e00",
      green: "#3ad900",
      yellow: "#ffc600",
      red: "#ff628c",
    },
    xterm: {
      background: "#193549",
      foreground: "#ffffff",
      cursor: "#ffc600",
      cursorAccent: "#193549",
      selectionBackground: "#0d3a58",
      black: "#000000",
      red: "#ff628c",
      green: "#3ad900",
      yellow: "#ffc600",
      blue: "#0088ff",
      magenta: "#ff9d00",
      cyan: "#80fcff",
      white: "#ffffff",
      brightBlack: "#6e8ba0",
      brightRed: "#ff628c",
      brightGreen: "#3ad900",
      brightYellow: "#ffc600",
      brightBlue: "#0088ff",
      brightMagenta: "#ff9d00",
      brightCyan: "#80fcff",
      brightWhite: "#ffffff",
    },
    shiki: "github-dark",
  },
  {
    key: "solarized-light",
    label: "Solarized Light",
    isDark: false,
    vars: {
      "bg-base": "#fdf6e3",
      "bg-panel": "#eee8d5",
      "bg-hover": "#e4ddc8",
      "bg-selected": "#d7e7e9",
      border: "#c7bd9f",
      "text-primary": "#073642",
      "text-secondary": "#3f5358",
      "text-muted": "#7c8a8a",
      accent: "#268bd2",
      "accent-dim": "#1f6f9f",
      green: "#859900",
      yellow: "#b58900",
      red: "#dc322f",
    },
    xterm: {
      background: "#fdf6e3",
      foreground: "#586e75",
      cursor: "#268bd2",
      cursorAccent: "#fdf6e3",
      selectionBackground: "#d7e7e9",
      black: "#073642",
      red: "#dc322f",
      green: "#859900",
      yellow: "#b58900",
      blue: "#268bd2",
      magenta: "#d33682",
      cyan: "#2aa198",
      white: "#eee8d5",
      brightBlack: "#586e75",
      brightRed: "#cb4b16",
      brightGreen: "#586e75",
      brightYellow: "#657b83",
      brightBlue: "#839496",
      brightMagenta: "#6c71c4",
      brightCyan: "#93a1a1",
      brightWhite: "#fdf6e3",
    },
    shiki: "solarized-light",
  },
  {
    key: "dracula",
    label: "Dracula",
    isDark: true,
    vars: {
      "bg-base": "#282a36",
      "bg-panel": "#21222c",
      "bg-hover": "#343746",
      "bg-selected": "#44475a",
      border: "#4a4d63",
      "text-primary": "#f8f8f2",
      "text-secondary": "#d2d4e0",
      "text-muted": "#8b94bb",
      accent: "#bd93f9",
      "accent-dim": "#9a73d4",
      green: "#50fa7b",
      yellow: "#f1fa8c",
      red: "#ff5555",
    },
    xterm: {
      background: "#282a36",
      foreground: "#f8f8f2",
      cursor: "#bd93f9",
      cursorAccent: "#282a36",
      selectionBackground: "#44475a",
      black: "#21222c",
      red: "#ff5555",
      green: "#50fa7b",
      yellow: "#f1fa8c",
      blue: "#bd93f9",
      magenta: "#ff79c6",
      cyan: "#8be9fd",
      white: "#f8f8f2",
      brightBlack: "#6272a4",
      brightRed: "#ff6e6e",
      brightGreen: "#69ff94",
      brightYellow: "#ffffa5",
      brightBlue: "#d6acff",
      brightMagenta: "#ff92df",
      brightCyan: "#a4ffff",
      brightWhite: "#ffffff",
    },
    shiki: "dracula",
  },
  {
    key: "stonks",
    label: "Stonks 🚀 (meme)",
    isDark: true,
    // Panel bgs are rgba with alpha so the meme wallpaper bleeds through every
    // surface. Accents are eye-melting neon — magenta/cyan/acid-green.
    vars: {
      "bg-base": "#0a0012",
      "bg-panel": "rgba(22, 0, 38, 0.62)",
      "bg-hover": "rgba(255, 32, 121, 0.18)",
      "bg-selected": "rgba(0, 240, 255, 0.22)",
      border: "#ff2079",
      "text-primary": "#f5e9ff",
      "text-secondary": "#c9a8ff",
      "text-muted": "#8a6bb0",
      accent: "#39ff14",
      "accent-dim": "#00f0ff",
      green: "#39ff14",
      yellow: "#ffe600",
      red: "#ff2079",
    },
    xterm: {
      background: "#0a0012",
      foreground: "#f5e9ff",
      cursor: "#39ff14",
      cursorAccent: "#0a0012",
      selectionBackground: "#3a006a",
      black: "#1a0030",
      red: "#ff2079",
      green: "#39ff14",
      yellow: "#ffe600",
      blue: "#00f0ff",
      magenta: "#d400ff",
      cyan: "#00f0ff",
      white: "#f5e9ff",
      brightBlack: "#6b4a8f",
      brightRed: "#ff5c9d",
      brightGreen: "#7dff5c",
      brightYellow: "#fff04d",
      brightBlue: "#5cf0ff",
      brightMagenta: "#e85cff",
      brightCyan: "#5cf0ff",
      brightWhite: "#ffffff",
    },
    shiki: "synthwave-84",
    bgImage: STONKS_BG,
  },
];

export const DEFAULT_THEME_KEY = "dark";

export function findTheme(key: string): Theme {
  return THEMES.find((t) => t.key === key) ?? THEMES[0];
}
