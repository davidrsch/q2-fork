# Language term files (localization)

Per-language translations of Quarto's document-visible terms (callout titles,
crossref titles/prefixes, TOC title, title-block labels, etc.).

- `_language.yml` — English defaults; the authoritative catalog of term keys.
- `_language-<tag>.yml` — overrides for one BCP 47 tag. Region/script variants
  (`_language-pt-BR.yml`, `_language-de-CH.yml`, `_language-sr-Latn.yml`) are
  merged on top of their parent (`_language-pt.yml`, …) via the subtag walk in
  `crates/quarto-core/src/language.rs`.

These files are embedded into the `quarto-core` binary via `include_dir!`
(see `crates/quarto-core/src/language.rs`) and resolved at render time based
on the document's `lang` option and `language:` metadata overrides.

## Provenance

Copied from `quarto-cli` (`src/resources/language/`) at tag `v1.11.6` (commit
`8680bd68d9083c8f2a575d68168d0e739a004061`) on 2026-10-21 (first copied at
`45caede32` on 2026-07-17), per the external-sources policy (compile-time resources must live
in-repo, never referenced from `external-sources/`).

To update: re-copy the files from a current `quarto-cli` checkout, update the
commit hash above, and run the catalog integrity test
(`cargo nextest run -p quarto-core -E 'test(language_catalog)'`). Key-level
compatibility with Quarto 1 is deliberate — do not rename keys.

## Quarto 2 additions

A few keys exist only in q2. A re-copy from `quarto-cli` drops them, so
re-add them to `_language.yml` after every update (the catalog integrity test
`base_catalog_contains_keys_consumed_by_transforms` fails until you do).
Other languages fall back to the English value until a translation lands.

| key | English | consumer |
|---|---|---|
| `announcement-close` | Close announcement | `website.announcement` close button (bd-website-announcement-w1gtpzki) |

Design: `claude-notes/plans/2026-07-17-localization-i18n-design.md`
(braid strand bd-llhlzd7p).
