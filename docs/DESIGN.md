# gen — design spec

Transcribed from the Claude Design mockup (4 artboards). Mockup dates/names are examples. UI copy is
Spanish (as in the mockup); keep all user-facing strings in Spanish.

## Principles

1. Main view is a **pan/zoom canvas** (drag background or wheel to pan; ⌘+wheel / pinch to zoom
   25 %–200 %). Clicking a card makes it the focus and recenters with an ease-out animation of ~200 ms.
2. Cards are 220×64, avatar with initials (blue = male, pink = female, grey = unknown). The focused
   person has a 2 px accent border.
3. Connectors are 1.5 px cubic Béziers drawn *behind* the cards (gpui `canvas` + `PathBuilder`).
4. A missing father/mother shows as a dashed empty slot «Añadir padre» / «Añadir madre».
5. The right panel (360 px) is a **read-only** person sheet ("ficha"); editing happens in a dialog.
6. The old side list is replaced by the ⌘K palette.

## Tokens (`crates/gen-app/src/theme.rs`)

| token | light | notes |
|---|---|---|
| canvas | #EFEDE8 | canvas background |
| surface | #FFFFFF | cards, panels, dialogs |
| border | #E2DFD8 | |
| ink | #1C1B19 | primary text, dark "Editar" button |
| ink-muted | #6B675F | secondary text |
| accent | #2F6B4F | primary buttons, focus border, progress |
| accent-soft | #F4F8F5 | |
| connector | #A9A498 | |
| slot-dashed | #C9C5BB | empty-slot border, dashed connectors |
| danger | #A3382A | |
| male bg / fg | #DCE8F7 / #1E4C86 | avatar tint |
| female bg / fg | #F7E1E8 / #8A2A4A | |
| unknown bg / fg | #F3F1EC / #4A4741 | |
| highlight | #F8E7A8 | search-match highlight |
| canvas dots | #D6D2C8 | 1 px dots every 20 px |

Dark: canvas #141413, surface #1F1E1C, border #34322E, ink #F2F0EA; sex tints drop to 20 % opacity
over surface.

## Typography

- Fraunces 600 24 — name in the ficha
- Fraunces 600 16 — tree name in the top bar
- Inter 600 13 — name in a card (max 2 lines)
- Inter 500 13 — buttons and rows
- Inter 400 12 — years, places, labels

Serif only for featured names; everything else Inter. Fonts are bundled (OFL) under
`crates/gen-app/assets/fonts/`.

## Measures

- Card 220×64, radius 14, padding 12. Focus card 220×88, 2 px accent border + 4 px halo.
- Avatar 36 (card) · 80 (ficha) · 30 (rows).
- Generation step 260 (220 + 40 gap). Minimum vertical gap 24.
- Ficha panel 360 wide, padding 24. Top bar 56 tall, controls 36.

## Person card states

Normal · Hover (darker border, shadow, elevation) · Focus · Unknown («(sin nombre)», «Sin fechas»,
avatar "?") · Empty slot («+ Añadir madre», dashed; hover → accent).

## Connectors

- Cubic from the child's right edge to the parent's left edge; control points at mid-x.
- 1.5 px #A9A498; into an empty slot: dashed 4/4 #C9C5BB.
- The path from the focus to the hovered card is highlighted in accent.

## Interactions

- Drag background pans; wheel pans; ⌘+wheel (or pinch) zooms 25 %–200 %.
- Click a card → focus; tree re-lays-out and recenters, ease-out 200 ms.
- Double-click or ⌘↵ → edit dialog.
- Hover ≥300 ms → quick actions (+ padre, + pareja, + hijo).
- Arrows ←→↑↓ move between relatives; F centers the focus.
- Changing focus updates the breadcrumb (history, ← to go back).

## Artboard 1 — Tree (pedigree) + ficha

