---
title: 'link-external-icon / -newwindow / -filter for HTML output (bd-t3n1nghb)'
date: 2026-10-22
description: 'The three link-external options are accepted into the merged config but nothing consumes them; this plan scopes a render-time AST transform plus a gated SCSS layer to match Quarto 1.'
status: draft  # Investigation — pending design alignment with user; do not implement before the go-ahead
braid:
  strand: bd-t3n1nghb
---

**Branch:** `braid/bd-t3n1nghb-link-external-icon-newwindow` (topic branch in the main checkout, based on `main` @ `03da98869`)
**Do not start implementation until the user gives the go-ahead.**

## Triage verdict

**Ready to design.** The gap is real and reproducible at HEAD (see the
repro below), the Quarto 1 behaviour is small and fully understood, and
q2 has a clear home for each piece (a finalization-phase AST transform
next to `ResponsiveImageTransform`, and a gated SCSS layer modelled on
`title_block_layer`). Five design questions remain, the main one being
how far a render-time transform should deviate from Quarto 1's
client-side, host-based test.

## Issue context

Filed 2026-10-22 by Claude Fable 5.1 (P2 feature, label
`websites parity`) while assessing `posit-dev/product-doc-theme` on q2.
The `posit-docs` project type sets `link-external-icon: true` and
`link-external-newwindow: true`. q2 accepts both keys into the merged
config (`q2 get-config` prints them), but no code in `crates/*/src` or
`resources/` reads them, so those sites get plain links.

Acceptance criteria (from the strand): with `newwindow`, external body
links get `target="_blank" rel="noopener"`; with `icon: true`, they get
class `external` and a visible icon; a string icon value renders the
named Bootstrap icon; `link-external-filter` (default derived from
`site-url`) exempts matching links; internal links are left alone. The
work ships with a smoke-all fixture and a docs entry.

## Dependency graph

- **discovered-from**: bd-0i6fii0k (in progress): the product-doc-theme
  (posit-docs) exploratory assessment. This strand is finding F3 of that
  assessment. Sibling findings were filed as bd-74ktvzv7 (F1, logo
  doubling), bd-rfz4e3bw (F4, `q2 add`), bd-7u2xu4gp, bd-hfghrqvh,
  bd-3z5bubrf, bd-f5l0n6oa and bd-2pmjmjmj. None of them blocks this
  strand, and this strand blocks none of them. The assessment plan
  (`claude-notes/plans/2026-10-21-product-doc-theme-q2.md`) lives on
  `braid/bd-0i6fii0k-product-doc-theme-q2`, not on `main`.
- **blocks / related**: none.

Nothing else depends on this strand. Its urgency comes from the
posit-docs adoption path alone.

## What the code looks like today

### q2

- No consumer: `grep -rn link-external crates/*/src resources/` matches
  only an unrelated Typst octique icon. The only matches elsewhere are
  in the `ts-packages/quarto-api` config key list, which classifies
  these keys as render options.
- No schema entries either: the keys pass through as unvalidated
  metadata.
- No `a.external` rule exists in q2's SCSS (`resources/scss/`).
- Bootstrap Icons CSS is shipped **only for websites**
  (`crates/quarto-core/src/transforms/website_bootstrap_icons.rs`). The
  string-icon form (`<i class="bi-…">`) on a standalone document would
  therefore render nothing unless the icons are also shipped there.
- Natural home for the transform: the finalization phase in
  `crates/quarto-core/src/pipeline.rs` (around line 2699), next to
  `TableBootstrapClassTransform` and `ResponsiveImageTransform`. Both
  are late, idempotent, HTML-gated passes over body nodes.
- SCSS gating precedent: `ThemeConfig::title_block_layer`
  (`crates/quarto-sass/src/config.rs`) together with
  `load_title_block_layer()` (`crates/quarto-sass/src/bundle.rs`). A
  `link_external_layer: bool` flag would follow the same shape and go
  into the CSS cache key.
- `site-url` is already read by `project/website_config.rs`.

Repro: `link-external-options-investigation/repro.qmd` (one relative
link, one external link, one filter-matched link and one `mailto:`
link). Result at HEAD: `q2 get-config repro.qmd link-external-icon`
prints `true`, but every link renders as a bare `<a href="…">`. No
`external` class, no `target`/`rel`, no `a.external` CSS rule in the
compiled stylesheet, and no client-side script. The bug reproduces as
the strand describes it.

### Quarto 1 (reference behaviour)

From the quarto-cli checkout at `~/repos/github/quarto-dev/quarto-cli`:

- `src/format/html/format-html.ts:355-368` passes the three options to
  the template. When `link-external-filter` is unset and
  `website.site-url` is set, the filter defaults to `site-url` with `.`
  and `/` escaped.
