# Report: terminál přetéká přes okno (špatná šířka/výška)

**Datum:** 2026-06-03
**Branch:** fix/agent-status-and-delegation
**Soubor:** `src/components/XTerm.vue`

## Příznak

Terminál měl špatnou šířku i výšku — byl větší než jeho panel a přetékal přes
okraj okna aplikace, takže část terminálu nebyla vidět. Po chvíli se "rozbilo",
psaní/redraw to dočasně srovnalo. Stav přetrval i u nových i obnovených terminálů.
Včera (build z `044fc28`) problém nebyl.

## Příčina

Commit `d5ff447` ("improvements to ui", dnes 12:31) přidal do `XTerm.vue`
`hostStyle` — counter-zoom hostu terminálu:

```ts
const hostStyle = computed(() => {
  const s = ui.effectiveScale;
  if (s === 1) return {};
  return { flex: "none", zoom: String(1 / s), width: `${s * 100}%`, height: `${s * 100}%` };
});
```

Záměr: celá UI je zvětšená přes CSS `zoom` na `#app`. To rozbíjelo výběr myší v
xtermu (xterm měří buňku z `offsetHeight` = layout px, ale souřadnice myši z
`getBoundingClientRect` = vizuální/zoomnuté px → výběr o řádek níž). Fix
counter-zoomoval host zpět na net-zoom-1 a dorostl box na `scale*100%`, aby pořád
vyplnil panel.

Problém: tento přístup spoléhá na konkrétní chování `zoom` v layout modelu WebKitu
(že `zoom:1/s` na boxu `width/height: s*100%` dá footprint přesně 100 %). V
produkčním WKWebView to nevyšlo — host vyšel větší než panel a terminál přetekl
přes okno.

Aktivovalo se to jen při `effectiveScale ≠ 1`. Uživatel měl
`uiFontSize:16, uiScale:1`, `BASE_FONT_SIZE = 13` →
**effectiveScale = 16/13 ≈ 1.23**, takže `hostStyle` byl aktivní → overflow.
(Při default scale=1 vracel `{}` a nic by se neprojevilo — proto to nebylo vidět
hned.)

Před `d5ff447` byl host obyčejný `flex:1` zoomovaný spolu s `#app` jako každý jiný
panel → žádný overflow ("včera fungovalo").

## Fix

Revert `d5ff447` části v `XTerm.vue` — terminál zase jede na `#app` zoomu jako
zbytek UI:

- odebrán `hostStyle` (a `:style` binding), `scaledFontSize`
- `fontSize` zpět na `ui.terminalFontSize` (zoom řeší `#app`)
- watch sleduje jen `terminalFont`/`terminalFontSize`, ne `effectiveScale`

Ponecháno z `bf12e05` (`safeFit` + `deferredFit` + `ResizeObserver`) — korektní
re-fit po layoutu a načtení fontu je v pořádku a pomáhá proti overflow po restartu.

**Tradeoff:** vrací se drobný posun výběru myší o řádek při scale ≠ 1 (původní
důvod `d5ff447`). Overflow měl prioritu — dělal terminál nepoužitelný. Výběr lze
vyřešit jinak (footprint-neutrálně) v samostatném kroku, pokud bude vadit.

## Ověření

- `vue-tsc` ✓ (změněný `XTerm.vue` čistý; nesouvisející unused-`ui` error v
  `AgentToolbar.vue` je z jiné rozdělané editace, neřešeno tady).
- Po rebuildu: terminál vyplní panel, nepřetéká přes okno; nová i obnovená záložka
  ve správné velikosti.

## Poznámka

Souběžně jsem nejdřív omylem řešil status tečky (kontext branche + CLAUDE.md mě
navedl tím směrem). Ty změny jsem revertoval na žádost uživatele — status funguje
dobře.
