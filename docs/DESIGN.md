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
| warning | #A86A12 | review warnings (dark #D9A441) |
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
- Arrows move between relatives (canvas focused):
  - Pedigrí: → father (mother if no father) · ← back to the child you came from (else the first
    child, i.e. the top card of the «Hijos» column) · ↑/↓ switch between the two parents of that
    child; otherwise previous/next sibling.
  - Descendientes: → first child · ← a parent · ↑/↓ previous/next sibling.
- F centers the focus; ⌘= / ⌘− / ⌘0 zoom in / out / 100 %; ⌘9 fits the whole tree.
- `+` (or `=`) / `−` (canvas focused) show two generations more / fewer, like the pill stepper
  (bare keys because ⌘⇧= is ⌘+ = zoom on most layouts).
- Changing focus updates the breadcrumb (history; ⌘[ or ⌫ goes back, a crumb click jumps there).
- The recenter after a click waits ~250 ms so a double-click still lands on the same card.
- Quick actions show «+ padre» (or «+ madre» if only the father is known, nothing if both are),
  «+ pareja», «+ hijo»; they create the relative and open the dialog, the focus stays put.

## Artboard 1 — Tree (pedigree) + ficha

**Top bar (56):** logo (accent rounded square with tree glyph) · tree name (Fraunces 16, e.g.
«Familia Ordóñez») with subtitle «132 personas · 31 familias · guardado» (or «sin guardar») ·
segmented tabs **Pedigrí | Descendientes | Abanico | Lista | Mapa** · search field «Buscar persona, lugar,
año» with ⌘K hint (opens the palette) · undo, redo icons · «Exportar GEDCOM» outline button ·
«+ Persona» accent button.

**Canvas overlays:** top-left breadcrumb pill «Juan de Dios › Miguel › Ramona» (clickable);
generations pill «Generaciones − 6 +»: a stepper from 2 to 20 in steps of 2 (default 4), round 28 px
− / + buttons dimmed and inert at the ends, with tooltips; the value is remembered in the settings
file. Pedigrí only expands known people (empty «add» slots only next to them) and is capped at
`MAX_NODES` cards, as is Descendientes; Abanico draws at most 8 rings and, when the stepper asks for
more, says so in a muted line under the fan («El abanico muestra hasta 8 generaciones»).
Column headers above each generation: «Hijos» (Pedigrí only, see below), «Persona en foco», «Padres»,
«Abuelos», «Bisabuelos», «Tatarabuelos»…
Bottom-left zoom control: − · 100% · + · fit · center. Bottom-right minimap of the laid-out tree
with the viewport rectangle.

Pedigree flows **left → right**: focus on the left, parents to the right, each column 260 apart,
each parent pair vertically centred on its child. One column to the **left** of the focus
(x = −260) lists the focus's children (all unions, in order) as normal cards stacked with the
minimum gap and centred on the focus; their connectors run from the child's right edge to the
focus's left edge. Without children it shows one dashed «+ Añadir hijo» slot.

**Ficha (right, 360):** avatar 80 centred · name (Fraunces 24) · «1790 – 1852 · vivió 62 años» ·
chips «Mujer» (sex tint) and the id «I500125» · buttons «✎ Editar» (ink filled), «Añadir pariente»
(outline, opens a menu Padre/Madre/Pareja/Hijo/Hija), «…» (more: Ver GEDCOM, Unir con otra persona…,
Eliminar). The three
share one row: 36 tall, radius 10, Inter 500 13, gap 8, centres aligned; the two menu triggers are
the app's own outline buttons (`ui::menu_button` / `ui::menu_icon_button`, border token, surface
background, 14 px chevron), not gpui-component's `Button`.
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

Keys (context `Palette`, bound by `palette::bind_keys`): ↑↓ move one selection across all rows
(people, places, actions; wraps) · ↵ focus person / place / create · ⌘↵ edit the selected person ·
⌘N create «query» · Tab / ⇧Tab cycle the filter chips · ⌘1–⌘5 pick a chip directly · esc or backdrop
click dismiss. Hovering a row selects it; clicking activates it (⌘-click = edit).

Edit dialog keys (context `EditDialog`, `edit_dialog::bind_keys`): ⌘↵ saves from any field (the note
included) · esc closes an open place/precision dropdown first, then cancels · ↑↓/↵ drive the place
autocomplete.

## Merge duplicates

Ficha «…» → «Unir con otra persona…» opens the palette in picker mode: title «Unir a X con…», no
chips, a «Posibles duplicados» section (`Tree::duplicate_candidates`: similar folded names, same or
unknown sex, years within 2) above the normal search; people that can't be merged (self, partners,
parents, children) are hidden. ↵ / click picks. Then a ~640-wide confirmation modal shows both
people side by side (avatar, name, years, id, parent/partner/child counts); the kept one has the
accent border. Keys (context `MergeDialog`, `merge_dialog::bind_keys`): ⌘↵ merges · Tab or the ⇄
button swaps who is kept · esc / backdrop cancels. The merge is one undo step; afterwards the kept
person is focused and a toast «Personas unidas · Deshacer» offers undo (⌘Z works too).
Merge rules (pointer rewrite, alternate names, event folding, dropped CHAN/_UPD/RIN) are documented
in `crates/gedcom/src/merge.rs`.