- `src/resources/formats/html/templates/quarto-html-after-body.ejs:~220-260`
  holds the client-side logic, which runs on `DOMContentLoaded`:
  - A link counts as **internal** when `filterRegex` matches its
    resolved `href`, when it is `localhost`, or when it is `mailto:`.
    Without a filter, `filterRegex` is `/<window.location.host>/`. Links
    are tested after resolution, so relative links are always internal.
  - Candidate links are `a[href]` minus the navigation classes
    (`.nav-link`, `.navbar-brand`, `.toc-action`, `.sidebar-link`,
    `.sidebar-item-toggle`, `.pagination-link`, `.no-external`,
    `[aria-hidden]`, `.dropdown-item`, `.quarto-navigation-tool`,
    `.about-link`). The test covers every link in the DOM, including
    links from raw HTML, listings and the title block.
  - `newwindow` sets `target="_blank"`, and sets `rel="noopener"` only
    when the link has no `rel` already.
  - `icon: true` adds the class `external`. A string `icon: <name>`
    inserts `<i class="bi-<name> quarto-ext-icon">` as a **sibling
    after** the `<a>`, not inside it. The value is the bare icon name
    (for example `box-arrow-up-right`) without the `bi-` prefix. The
    strand text says `bi-<name>`, which is slightly off.
  - The filter string is used as a JS regex. If it contains `\\`, it is
    emitted as a regex literal; otherwise it goes through
    `new RegExp("…")`.
- `src/resources/formats/html/_quarto-rules-link-external.scss` is
  included only when `link-external-icon` is truthy
  (`format-html-shared.ts:281`). It holds `a.external:after` (an inline
  SVG box-arrow tinted with `$link-color`), a `none` override for
  `div.sourceCode code a.external:after`, and the `.quarto-ext-icon`
  sizing.
- Default: `formats-shared.ts:259-260` sets both booleans to `false`.

## Proposed phases (draft)

- **Phase 0: Tests first.** Write unit tests for the classifier
  (relative, absolute-other-host, filter-matched, localhost, mailto, a
  link carrying `.no-external`, a link with an existing `rel`). Add a
  smoke-all fixture covering `icon: true`, `icon: <name>`,
  `newwindow: true`, an explicit filter and a website with a default
  `site-url` filter. Add a quarto-sass test that the layer is present
  when the flag is on and absent when it is off.
- **Phase 1: SCSS layer.** Port `_quarto-rules-link-external.scss` into
  `resources/scss/html/templates/`. Add the `ThemeConfig` flag, wire it
  from the merged config and include it in the cache key.
- **Phase 2: AST transform.** Add a `LinkExternalTransform` that runs in
  the finalization phase and is HTML-gated (html, and probably
  revealjs; see Q4). It walks `Inline::Link`, classifies each link and
  applies the class, `target`/`rel` and sibling icon. It runs after
  `LinkRewriteTransform`, so hrefs are final.
- **Phase 3: Icon availability.** Make sure Bootstrap Icons ship when
  the string form is used outside websites (see Q3).
- **Phase 4: Schema + docs.** Add schema entries for the three keys
  (`boolean | string`, `boolean`, `string`). Add a docs entry under
  `docs/guides/formats/html/`. Record any deliberate deviations in the
  strand.

## Open design questions for the user

1. **Where the "external" test runs.** Quarto 1 decides in the browser
   by comparing against `window.location.host`. A render-time transform
   cannot know the deploy host, so it would treat as external every
   absolute `http(s)` URL that does not match the filter
   (default: `site-url`), and as internal every relative link and every
   `localhost`/`mailto:` link. The only divergence is an absolute link
   to the page's own host on a site with no `site-url`. Is that
   deviation acceptable, or do you want the client-side JS ported
   instead, or added on top for that case?
2. **Coverage beyond AST links.** An AST transform only sees Markdown
   `Link`s in the body. Quarto 1 also adorns `<a>` tags in raw HTML
   blocks, listing items and title-block or author metadata links that
   carry none of the navigation classes. Is body-AST-only acceptable for
   now, perhaps with a follow-up strand, or is parity on raw-HTML links
   required? Requiring it pushes us toward a post-HTML pass or the JS.
3. **String icon on standalone documents.** Bootstrap Icons ship only
   for websites today. When `link-external-icon` is a string on a
   non-website render, should we (a) ship the icons CSS and font for
   that page, (b) warn, or (c) fall back to the `.external` class?
4. **Formats.** Apply to `html` only, or also to `revealjs` and other
   HTML-family formats? Quarto 1's after-body script runs for
   Bootstrap HTML, and reveal.js has its own `a.external` styling.
   There is also the q2-preview (hub-client) question: add the
   transform to `Q2_PREVIEW_TRANSFORM_EXCLUDED`, or let the React
   preview show the adorned links?
5. **Filter regex dialect.** The filter is a JS regex in Quarto 1. Rust's
   `regex` crate has no lookaround or backreferences. Should we accept
   the Rust dialect and emit a diagnostic when the pattern does not
   compile? Should we also replicate Quarto 1's "`\\` means regex
   literal" quirk, or treat the value uniformly as a regex source
   string?

## Risks / tradeoffs (draft)

- Adding a flag to `ThemeConfig` changes the SCSS cache key. That is
  harmless, but cache-key tests may need updating.
- Inserting the string-icon `<i>` as a sibling means splicing into the
  parent inline list. That is easy in the walker, but it has to avoid
  double-insertion if the transform runs twice. (Neighbouring
  transforms are idempotent by convention.)
- Behavioural risk is low: both booleans default to `false`, so
  existing renders do not change unless a document opts in.