**Top bar (56):** logo (accent rounded square with tree glyph) · tree name (Fraunces 16, e.g.
«Familia Ordóñez») with subtitle «132 personas · 31 familias · guardado» (or «sin guardar») ·
segmented tabs **Pedigrí | Descendientes | Abanico | Lista** · search field «Buscar persona, lugar,
año» with ⌘K hint (opens the palette) · undo, redo icons · «Exportar GEDCOM» outline button ·
«+ Persona» accent button.

**Canvas overlays:** top-left breadcrumb pill «Juan de Dios › Miguel › Ramona» (clickable);
generations pill «Generaciones 3 4 5 6» (selected is a dark filled circle). Column headers above
each generation: «Persona en foco», «Padres», «Abuelos», «Bisabuelos», «Tatarabuelos»…
Bottom-left zoom control: − · 100% · + · fit · center. Bottom-right minimap of the laid-out tree
with the viewport rectangle.

Pedigree flows **left → right**: focus on the left, parents to the right, each column 260 apart,
each parent pair vertically centred on its child.

**Ficha (right, 360):** avatar 80 centred · name (Fraunces 24) · «1790 – 1852 · vivió 62 años» ·
chips «Mujer» (sex tint) and the id «I500125» · buttons «✎ Editar» (ink filled), «Añadir pariente»
(outline, opens a menu Padre/Madre/Pareja/Hijo/Hija), «…» (more: Ver GEDCOM, Eliminar).
Completeness box: «Ficha completa al 40 %» + «Completar» link (opens the dialog), accent progress
bar, «Faltan lugar de defunción, pareja y fuentes.»
**Línea de vida**: vertical timeline with dots: «1790 · Nacimiento / Santafé de Bogotá», «1812 ·
Matrimonio / Añadir pareja» (link when unknown), «1852 · Defunción / Lugar sin registrar» (italic,
muted). **Padres**: rows (avatar 30, name, years). **Hermanos (9)** + «Ver todos»: overlapping
avatar circles, «+4». **Hijos**: rows, or a dashed full-width button «Añadir hijo o hija».

## Artboard 2 — Edit person (dialog)

Modal ~560 wide over a dimmed backdrop. Header: avatar · «Editar a Ramona Ordóñez Rubira» ·
sub «I500125 · hija de Miguel Ordóñez Valdés» · close ×.
Tabs: **Datos · Eventos (3) · Familia · Notas y fuentes · Fotos**.

Datos: «Nombres» | «Apellidos» inputs side by side · «Sexo» segmented Hombre / Mujer / Desconocido ·
grouped card «Nacimiento»: «Precisión» select (Exacta, Aproximada, Antes de, Después de, Entre) |
«Fecha» input; helper «Se guarda como `ABT 1790`» (mono chip); «Lugar» input with autocomplete
dropdown («Santafé de Bogotá, Nueva Granada — usado 14 veces»). Grouped card «Defunción»: Precisión |
Fecha | Lugar on one row.
Footer: «Eliminar persona» (danger text) · «Ver GEDCOM» link · «⌘↵ para guardar» hint · «Cancelar» ·
«Guardar cambios» (accent).

## Artboard 3 — ⌘K search

Centred palette (~560 wide) over a dimmed backdrop. Input with search icon and «esc» hint.
Filter chips: **Todo** (active = ink filled) · Personas · Lugares · Sin fechas · Sin padres.
Sections:
- «Personas · 6»: rows with avatar 30, name with the matched part highlighted (#F8E7A8), sub-line
  «1758 – 1821 · padre de Ramona» (relationship to the focus; matched years highlighted too); the
  selected row shows a ↵ hint.
- «Lugares · 1»: pin icon, «Santafé de Bogotá», «14 eventos de la familia Ordóñez».
- «Acciones»: «+ Crear persona «ordóñez»» with ⌘N hint.
Footer: «↑↓ moverse · ↵ centrar en el árbol · ⌘↵ editar».