## Relationship calculator

Ficha «…» → «Calcular parentesco con…» (or the palette action «Calcular parentesco de X…», offered
under «Acciones» when the query is empty or matches its keywords) opens the palette in picker mode:
title «Parentesco de X con…», no chips, people only, sub-lines relative to X. ↵ / click picks. A
~600-wide modal then shows «X es primo segundo de Y» (Fraunces 22), the common ancestor(s), the
kind and generation counts («Consanguíneo · 3 ↑ · 3 ↓ generaciones»), other relationships through
pedigree collapse («También: primo tercero (por otra rama)»), and the path as a Λ of mini cards
(230 wide, avatar 30, name, years): the common ancestor on top with the accent border, X's line up
on the left, Y's line down on the right, each step labelled («↑ madre», «↓ hija», ♥ «esposa»).
Clicking a card closes the modal and focuses that person. Keys (context `KinshipDialog`,
`kinship_dialog::bind_keys`): Tab or the ⇄ button flips the direction · esc / backdrop closes.
Naming rules (lowest common ancestors over FAMC, half/double, «N generaciones de diferencia»,
one-marriage in-laws: «suegro», «yerno», «cuñado», «padrastro», «hijastro», «tía política»…) are in
`crates/gedcom/src/kinship.rs`.

## Revisión (data-quality review)

`Tree::quality_issues()` (`crates/gedcom/src/quality.rs`) scans the whole tree and returns
`Issue { kind, severity, people, family, … }` sorted by severity (Error · Warning · Info), then kind;
`people[0]` is who the message is about, `message()` / `kind.label()` are localized at render time.
Checks: death before birth, burial before death, age at death > 110, born > 110 years ago with no
death record («posiblemente fallecido», info); child born before a parent, father < 13 / mother < 12
or father > 80 / mother > 55 at the birth, born > 1 year after the father's death or after the
mother's; marriage before birth / before 13 / after a partner's death, partners > 30 years apart
(info); siblings < 9 months apart (full dates only; ≤ 2 days = twins); no family links (info), empty
family, one-member family (info), same child twice in a family; own ancestor (cycle, Tarjan —
never hangs); HUSB with SEX F / WIFE with SEX M; no name; possible duplicates
(`duplicate_candidates` score ≥ 0.85, each pair once, info). Dates are day ranges («1850» = the
whole year, BEF/AFT open-ended, BET spans both): a check fires only when it holds for the whole
range; «ABT» widens by a year and lowers the severity one step; free-text dates are skipped.

Top bar: an icon button left of undo/redo (⚠ with a badge counting errors + warnings — danger
when there are errors, else warning colour; ✓ when clean), tooltip «Revisión de datos (⌘⇧R)».
⌘⇧R or the button toggles a **non-modal 380 px panel** between the canvas and the ficha (so the
focused person stays visible): title «Revisión» + ×, filter chips «Todos · n / Errores · n /
Avisos · n / Info · n», then sections per severity (icon, label, count) and per kind («Nacido antes
que su madre · 2»; at most 30 rows, then «Ver N más»). Row: severity icon, avatar 30, name, message
(«Nació en 1850, antes que su madre (1862)»); the selected row (accent-soft) shows links «Editar»,
«Unir» (duplicates → the merge confirmation dialog), «Ignorar» (session only; cleared when another
file is opened). Click / ↵ focuses `people[0]` on the canvas and keeps the panel open.
Keys (context `Review`, `review::bind_keys`): ↑↓ move over rows (headers skipped, wraps) · ↵ focus
(or expand «Ver más») · ⌘↵ edit · ⌘⌫ ignore · esc closes.
Issues are recomputed off the UI thread whenever the tree revision changes (cached by revision;
≈ 65 ms for 4 000 people in the dev profile). The ficha shows «N problemas en la revisión» under the
chips for a person involved in errors/warnings; clicking it opens the panel on their first issue.

## Mapa (places on OpenStreetMap)

Tab «Mapa» (`crates/gen-app/src/map/`). The ficha stays visible on the right.

**Data.** `Tree::place_groups()` (`crates/gedcom/src/places.rs`) collects every person and family
event with a `PLAC` (birth, death, marriage, burial, residence…), grouped by `place_key` (folded,
whitespace collapsed, comma parts trimmed); the most used spelling is shown. A family event
counts once and lists both partners. Coordinates come first from `PLAC.MAP.LATI/LONG`
(`N/S/E/W` prefixes or signed degrees, `parse_coord`); otherwise from the geocoding cache.
Coordinates are never written back into the GEDCOM.

