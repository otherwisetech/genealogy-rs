<p align="center"><img src="crates/gen-app/assets/logo/gen-icon-1024.png" width="128" alt=""></p>

# genealogy-rs

A fast desktop app to **explore and edit GEDCOM family trees**, written in Rust with
[GPUI](https://www.gpui.rs/) and [gpui-component](https://github.com/longbridge/gpui-component).

- Open any `.ged` export (MyHeritage, Ancestry, FamilySearch, Gramps, …) — thousands of people load in milliseconds.
- Instant accent-insensitive search by name, married name, year, place or id.
- Pan/zoom pedigree and descendants canvas (3–6 generations), ancestor fan, full list, ⌘K search.
- Map of the tree's places on OpenStreetMap: markers sized by event count, clustering, «ancestors of»
  filter. Uses `PLAC.MAP` coordinates from the file; other places are geocoded with Nominatim only
  after you agree (only place names are sent, 1 request/second, cached forever). Tiles are cached
  on disk (≤ 200 MB). Map data © OpenStreetMap contributors.
- Edit names, sex, birth/death, notes in a dialog; add father/mother/partner/son/daughter; delete.
- Spanish, English, Portuguese and French UI (switch from the globe menu; remembered across runs).
- Undo/redo, and **lossless saving**: every tag the app does not understand (vendor `_UID`, `_UPD`,
  sources, media, …) is written back untouched.

## Run

```sh
cargo run -p gen-app --release -- path/to/tree.ged   # or open one from the app with ⌘O
```

On macOS, `script/bundle-macos.sh` builds `target/release/genealogy-rs.app` with the app icon (drag it to
/Applications).

Requires a recent stable Rust. macOS is the primary target (GPUI also supports Linux/Windows).
The first build compiles GPUI from git and takes a while.

### Shortcuts

| Key | Action |
| --- | --- |
| ⌘O / ⌘S / ⌘⇧S / ⌘N | open / save / export (save as) / new tree |
| ⌘K or ⌘F | search palette (↵ focus, ⌘↵ edit) |
| ← → ↑ ↓ | move between relatives on the tree (see docs/DESIGN.md) |
| F | center the focus person |
| ↵ or ⌘↵, double-click | edit the focus person |
| ⌘[ or ⌫ | back to the previously focused person |
| drag, wheel / ⌘+wheel, pinch | pan / zoom (25–200 %) |
| ⌘= / ⌘− / ⌘0 / ⌘9 | zoom in / out / 100 % / fit |
| ⌘Z / ⌘⇧Z | undo / redo (outside text fields) |

## Layout

```
crates/i18n      translation catalogs (gen-i18n), embedded at build time
crates/gedcom    library (gen-gedcom): lossless parser/writer, family-tree model, edits, search — no UI
crates/gen-app   the `genealogy-rs` desktop binary (GPUI)
```

The library is UI-free and usable on its own:

```rust
let mut tree = gedcom::load("tree.ged".as_ref())??;
let id = tree.add_person("Ana", "García", gedcom::Sex::Female);
tree.set_event(&id, "BIRT", "12 JUN 1950", "Madrid")?;
gedcom::save(&tree, "tree.ged".as_ref())?;
```

`cargo run -p gen-gedcom --example roundtrip -- tree.ged` checks that a file survives a load/save.

### Why not an existing GEDCOM crate?

Typed parsers such as `ged_io` reject common real-world exports (e.g. non-standard `QUAY` values,
raw newlines inside notes) and drop vendor tags on write, so editing would lose data. gen-rs keeps
a generic line tree and builds the family view on top of it.

## Develop

```sh
cargo test -p gen-i18n -p gen-gedcom -p gen-app
cargo build -p gen-app
cargo clippy --workspace
```

Test fixtures are synthetic. **Do not commit real family trees** — `.gitignore` blocks `*.ged`
outside `crates/gedcom/tests/fixtures/`.

## Translations

Every UI string lives in `crates/i18n/locales/<lang>/*.ftl` (`key = text`, `{name}` placeholders,
`.one`/`.other` plural forms, `.vowel` forms for languages that elide before a vowel). To add a
language:

1. Copy a locale folder, e.g. `cp -r crates/i18n/locales/en crates/i18n/locales/de`.
2. Translate every value in every file (keep keys and `{placeholders}` as they are), including
   `common.language_name` — the name shown in the language menu.
3. Run `cargo test -p gen-i18n`: it fails on missing/extra keys, mismatched placeholders, keys
   used in code but missing from the catalog, catalog keys no code uses, and hard-coded UI text.

The language is picked from the system locale on first run and switched from the globe menu.

## License

MIT OR Apache-2.0
