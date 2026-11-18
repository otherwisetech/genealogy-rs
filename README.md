# genealogy-rs

A fast desktop app to **explore and edit GEDCOM family trees**, written in Rust with
[GPUI](https://www.gpui.rs/) and [gpui-component](https://github.com/longbridge/gpui-component).

- Open any `.ged` export (MyHeritage, Ancestry, FamilySearch, Gramps, …) — thousands of people load in milliseconds.
- Instant accent-insensitive search by name, married name, year, place or id.
- Pan/zoom pedigree and descendants canvas (3–6 generations), ancestor fan, full list, ⌘K search.
- Edit names, sex, birth/death, notes in a dialog; add father/mother/partner/son/daughter; delete.
- Spanish and English UI (switch from the globe menu; remembered across runs).
- Undo/redo, and **lossless saving**: every tag the app does not understand (vendor `_UID`, `_UPD`,
  sources, media, …) is written back untouched.

## Run

```sh
cargo run -p gen-app --release -- path/to/tree.ged   # or open one from the app with ⌘O
```

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
crates/gedcom    library (gen-gedcom): lossless parser/writer, family-tree model, edits, search — no UI
crates/gen-app   the `gen` desktop binary (GPUI)
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
cargo test -p gen-gedcom
cargo build -p gen-app
cargo clippy --workspace
```

Test fixtures are synthetic. **Do not commit real family trees** — `.gitignore` blocks `*.ged`
outside `crates/gedcom/tests/fixtures/`.

## License

MIT OR Apache-2.0
