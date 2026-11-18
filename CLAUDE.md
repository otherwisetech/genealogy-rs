# gen-rs — for agents and contributors

Read `docs/DESIGN.md` before touching UI.

## Layout

```
crates/i18n     gen-i18n: embedded translation catalogs, t!/tn!/tv! macros, locale switching, i18n guard tests
crates/gedcom   gen-gedcom: lossless GEDCOM parser/writer, tree model, edits, search — no UI
crates/gen-app  `gen` desktop app (GPUI + gpui-component)
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
- Comments: sparse, only non-obvious why.
- Disk-heavy GPUI build: `cargo build -p gen-app` (dev profile has no debuginfo on purpose).

## Validate

```
cargo test -p gen-i18n -p gen-gedcom -p gen-app
cargo clippy --workspace --all-targets
```
