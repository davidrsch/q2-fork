# Listing runtime

The client-side runtime for website listings (sorting, filtering,
pagination), embedded by `crates/quarto-core/src/transforms/listing_render.rs`.
Registered as `quarto-listing-js` in `resources/vendored.toml`.

## Source

Both files are byte-identical to quarto-cli at commit
`19a7da95dccf7581cf3b54a552ba04f55f86d8f2` (verified 2026-10-21; they were
copied earlier, at an unrecorded ref):

- `list.min.js` ← `src/resources/projects/website/listing/list.min.js` —
  **Quarto 1's patched build of List.js** (special-character handling,
  quarto-cli `d57aae7aa`). Its upstream is quarto-cli, not the npm `list.js`
  package; do not replace it with an npm release.
- `quarto-listing.js` ← `src/resources/projects/website/listing/quarto-listing.js`

## Updating

```bash
cd external-sources/quarto-cli   # or any clone, at the new ref
git log --oneline 19a7da95d..<new-ref> -- \
  src/resources/projects/website/listing/list.min.js \
  src/resources/projects/website/listing/quarto-listing.js
```

If either changed, re-copy it, then update the commit above and `version`
in the `quarto-listing-js` registry entry.
