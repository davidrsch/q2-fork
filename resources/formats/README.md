# Per-format include resources

Static resources from Quarto 1's `src/resources/formats/` that q2 embeds
into the pandoc share tree, per the External Sources Policy (see root
`CLAUDE.md`). Registered as `formats` in `resources/vendored.toml`.

## Source

Copied from quarto-cli tag `v1.11.3`, unchanged through `v1.11.6` (verified 2026-10-21; recorded as `v1.11.6`):

- `html/styles-callout.html` ← `src/resources/formats/html/styles-callout.html`
  (byte-identical)
- `docx/{caution,important,note,tip,warning}.png` ←
  `src/resources/formats/docx/` (byte-identical; the docx callout icons)
- `epub/styles.html` — adapted from `src/resources/formats/html/pandoc/styles.html`
  for the epub follow-on (not byte-identical)

Embedded via `include_dir!` as `FORMATS_DIR` / `FORMATS_DOCX_DIR` in
`crates/quarto-core/src/pandoc_filters/mod.rs`.

## Updating

```bash
cd external-sources/quarto-cli   # or any clone, at the new tag
git diff v1.11.6 <new-tag> -- src/resources/formats/html/styles-callout.html \
  src/resources/formats/html/pandoc/styles.html src/resources/formats/docx
```

Re-copy the byte-identical files, hand-merge any change into
`epub/styles.html`, then update the tag above and the `formats` entry in
`resources/vendored.toml`.