**Privacy / network.** Geocoding is opt-in. The first time the map has places without
coordinates a calm banner says «Para ubicar N lugares se consultará OpenStreetMap (Nominatim).
Solo se envían nombres de lugares.» with «Buscar ubicaciones» (accent) / «Ahora no» (outline).
Consent is remembered in `settings.json` (`geocode_consent`); «Ahora no» lasts for the session and
leaves a quiet «N lugares sin ubicar» pill that brings the banner back. Only place names are sent,
never people. Requests go to `nominatim.openstreetmap.org/search?format=jsonv2&limit=1`, one at a
time and at least 1.1 s apart (process-wide lock), with the User-Agent
`genealogy-rs/<version> (+https://github.com/otherwisetech/genealogy-rs)`. Every answer — «not
found» included — is cached forever in `geocode-cache.json` next to `settings.json` and never
asked again. A network error or HTTP error stops the run and shows «No se pudo conectar con
OpenStreetMap.» + «Reintentar». While it runs a pill shows «Ubicando 12 de 40…»; afterwards
«N lugares no encontrados» opens the list of names OpenStreetMap did not recognise.

**Tiles.** Standard OSM raster tiles (`tile.openstreetmap.org/{z}/{x}/{y}.png`, same User-Agent),
only the ones on screen, centre first, at most 2 downloads at a time, no prefetch. Disk cache under
`~/Library/Caches/genealogy-rs/tiles/{z}/{x}/{y}.png` (XDG cache dir elsewhere), capped at 200 MB
(oldest files deleted down to 160 MB); tiles older than 30 days are shown and fetched again.
Offline, downloads pause 20 s after a failure (5 min on HTTP 403/429). While a tile loads, a
lower-zoom tile already in memory is shown scaled, else the canvas colour. In dark mode tiles are
drawn at 78 % opacity over the dark canvas. Attribution «© OpenStreetMap contributors» sits
bottom-right on a translucent surface chip and opens openstreetmap.org/copyright.

**View.** Web-Mercator camera with fractional zoom 1–18 (tiles of the rounded level, scaled):
drag or wheel pans, ⌘+wheel / pinch zooms around the pointer; bottom-left pill − / + (snap to whole
levels) · fit («Ver todos los lugares»). The initial view (and every filter change) fits all
markers, up to zoom 11; new geocoding results refit until the user moves the map.
Markers: accent circles with a 2 px surface ring and the event count (radius 9 + 4·ln n, max 24),
clustered greedily in screen space when they overlap. Hover → dark tooltip with the place name (or
«N lugares») and «N eventos». Click a single place → a 320-wide panel top-right: place name,
«N eventos», rows of avatar 30 + name + «Nacimiento · 1850»; the focus row is accent-soft. Click a
row focuses that person (ficha updates, the map stays), double-click edits. Clicking a cluster
zooms in two levels on it. A plain click on the map closes the panel. Choosing a place in the ⌘K
palette while on the map selects it and centres there.
Filter pill top-left: «Todos» / «Antepasados de <foco>» (the focus and all ancestors over every
FAMC link; family events count when either partner passes).

**Conexiones familiares.** When a single place is selected, the panel opens with a section
«Conexiones familiares» above the event rows (now headed «Eventos»); data from
`Tree::place_links(groups, key, focus, filter)` (`crates/gedcom/src/place_links.rs`), respecting the
filter pill. Summary line «42 personas · 3 ramas familiares · 1710–1918». Then up to 8 **branch
cards** (radius 10, border token; the focus's branch first, accent border + accent-soft): people at
the place joined when one is a direct ancestor of the other (≤ 8 generations) or they share an
ancestor ≤ 3 generations up, plus partners. Card: «Familia Prueba» (most common surname,
Inter 600 13), «6 personas · 4 generaciones · 1710–1800» (muted 12), and the longest direct line
«Abel → Bruno → … → Dario» (first 3, last 2) — or up to 5 member names when there is no line;
every name is a link that focuses the person. «N personas sin parientes aquí» below; with one
person only «Solo una persona tiene eventos aquí.». **Parejas**: family events at the place (MARR
first) or both partners with a RESI there — «Bruno ♥ Eva», «Matrimonio · 1765». **Padres e
hijos**: both have events there — «Caro → Dario», «madre e hijo · nacieron aquí los dos».
**Migraciones**: by event year, the event just before someone's first one here («Llegó desde X» /
«3 llegaron desde X») and just after their last one («Se fue a Y»), grouped by place, most people
first; a row click selects that place and centres the map there. The map draws a 1.5 px accent arc
(55 % opacity, bowed to one side) from the selected place to each migration place with
coordinates. Event rows add the relationship to the focus («Nacimiento · 1850 · tatarabuelo»,
`Tree::kinship`), computed 12 per frame and cached per (revision, focus, language). Lists are
capped (8 branches, 12 pairs, 10 migrations, then «y N más»); the panel scrolls.
