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

## Download

Ready-made builds for macOS (Apple silicon and Intel), Linux and Windows are attached to each
[GitHub Release](https://github.com/otherwisetech/genealogy-rs/releases).

- **macOS:** unzip and drag `genealogy-rs.app` to Applications. The app is not notarized yet, so the
  first time right-click it → *Open* (or run `xattr -dr com.apple.quarantine /Applications/genealogy-rs.app`).
- **Linux:** extract the `.tar.gz` and run `./genealogy-rs`; copy `genealogy-rs.desktop` and the icon to
  `~/.local/share/applications` / `~/.local/share/icons` for a menu entry. Needs Vulkan.
- **Windows:** unzip and run `genealogy-rs.exe` (SmartScreen may ask to confirm, since it is unsigned).

### Opening `.ged` files by double-click

- **macOS:** the app declares the GEDCOM type (`org.gedcom.ged`), so once it is in Applications it
  appears in Finder's *Open With* for `.ged` files. To make it the default: select a `.ged` file →
  *Get Info* (⌘I) → *Open with: genealogy-rs* → *Change All…*.
- **Linux:** from the extracted folder, install the binary, menu entry, icon and the
  `application/x-gedcom` MIME type, then set the default:

  ```sh
  install -Dm755 genealogy-rs ~/.local/bin/genealogy-rs
  install -Dm644 genealogy-rs.desktop ~/.local/share/applications/genealogy-rs.desktop
  install -Dm644 genealogy-rs.png ~/.local/share/icons/hicolor/1024x1024/apps/genealogy-rs.png
  install -Dm644 genealogy-rs-gedcom.xml ~/.local/share/mime/packages/genealogy-rs-gedcom.xml
  update-mime-database ~/.local/share/mime
  update-desktop-database ~/.local/share/applications
  xdg-mime default genealogy-rs.desktop application/x-gedcom
  ```
  (`~/.local/bin` must be on your `PATH`.)
- **Windows:** the zip has no installer, so nothing is registered. Right-click a `.ged` file →
  *Open with* → *Choose another app* → *Choose an app on your PC* → pick `genealogy-rs.exe` → *Always*.
  From a terminal, `genealogy-rs.exe tree.ged` opens it directly.

### Unsigned builds and update notices

The downloads are built by GitHub Actions from the tagged source, but they are **not code-signed
or notarized** (that needs paid certificates), which is why macOS and Windows ask for confirmation
the first time. If you prefer, build from source (see *Run*).

There is no auto-updater. On startup, at most once a day, the app asks the public GitHub API
(`api.github.com/repos/otherwisetech/genealogy-rs/releases/latest`) for the latest release; the
request carries only a `genealogy-rs/<version>` User-Agent with the project URL, never tree data. If a newer version
exists, a small notice offers to open the release page in your browser. Turn it off from the globe
menu (*Check for updates on startup*).

The same switch covers the star count next to *Star on GitHub* (footer and About): once a day the
app reads `api.github.com/repos/otherwisetech/genealogy-rs` (no login, no user data) and caches the
number; if that fails the count is simply not shown. Starring itself happens on github.com.

## AI assistant (⌘J)

The **Asistente** button (or ⌘J) docks the shared Otherwise AI panel
([otherwise-agent-panel](https://github.com/otherwisetech/otherwise-agent-panel)) next to the
tree, as a bar, at the bottom, on the right or full window, in Chat or Terminal mode. It runs
**your own** Claude Code, Codex or opencode (whichever is installed), or a model of your own
**Ollama** that runs on this computer, and gives it the open tree for that session only, through a small read-only MCP server (`genealogy-rs mcp`); your agent's
configuration files are never changed. Try *¿Qué nombres se repiten más?*, *¿De dónde viene la
familia?*, *¿Cómo están emparentados … y …?* or *¿Qué datos están mal o faltan?*.

- **What it can read**: exact aggregates in one call — name frequencies (`name_stats`: given
  names and surnames counted separately, «Ana María» counts for Ana and María, accents folded),
  the shape of the tree (`tree_stats`: sex, living, births per century, lifespans, generations,
  children per family, completeness), places by birth / death / marriage / residence with their
  hierarchy (`place_stats`), how two people are related with the path (`relationship`), events
  in order for a person or a branch (`timeline`), a typed query (`find_people`: name, sex,
  dates, places, missing fields, descendants / ancestors of someone, counts per group) and data
  problems (`data_quality`: duplicates, impossible dates, odd ages, gaps). Name search matches
  whole words: *Juan* never finds *Juana*. It reads the tree as it is in the window,
  unsaved edits included (a snapshot in `<data folder>/genealogy/assistant/live/`, removed when
  the app quits).
- **Changes are cards**: in *Preguntar antes de cambiar* or *Automático* the assistant may
  propose adding a person or a relationship, fixing a date, place or name, or merging duplicates.
  Each appears as a card; nothing changes until you press *Aplicar*, and ⌘Z undoes it.
  *Solo lectura* offers no proposals at all.
- **Privacy**: family data is personal. What you ask, what you attach and whatever the assistant
  reads from the tree (names, dates, places, notes) goes to the provider you picked (Anthropic,
  OpenAI, opencode's provider) with your own account; the panel says so under the input. With
  Ollama everything stays on this computer. Nothing else leaves the computer, and the agent's own shell, file and web tools are off.
- Conversations are kept in `<data folder>/genealogy/assistant/`.

## Works with other Otherwise apps

genealogy-rs uses [otherwise-link](https://github.com/otherwisetech/otherwise-link), the small
layer Otherwise Technologies' apps share. Everything stays on your computer; both switches are in
the globe (preferences) menu.

- **Share people** (off by default). When it is on *and* the shared entity registry is turned on in
  [Otherwise Hub](https://github.com/otherwisetech/otherwise-apps), every person of the open tree
  becomes a *person* card other apps can use (mosaico tags photos with them, Vigía tracks their
  documents). A card carries the name (given / family) and the **exact** birth and death dates;
  notes, places, sources and everything else stay in your file. People are pushed when a tree is
  opened or saved, only those that changed; nothing is ever deleted from the registry. A person's
  id there comes from the record's `_UID` / `UID` when it has one, else from the file's path and
  the record's xref.
- **Calendar** (same switch as Today): the same people as yearly events in
  `<data folder>/genealogy/events.json` for Otherwise Hub's Calendar (otherwise-link SPEC §9):
  names and dates only, each opening the person (`otherwise://genealogy/person?file=…&id=…`).
- **Today** (on by default): birthdays of living people and wedding anniversaries of living couples
  in the next 14 days, from exact dates, appear in Otherwise Hub's *Today* view and open the
  person here. The feed is `today/genealogy.json` in the shared Otherwise folder.
- **Track a document in Vigía…** in a person's ⋯ menu (shown when Vigía is installed) shares that
  person and opens Vigía's new-item form with them as the owner
  (`otherwise://vigia/new?owner=<entity>&template=passport`).
- **Links.** genealogy-rs opens `otherwise-genealogy://` links (also `otherwise://genealogy/…`
  through Otherwise Hub):

  | Link | Does |
  |------|------|
  | `otherwise://genealogy` | brings the window to the front |
  | `otherwise://genealogy/open?file=/path/tree.ged` | opens that tree |
  | `otherwise://genealogy/person?file=/path/tree.ged&id=I12` | opens the tree if needed and shows that person |
  | `otherwise://genealogy/entity/<uuid>` | shows the person behind a shared entity, if it is in the open tree |

  macOS registers the scheme from the app's `Info.plist`; Windows registers it for your user
  (`HKCU\Software\Classes\otherwise-genealogy`) when the app starts; on Linux
  `genealogy-rs.desktop` declares `x-scheme-handler/otherwise-genealogy` and the app makes itself
  the handler when that file is in `~/.local/share/applications`.
- **About** lists the other Otherwise apps installed on this computer; click one to open it.

## Where files live

- **Your trees** stay wherever you save them. A new tree is offered the folder you used last, else
  the Otherwise data folder (`~/.otherwise/genealogy/`, or wherever you moved the data root in
  Otherwise Hub); never a temp folder. **Import / export** has *Move to…* (verified copy, the
  original deleted only afterwards), *Show in Finder* and your **recent trees**; a tree that moved
  shows *Locate…* / *Remove from list*. The native **File** menu has the same.
- **Cloud folders** (OneDrive, Dropbox, iCloud Drive, Google Drive): an open tree is marked with a
  small `<file>.lock.json` next to it. Opened on a second computer while the first still has it,
  it opens read-only there (*Use here anyway* takes it over), and conflicted copies left by the
  sync client are pointed out with *Show in Finder*.
- **The app's own files** (settings, `recent.json`, `otherwise-sync.json`, the geocoding and tile
  caches) live in `~/.otherwise/genealogy/`. Version 0.5 moves them there once from the old
  folder (`~/Library/Application Support/genealogy-rs`, `%APPDATA%\genealogy-rs`,
  `~/.config/genealogy-rs`), never overwriting anything, and leaves a `.migrated-to-otherwise`
  note behind. `OTHERWISE_DATA_DIR` points the data root elsewhere (tests, portable setups).
- **Help → Crea tu tienda en Vcity** (also in About): a short invitation to
  [Vcity](https://vcity.io), online stores for businesses. The link only carries
  `utm_source=genealogy`; nothing about you.

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
| ⌘J | show / hide the AI assistant |

## Layout

```
crates/i18n      translation catalogs (gen-i18n), embedded at build time
crates/gedcom    library (gen-gedcom): lossless parser/writer, family-tree model, edits, search — no UI
crates/mcp       gen-mcp: the read-only MCP server the AI panel starts (`genealogy-rs mcp`) and its proposals
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

Developed by [Otherwise Technologies LLC](https://otherwisetech.com), Wyoming, USA.
