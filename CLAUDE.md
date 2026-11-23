# gen-rs — for agents and contributors

Read `docs/DESIGN.md` before touching UI.

## Layout

```
crates/i18n     gen-i18n: embedded translation catalogs, t!/tn!/tv! macros, locale switching, i18n guard tests
crates/gedcom   gen-gedcom: lossless GEDCOM parser/writer, tree model, edits, search — no UI
crates/mcp      gen-mcp: `genealogy-rs mcp` — stdio MCP server over one tree file (read tools; with
                `--propose <dir>` also propose_* tools that only drop proposals in that folder)
crates/gen-app  `genealogy-rs` desktop app (GPUI + gpui-component); otherwise.rs: otherwise-link
                logic (links, entity cards, registry push, Today events; tested), shared.rs: the
                window's side of it (link pump, publish after load/save, Vigía action);
                storage.rs: the app's own folder `<data root>/genealogy/` (settings, caches,
                recent.json), one-time migration from the old config folder, default save folder
                (never temp); files.rs: the open tree's lock / conflicted copies / Move to… /
                Show in Finder / recent + Locate…; vcity.rs: the Vcity invitation (About + Help)
```

## Rules

- **i18n is mandatory.** The whole UI exists in every bundled language (es, en, pt, fr) and the
  user switches between them at runtime. No user-facing string literal anywhere (UI, tooltips,
  placeholders, menus, window title, toasts, errors shown to users — `#[error]` included —, labels
  returned by the library). Use `i18n::t!("module.key", name = value)`, `i18n::tn!("module.key",
  count)` (plural `.one`/`.other`) or `i18n::tv!("module.key", word, …)` (`.vowel` form when `word`
  starts with a vowel: fr «fils d'Anne») and add the key to **every** `crates/i18n/locales/<lang>/`
  catalog in the same change. Prefer whole-phrase keys over concatenating translated fragments.
- `cargo test -p gen-i18n` enforces it: catalogs in sync (keys + placeholders), and
  `crates/i18n/tests/guard.rs` scans `gen-app/src` and `gedcom/src` (test code excluded) for
  literals with words passed to UI sinks (`.child`, `.label`, `.tooltip`, `set_placeholder`,
  `Tooltip::new`, `title:`, `#[error]`, …), for keys used in code but missing from the catalog, and
  for catalog keys nothing uses. Keys built at runtime need their prefix in `DYNAMIC_PREFIXES`. A
  literal that is genuinely not translatable (product name, symbols) ends its line with
  `// i18n-ok`.
- Never cache translated text across renders: build it in `render`, or store the locale it was
  built for and rebuild on change (see `sync_locale` in the edit dialog and palette — input
  placeholders are stored state, so they need this). Native menus are rebuilt in `set_language`.
- Catalog files are per module (`core.ftl`, `app.ftl`, `dialogs.ftl`, `common.ftl`); keys are
  prefixed with the module (`app.topbar.export`). Adding a language = copying a locale folder and
  translating every file (incl. `common.language_name`); no code changes.
- Never commit real family trees (`*.ged` is ignored outside test fixtures) or real people's names.
- Every core change ships with a test. GEDCOM output must stay lossless (unknown tags preserved).
- Shared entity cards carry names and exact birth / death dates only — never notes, places or
  sources. Pushing is opt-in (`share_people`, plus the registry's own opt-in), incremental
  (`otherwise-sync.json`), and never deletes or merges in the registry. otherwise-link is pinned
  by git tag and must only add its own crates to Cargo.lock; tests use temp dirs (`Root::at`).
- The app's own files go through `storage::app_dir()`; never write next to a tree except its
  `.lock.json` (released on close). Moves use otherwise-link's verified `storage::move_any`,
  never a plain copy + delete. Migration never overwrites a file at the new place.
- **The assistant is the shared AI panel** (`otherwise-agent-panel`, `gen-app/src/agent.rs`,
  ⌘J): the user's Claude Code / Codex / opencode with `genealogy-rs mcp --tree <snapshot>` for
  that session only (never write the user's agent configs). The server **never writes the
  tree**: it reads the window's snapshot (`<app dir>/assistant/live/<pid>/tree.ged`, rewritten
  when `rev` changes, removed on quit) and, outside "Solo lectura", `propose_*` tools validate a
  `genmcp::proposal::Proposal` and drop it in `…/proposals/`; the window polls that folder and
  shows cards; Aplicar is one `GenApp::edit` (⌘Z). New tools go in `gen-mcp` with a test on
  synthetic fixtures. The privacy note (`app.agent.privacy_*`) must keep saying that family
  data the assistant reads goes to the chosen provider with the user's account.
- Comments: sparse, only non-obvious why.
- Disk-heavy GPUI build: `cargo build -p gen-app` (dev profile has no debuginfo on purpose).

## Validate

```
cargo test -p gen-i18n -p gen-gedcom -p gen-mcp -p gen-app
cargo clippy --workspace --all-targets
```
