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

## Fix (footprint-neutrální counter-zoom)

Zachová opravu výběru myší z `d5ff447`, ale bez overflow. `XTerm.vue` →
`applyCounterZoom()`:

```ts
el.style.flex = "none";
el.style.zoom = String(1 / s);                  // net-zoom-1 → rect == cell metric
el.style.width  = `${parent.clientWidth  * s}px`;  // PX, ne %
el.style.height = `${parent.clientHeight * s}px`;
```

Klíč: růst boxu v **px z `parent.clientWidth/Height`**, ne `s*100%`. `%` se
počítá vůči containing blocku už nafouklému `#app`-zoom řetězcem a kumuluje se →
overflow. Px z layout šířky rodiče posadí zoomnutý footprint přesně na panel.

Voláno při mountu, při resize **rodiče** (ResizeObserver teď sleduje
`parentElement`, ne host — host si velikost řídí sám), a při změně
`effectiveScale`/fontu. `fontSize` zpět na `scaledFontSize() = terminalFontSize *
effectiveScale` (vizuální velikost při net-zoom-1). Při `scale === 1` se inline
styly vyčistí → host je zase obyčejný `flex:1`.

Ponecháno z `bf12e05` (`safeFit`/`deferredFit`) — re-fit po layoutu a fontech.

## Ověření

Matematika ověřena v reálném prohlížeči (Chromium, stejná `zoom` sémantika jako
WebKit) na scale 1.23:

| varianta | getBoundingClientRect vs offset (net-zoom) | host vs pane (overflow) |
|----------|--------------------------------------------|--------------------------|
| `%` (d5ff447) | — | host 1575px vs pane 1280px → **+295px overflow** |
| `px` (fix)    | ratio **1.000** → net-zoom-1, výběr sedí   | host 1280px == pane 1280px → **0** |

- `vue-tsc` ✓ (změněný `XTerm.vue` čistý; nesouvisející unused-`ui` v
  `AgentToolbar.vue` z jiné editace, neřešeno tady).
- Po rebuildu: terminál vyplní panel, nepřetéká; výběr myší trefí správnou buňku
  i při scale ≠ 1.

## Poznámka

Souběžně jsem nejdřív omylem řešil status tečky (kontext branche + CLAUDE.md mě
navedl tím směrem). Ty změny jsem revertoval na žádost uživatele — status funguje
dobře.
