# gen-rs — for agents and contributors

Read `docs/DESIGN.md` before touching UI.

## Layout

```
crates/i18n     gen-i18n: embedded translation catalogs, t!/tn! macros, locale switching
crates/gedcom   gen-gedcom: lossless GEDCOM parser/writer, tree model, edits, search — no UI
crates/gen-app  `gen` desktop app (GPUI + gpui-component)
```

## Rules

- **i18n is mandatory.** No user-facing string literal anywhere (UI, status lines, errors shown
  to users, labels returned by the library). Use `i18n::t!("module.key", name = value)` /
  `i18n::tn!("module.key", count)` and add the key to **every** `crates/i18n/locales/<lang>/`
  catalog in the same change. `cargo test -p gen-i18n` fails if any language misses a key or a
  placeholder. The user switches language at runtime, so never cache translated text across
  renders.
- Catalog files are per module (`core.ftl`, `app.ftl`, `dialogs.ftl`, `common.ftl`); keys are
  prefixed with the module (`app.topbar.export`). Adding a language = adding a folder with all
  files.
- Never commit real family trees (`*.ged` is ignored outside test fixtures) or real people's names.
- Every core change ships with a test. GEDCOM output must stay lossless (unknown tags preserved).
- Comments: sparse, only non-obvious why.
- Disk-heavy GPUI build: `cargo build -p gen-app` (dev profile has no debuginfo on purpose).

## Validate

```
cargo test -p gen-i18n -p gen-gedcom -p gen-app
cargo clippy --workspace --all-targets
```
