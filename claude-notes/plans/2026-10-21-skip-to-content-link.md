---
title: 'HTML: skip-to-content link for Bootstrap-themed output (bd-rbd3jq4n)'
date: 2026-10-21
description: 'Port Quarto 1''s skip-to-content link (#14685) to q2''s full HTML template, SCSS and language plumbing; investigation skeleton pending design alignment.'
status: draft  # Investigation — pending design alignment with user; do not implement before the go-ahead
braid:
  strand: bd-rbd3jq4n
---

**Branch:** `braid/bd-rbd3jq4n-html-skip-content-link` (topic branch in the main checkout, no worktree; based on `main` @ `905c4961f`)
**Do not start implementation until the user gives the go-ahead.**

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

## Proposed phases (draft)

- Phase 0 — Sequencing: rebase onto `main` after #882 merges (or branch off
  it now).
- Phase 1 — Tests first: port `issue-14684.qmd` and
  `issue-14684-theme-none.qmd` to `crates/quarto/tests/smoke-all/`; a
  website/book fixture (navbar + sidebar) asserting `body >
  a#quarto-skip-link:first-child`; a localization fixture (`lang: es` or
  `fr`) asserting the translated text; a `template.rs` unit test.
- Phase 2 — Markup: transform (or extension of an existing one) writes
  `rendered.skip-to-content-text` from `LanguageTerms`, escaped; template
  emits the link as the first thing in `<body>` and `tabindex="-1"` on
  `<main>`.
- Phase 3 — SCSS: port the `:focus` rule, the outline suppression, and
  `$zindex-skip-link`; update the styles.css baseline hash; note the port
  in the bootstrap layer's drift record.
- Phase 4 — Schema: add `skip-to-content` to the `language:` term schema
  (if q2's schema enumerates term keys — check `is_known_term_key`).
- Phase 5 — Preview parity (pending Q3 below).
- Phase 6 — Docs / changelog entry.

## Open design questions for the user

1. **Sequencing with #882.** Wait for PR #882 to merge and rebase this
   branch onto `main`, or rebase onto `feature/bd-xm7l-vendored-updates`
   now and land after it?
2. **Custom `template:` documents.** Q1 injects the link into every
   bootstrap page, including user templates, because it's a DOM
   postprocessor. In q2, should the link be (a) template-only (built-in
   template gets it; custom templates can use a `$skip-link()$` partial /
   `$rendered.skip-to-content-text$` if they want), or (b) injected
   post-template into any bootstrap-themed HTML to match Q1 exactly?
3. **Preview.** Should hub-client preview render the skip link too (in
   `PreviewDocument.tsx`), or is it render-only — with the parity tooling
   told to ignore `#quarto-skip-link`? (Preview runs in an iframe inside
   the editor; a "skip to content" target there is arguably meaningless.)
4. **`page-layout: custom`.** q2 keeps `<main>` under custom layout, so the
   link targets `main` there. Accept that divergence from Q1 (whose target
   is `div.page-layout-custom`), or is the custom-layout DOM itself a
   separate parity bug worth a strand?
5. **Revealjs and other bootstrap-less HTML targets.** Confirm the link is
   only for the full (bootstrap) HTML template — i.e. revealjs, `minimal`,
   `theme: none/pandoc` all stay without it.

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
