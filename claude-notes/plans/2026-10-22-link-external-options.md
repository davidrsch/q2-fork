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
`title_block_layer`). The user answered the five initial design
questions on 2026-10-22 (see § Decisions); two small follow-up
questions remain before implementation.

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
- **Quarto 1's schema rejects the string form.**
  `src/resources/schema/document-links.yml` declares
  `link-external-icon` as `schema: boolean` (tagged
  `[$html-doc, revealjs]`). Rendering the repro with
  `link-external-icon: box-arrow-up-right` fails with "Render failed due
  to invalid YAML". With `validate-yaml: false` the template's string
  branch works and emits `quarto-ext-icon` /
  `bi-box-arrow-up-right`. So the string form is an undocumented,
  validation-blocked feature in Quarto 1. The TypeScript type
  (`string | boolean`, `config/types.ts:513`) and the template support
  it; the schema and quarto-web docs do not.
- **Bootstrap Icons ship with every Bootstrap HTML document**, not only
  websites: `bootstrapFormatDependency()`
  (`src/format/html/format-html-bootstrap.ts:93-115`) lists
  `bootstrap-icons.css` and `.woff`. The rendered single file has them
  under `icon_files/libs/bootstrap/`.
- **The `\\` filter quirk has no semantic reason.** It was added in
  `af851f8d9` ("Don't allow invalid regex to result in JS error", fixes
  quarto-cli#4765, 2023-03-11). The filter string is spliced into
  generated JS source, and the branch only picks between a regex literal
  and `new RegExp("…")` to survive JS string escaping. Rust compiles the
  pattern directly, so the quirk does not carry over.
- **quarto-web docs** (`docs/output-formats/html-basics.qmd:347-375`)
  describe the filter only as "a regular expression", with the example
  `^(?:http:|https:)\/\/www\.quarto\.org\/custom`. They say nothing
  about the dialect and do not mention the string icon form. They also
  show manual `{.external target="_blank"}` on a single link. The
  example compiles under Rust's `regex` crate: since 1.8 the crate
  accepts superfluous escapes such as `\/`, and q2 uses 1.12.

## Decisions (user, 2026-10-22)

1. **Classification happens in a render-time AST transform**, not in the
   browser. A transform can later move around the pipeline and be
   replaced or augmented by user filters. q2 treats as external every
   absolute `http(s)` URL that does not match the filter, and as
   internal every relative link and every `localhost` and `mailto:`
   link. The filter defaults to the escaped `site-url`. This diverges
   from Quarto 1 in one accepted case: an absolute link to the page's
   own host on a site with no `site-url` counts as external. A custom
   filter can fix that case.
2. **Only body AST links.** Raw-HTML `<a>` tags, listing chrome and
   title-block metadata links are out of scope, and no follow-up strand
   is needed. Quarto 2 steers users away from raw HTML.
3. **Ship Bootstrap Icons for the string icon form on standalone
   documents too**, which is option (a). The investigation shows this is
   cheap: `WebsiteBootstrapIconsTransform` gates on
   `ProjectKind::Website` alone, and Project-scope artifacts already
   resolve on single-file renders (mermaid and the theme CSS use them).
   See open question A for how wide the new gate should be.
4. **All HTML-family formats** (html, revealjs, …), deliberately wider
   than strict Quarto 1 parity. Quarto 1's schema tags revealjs too. The
   **q2-preview shows adorned links**, so the transform is *not* added
   to `Q2_PREVIEW_TRANSFORM_EXCLUDED`.
5. **The filter uses the Rust `regex` dialect.** A pattern that fails to
   compile produces a diagnostic. The feature docs must say plainly that
   the dialect differs from JavaScript (no lookaround, no
   backreferences). Quarto 1's `\\` quirk is **not** replicated; it has
   no legitimate reason (see above). The default filter is built with
   `regex::escape(site_url)`.

## Proposed phases (draft)

- **Phase 0: Tests first.**
  - Unit tests for the classifier: relative, absolute other-host,
    filter-matched, the default `site-url` filter, localhost, mailto, a
    link carrying `.no-external`, and a link with an existing `rel`.
  - An invalid-regex diagnostic test.
  - Idempotence of the sibling icon insertion.
  - Smoke-all fixtures: a single doc with `icon: true` plus `newwindow`,
    a single doc with `icon: <name>` (asserting that the icons CSS
    ships), and a website with only `site-url`. Also a revealjs doc.
  - A quarto-sass test that the layer is present when the flag is on and
    absent when it is off.
- **Phase 1: SCSS layer.** Port `_quarto-rules-link-external.scss` into
  `resources/scss/html/templates/`. Add the `ThemeConfig` flag, set from
  `link-external-icon` being truthy, and include it in the cache key.
- **Phase 2: AST transform.** Add a `LinkExternalTransform` that runs in
  the finalization phase after `LinkRewriteTransform` and is gated to
  the HTML family. It walks `Inline::Link`, classifies each link, then
  adds class `external` (for `icon: true`), sets `target="_blank"` and
  sets `rel="noopener"` only when `rel` is absent (for `newwindow`). For
  `icon: <name>` it splices an icon inline after the link: a raw HTML
  `<i class="bi-<name> quarto-ext-icon">` or a `Span`, decided at
  implementation time. It skips `.no-external` links and leaves
  `.external` links that already carry the class untouched.
- **Phase 3: Icon shipping.** Widen the `WebsiteBootstrapIconsTransform`
  gate, scoped as decided in open question A.
- **Phase 4: Schema + docs.**
  - Add schema entries: `link-external-icon` as `boolean | string`
    (a deliberate departure from Quarto 1's boolean-only schema, which
    goes in the strand), `link-external-newwindow` as `boolean`,
    `link-external-filter` as `string`.
  - Add a docs page or section under `docs/guides/formats/html/` that
    covers the Rust regex dialect, the string icon form, body-only
    scope, the no-`site-url` divergence, the manual
    `{.external target="_blank"}` escape hatch and `.no-external`.
  - Record the deviations in the strand.

## Open design questions for the user

A. **How wide should the Bootstrap Icons gate be?** Option (i) ships the
   icons when the project is a website *or* `link-external-icon` is a
   string. That is minimal, and it is the only new consumer today.
   Option (ii) ships them for every Bootstrap HTML render, as Quarto 1
   does. That is simpler to reason about and helps future `bi-*`
   consumers in single docs, but every standalone page then carries about
   100 KB of CSS plus a 180 KB woff more.
   Recommendation: (i) now, and (ii) as its own decision if a second
   consumer appears.
B. **Should a manual `.external` link get the icon without the option?**
   Quarto 1 compiles the `a.external::after` rule only when
   `link-external-icon` is on, so `[x](…){.external}` in a document
   without the option gets no icon, even though the quarto-web docs
   present the class as a per-link alternative. Should q2 compile the
   small layer unconditionally for HTML so that the per-link class
   always works? That would also drop the `ThemeConfig` flag and the
   cache-key change. Recommendation: unconditional; the layer is about
   1.6 KB of SCSS.

## Risks / tradeoffs (draft)

- If B resolves to "gated", adding a flag to `ThemeConfig` changes the
  SCSS cache key. That is harmless, but cache-key tests may need
  updating. If B resolves to "unconditional", every HTML stylesheet
  changes once, which may churn CSS snapshot tests.
- Inserting the string-icon `<i>` as a sibling means splicing into the
  parent inline list. That is easy in the walker, but it has to avoid
  double-insertion if the transform runs twice. (Neighbouring
  transforms are idempotent by convention.)
- Behavioural risk is low: both booleans default to `false`, so
  existing renders do not change unless a document opts in.
