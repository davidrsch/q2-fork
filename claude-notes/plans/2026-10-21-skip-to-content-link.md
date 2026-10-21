---
title: 'HTML: skip-to-content link for Bootstrap-themed output (bd-rbd3jq4n)'
date: 2026-10-21
description: 'Port Quarto 1''s skip-to-content link (#14685) to q2''s full HTML template, SCSS and language plumbing, render-only for now.'
status: in-progress
braid:
  strand: bd-rbd3jq4n
---

**Branch:** `braid/bd-rbd3jq4n-html-skip-content-link` (topic branch in the main checkout, no worktree; rebased onto `feature/bd-xm7l-vendored-updates` (PR #882) @ `eeaefbc62` per decision 1)

## Triage verdict

**Ready to design, with one sequencing prerequisite:** the `skip-to-content`
language key (and the v1.11.6 recording of the bootstrap SCSS layer) live on
the unmerged PR #882 (`feature/bd-xm7l-vendored-updates`), not on `main`, so
implementation should either wait for #882 or branch off it.

## Issue context

bd-rbd3jq4n (feature, P3, labels `a11y`, `html`; filed 2026-10-21 by Carlos
Scheidegger, open). Upstream quarto-cli `7eebb00b9` (#14685, in 1.11.6) adds a
"Skip to main content" link to Bootstrap-themed HTML (documents, websites,
books, dashboards, custom layouts; not `theme: none`), a new language key
`skip-to-content`, and SCSS in `_bootstrap-rules.scss` /
`_bootstrap-variables.scss`. The strand asks to port markup, styles, schema
option and tests.

Note on "schema option": upstream adds no new user option. The only schema
change is the `skip-to-content: string` entry under the `language:` term map
in `definitions.yml`. That is the whole schema surface to port.

## Dependency graph

- **discovered-from**: bd-jsfg1494 (closed) — the vendored survey's
  quarto-cli v1.11.3 → v1.11.6 resync. Its commit `b7126df57` re-copied the
  language catalog (so `skip-to-content: "Skip to main content"` is in
  `resources/language/_language*.yml` *on that branch*) and recorded the
  `bootstrap-quarto-layer` asset at v1.11.6 after porting three standalone
  CSS fixes into q2's reorganised `_bootstrap-rules.scss`. The skip-link
  SCSS was **deliberately left out** and deferred to this strand (sibling
  follow-ups: bd-ix7psky0 accessible dark-mode switch, bd-gebcr1sa
  placeholder rasterization).
- **Consequence:** once #882 merges, `resources/vendored.toml` will claim
  `bootstrap-quarto-layer` is current at v1.11.6 while the skip-link rules
  are missing. This strand closes that gap; it should note itself in
  `resources/scss/README.md` (or wherever the layer's "ours vs. pinned"
  drift is recorded) until it lands.
- No `blocks` / `related` edges.

## What the code looks like today

Upstream implementation (Q1, `external-sources/quarto-cli` @ 7eebb00b9):

- `format-html-bootstrap.ts` `injectSkipLink`: a DOM *postprocessor* that,
  for every bootstrap-themed page, picks a target
  (`#quarto-document-content` → `main` → `div.page-layout-custom` →
  `#quarto-content`), gives it an id if missing and `tabindex="-1"`, and
  inserts `<a id="quarto-skip-link" class="visually-hidden-focusable"
  href="#<id>">{language.skip-to-content}</a>` as the first child of `body`.
  Because it's a postprocessor, it also applies to user `template:` files.
- `_bootstrap-rules.scss`: `#quarto-skip-link:focus { position: fixed; … }`
  plus `#quarto-document-content:focus, #quarto-content:focus { outline:
  none; }`. `_bootstrap-variables.scss`: `$zindex-skip-link: 1070 !default`.
- Dashboards: `role="main"` on the container + JS click handler that
  focuses without touching the hash.
- Tests: `tests/docs/smoke-all/2026/07/13/issue-14684{,-custom-layout,-dashboard,-theme-none}.qmd`,
  `tests/smoke/site/render-skip-link.test.ts`,
  `tests/integration/playwright/tests/dashboard-skip-link.spec.ts`.

q2 at `main` @ 905c4961f:

- No trace of `skip-to-content` / `quarto-skip-link` anywhere in `crates/`,
  `resources/`, `ts-packages/`. The language key is absent on `main`
  (present only on `origin/feature/bd-xm7l-vendored-updates`).
- q2 is **template-based, not postprocessor-based.** `FULL_HTML_TEMPLATE`
  in `crates/quarto-core/src/template.rs:239` always emits
  `<main class="content…" id="quarto-document-content">`, so the target
  lookup collapses to a constant. `theme: none` / `minimal: true` already
  route to `MINIMAL_HTML_TEMPLATE` (`format.rs:1118` `is_minimal_html`),
  so the theme-none exclusion falls out for free.
- Localized strings for the template follow the `draft_alert.rs` pattern:
  a transform reads `LanguageTerms::from_meta(meta)`, HTML-escapes, and
  writes `rendered.<key>`; the template interpolates it. Doctemplate does
  not escape interpolations, so escaping in Rust matters (term files are
  user-editable via `language:`).
- `<body>` currently opens with `$color-mode-script$`, then the draft
  alert, search results div, header. Q1's `:first-child` assertion requires
  the skip link to come *before* the color-mode script.
- `page-layout: custom` in q2 still emits `<main>` (only the
  `page-layout-custom` class on `#quarto-content` changes), unlike Q1 which
  has no `<main>` there. So Q1's custom-layout test expectation
  (`div.page-layout-custom#quarto-document-content`) does not transfer
  as-is.
- No dashboard format in q2 → dashboard parts are out of scope.
- User `template:` (`apply_template.rs:178`, `pandoc_write.rs:1695`):
  q2 doesn't postprocess, so a custom template gets no skip link unless it
  opts in.
- Preview: `ts-packages/preview-renderer/src/q2-preview/PreviewDocument.tsx`
  mirrors the template's DOM in React (`#quarto-content` / `<main
  id="quarto-document-content">` at ~L386–401). Parity tooling
  (`domParity.ts`, `smokeAllParity.wasm.test.tsx`) may flag the new
  element if preview doesn't render it.
- SCSS: `resources/scss/bootstrap/_bootstrap-rules.scss` is q2's
  reorganised copy (vendored asset `bootstrap-quarto-layer`, policy
  `track`); the rendered `styles.css` has a baseline hash test that will
  need updating.
- Smoke-all home: `crates/quarto/tests/smoke-all/` (e.g. `localization/`,
  `navigation/`, `themes/`) is the natural place for ported fixtures.

## Decisions (2026-10-21, with the user)

1. **Sequencing:** rebase onto `feature/bd-xm7l-vendored-updates` (PR #882)
   and start now; this branch lands after #882.
2. **Custom templates:** the link is in the built-in full template only.
   Custom templates opt in with the built-in `skip-link` partial
   (`$skip-link()$` / `$skip-link.html()$`), documented in
   `docs/guides/authoring/templates.qmd`.
3. **Preview:** render-only for now. Follow-up strand **bd-21n09dwj**
   (discovered-from this one) covers the preview.
4. **`page-layout: custom`:** accept the divergence. q2 keeps `<main>`, so
   `<main>` is the target there.
5. **Scope:** full Bootstrap HTML template only (the long navbar/sidebar
   website case is the motivation). Revealjs, `minimal`, `theme:
   none/pandoc` stay without it.

## Implementation (as built)

- Markup: `SKIP_LINK_PARTIAL` in `crates/quarto-core/src/template.rs`,
  registered in `builtin_html_partials()` and called as the first line of
  `<body>` in `FULL_HTML_TEMPLATE`. `<main id="quarto-document-content">`
  gains `tabindex="-1"`.
- Label: `render_with_compiled_template` computes `$skip-to-content-text$`
  from `LanguageTerms::from_meta` (fallback "Skip to main content"),
  HTML-escapes it, and excludes the same-named metadata key so authors
  can't inject markup. Computing it there (like `$main-column$`) rather
  than in a new transform means custom templates get the variable too,
  with no new pipeline stage.
- Ordering: the link precedes the color-mode script, matching Q1. The FOUC
  comment there was updated: one inline element ahead of a synchronous
  inline script opens no paint window.
- SCSS: upstream's `#quarto-skip-link:focus` rule, the outline suppression
  on `#quarto-document-content` / `#quarto-content`, and
  `$zindex-skip-link: 1070 !default`, ported verbatim into q2's
  `_bootstrap-rules.scss` / `_bootstrap-variables.scss`.
- Schema: no edit needed. `is_known_term_key` derives legal `language:`
  keys from the vendored catalog, which already has `skip-to-content`.
  The TS mirror `kLanguageDefaultsKeys` (`ts-packages/quarto-api`) gained
  `skip-to-content` plus the five `navigation-*-label` keys (#14813),
  whose absence already had its Q1 parity test red against the local
  `external-sources/quarto-cli` checkout.
- Preview: the `<main>` in `PreviewDocument.tsx` gains `tabIndex={-1}`. Not the
  link, just the attribute, because `<main>` is the DOM-parity root and
  its attributes are compared.

## Phases

- [x] Phase 1 — Tests first: 7 `template.rs` unit tests (first child of
  body, before color-mode script, focusable target, localized + escaped,
  not author-settable, absent from minimal template, custom-template
  partial); smoke-all fixtures `skip-link/{skip-link,skip-link-theme-none,skip-link-custom-layout}.qmd`,
  `localization/lang-fr-skip-link.qmd`, `navigation/skip-link-website/`.
- [x] Phase 2 — Markup + label (template.rs).
- [x] Phase 3 — SCSS port.
- [x] Phase 4 — Schema / key lists (TS mirror only; see above).
- [x] Phase 5 — Preview parity attribute (`tabIndex` on `<main>`);
  the preview link itself → bd-21n09dwj.
- [x] Phase 6 — Docs (`templates.qmd` section).
- [x] Phase 7 — Goldens and verify. Accepted insta snapshots (lmd
  blog/desugar/project, attribution baseline, banner main tag) and the
  phase-5 hash baseline; the only deltas are the link and `tabindex`
  (CSS delta verified to be exactly the two new rules). Fixtures that
  pinned "first thing in `<body>`" (draft banner, title banners) and the
  color-mode FOUC test now allow the skip link ahead. `cargo xtask
  verify --skip-hub-build`: 17054/17055 pass; the remaining failure is
  three typst smoke-all fixtures that fetch remote images, failing
  because the local Homebrew pandoc 3.12.1 is built without HTTP
  support (environmental). hub-client WASM smoke-all not run locally
  (needs the WASM build).
- [x] Phase 8 — hub-client changelog entry.

## Risks / tradeoffs (draft)

- Adding a first-child element to `<body>` may break existing tests or
  goldens that assert body structure (`body >` selectors, DOM-parity
  snapshots, book refactor guard goldens). Expect a round of golden
  updates.
- `tabindex="-1"` on `<main>` plus `outline: none` must not interfere with
  q2's existing focus-ring work (bare `.btn` focus fix from b7126df57) —
  the outline suppression is scoped to the content containers only.
- Upstream's `box-shadow: rgba(...)` is a shadow, an explicitly allowed
  alpha exception per `.claude/rules/hub-client-theme.md` (and that rule is
  about hub-client anyway, not the rendered bootstrap SCSS).
