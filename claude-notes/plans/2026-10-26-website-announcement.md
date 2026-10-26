---
title: 'website.announcement: an announcement banner as a generate/render stage pair'
date: 2026-10-26
description: 'Ports Quarto 1 `website.announcement` to Quarto 2 as a format-agnostic generate transform (`navigation.announcement`) plus an HTML render transform (`rendered.navigation.announcement-*`), mirroring the page-footer pair. The banner lives only in metadata, so llms.txt companions never contain it; configuration is set once in `_quarto.yml` and merged into every page, so a page opts out with `website.announcement: false`.'
---

Strand: **bd-website-announcement-w1gtpzki**.

## Motivation

The preliminary Quarto 2 documentation site (`docs/`) needs a sitewide banner
saying that some pages have not yet received a human review. Pages that *have*
been reviewed must be able to turn the banner off. Two requirements from the
user:

1. llms.txt output (the `.md` companions, `llms.txt`, `llms-full.txt`) never
   contains the banner, by default.
2. The banner is configured in one place (`website.announcement` in
   `docs/_quarto.yml`) and reaches each page through the ordinary metadata
   merge, so a reviewed page opts out with `website: {announcement: false}` in
   its own front matter (or a whole directory opts out via `_metadata.yml`).

## How Quarto 1 does it

Reference: `external-sources/quarto-cli/src/`.

**Schema** (`resources/schema/definitions.yml:919`). `website.announcement` is
either a string (the content) or an object:

| key           | type                                   | Q1 default      |
|---------------|----------------------------------------|-----------------|
| `content`     | string, markdown                       | (required)      |
| `dismissable` | boolean                                | `true`          |
| `icon`        | bootstrap icon name                    | none            |
| `position`    | `above-navbar` or `below-navbar`       | `above-navbar`  |
| `type`        | `primary`, `secondary`, `success`, `danger`, `warning`, `info`, `light`, `dark` | `primary` |

**Parsing** (`project/types/website/website-shared.ts:293-317`). Read once from
the *project* config (`websiteConfig(kAnnouncement, project.config)`), not from
page metadata, into a `NavigationAnnouncement` with an `id` equal to the MD5 of
the content string. Consequence: Q1 has **no per-page opt-out**. The feature we
want is strictly more than Q1 offers.

**Markdown** (`website-navigation-md.ts:231-249`). The content string goes
through the "navigation markdown pipeline" envelope; the rendered HTML is
spliced into `#quarto-announcement .quarto-announcement-content` by a DOM
postprocessor.

**Markup** (`resources/projects/website/templates/nav-before-body.ejs:14-19,
92-94`). The banner lives *inside* `<header id="quarto-header" class="headroom
fixed-top">`, either before the `<nav class="navbar">` or after it (and after
the secondary nav). The announcement alone is enough to emit the header
(`nav.navbar || nav.sidebar || nav.announcement`):

```html
<div id="quarto-announcement" data-announcement-id="<md5>"
     class="alert alert-<type> hidden">
  <i class="bi bi-<icon> quarto-announcement-icon"></i>   <!-- if icon -->
  <div class="quarto-announcement-content">…rendered markdown…</div>
  <i class="bi bi-x-lg quarto-announcement-action"></i>    <!-- if dismissable -->
</div>
```

**JS** (`resources/projects/website/navigation/quarto-nav.js:8-45`). The banner
is emitted `hidden`. On `DOMContentLoaded`, `announceRegister()` checks
`localStorage["quarto-announce-<id>"]`; if set, it removes the element,
otherwise it removes `hidden` and wires the close icon to remove the element
and set the key. Because the id is a content hash, editing the text re-shows
the banner to everyone who dismissed the old one.

**SCSS** (`resources/projects/website/navigation/quarto-nav.scss:1021-1043`):
flex row, `padding: .5em`, `font-size: .9em`, zero bottom margin, `p` margins
zeroed inside the content, icon spacing, pointer cursor on the close icon.

## What Quarto 2 already has

- **The generate/render pattern.** `transforms/footer_generate.rs` resolves
  `page-footer:` into a typed `quarto_navigation::PageFooter` stored at
  `navigation.footer` (format-agnostic); `transforms/footer_render.rs` turns it
  into HTML at `rendered.navigation.footer` (HTML-specific, selected per format
  by `footer_render_stage()` in `pipeline.rs`). Navbar and sidebar follow the
  same shape. This is the template for the new pair.
- **Metadata merging.** Project `website:` config is merged into every page
  metadata, with `_metadata.yml` and front matter layered on top.
  `MergedConfig` resolves a path from the *highest-priority layer that has it*,
  so a front-matter scalar `announcement: false` replaces a project-level map,
  while a front-matter *map* merges field-wise (e.g. a page could override only
  `type:`). `quarto_config::resolve_website_value(meta, key)` reads a key at
  either `meta.<key>` or `meta.website.<key>`.
- **Markdown in config strings.** `transforms/config_markdown.rs`
  (`MARKDOWN_CONFIG_PATHS`) re-parses blessed project-config strings as qmd
  before `ShortcodeResolveTransform`, so shortcodes and inline markup work.
  Adding the announcement content is one or two entries.
- **Rendering metadata markdown to HTML.** `quarto_navigation::render_html`
  (`render_text`, `inlines_to_html`) renders inlines/blocks held in a
  `ConfigValue`; `footer_render.rs` (`rewrite_region_hrefs`) already rewrites
  links/images inside a metadata text region to page-relative hrefs at depth.
- **The header partial.** `template.rs` `QUARTO_HEADER_PARTIAL` holds the
  `<header id="quarto-header">` markup; the call site in `FULL_HTML_TEMPLATE`
  gates it on `rendered.navigation.navbar` / `secondary-nav` (content-gated,
  documented as a deliberate deviation from Q1 that omits the announcement
  term only because q2 had no announcement).
- **quarto-nav.js** (`resources/js/quarto-nav/quarto-nav.js`) is ported, with an
  explicit note: *"no announcement-bar registration — feature not ported."*
  It already re-measures the fixed header via `ResizeObserver`, so un-hiding
  or dismissing a banner re-pads the page.
- **llms capture** (`transforms/llms.rs`, `LlmsCaptureTransform`) clones the
  page *body* AST and serializes it with an empty metadata map. Anything that
  lives only in metadata — navbar, footer, and the announcement — never reaches
  a companion. Requirement 1 is therefore met *structurally* by keeping the
  banner out of `ast.blocks`.
- **Preview.** `ts-packages/preview-renderer/src/q2-preview/PreviewDocument.tsx`
  and `chromeSlots.tsx` rebuild the header in React from
  `rendered.navigation.*`, and the preview pipeline is the HTML transform
  pipeline minus `Q2_PREVIEW_TRANSFORM_EXCLUDED`.
- **Not present:** no `#quarto-announcement` SCSS (the `.hidden` utility does
  exist in `resources/scss/bootstrap/_bootstrap-rules.scss:1319`); no schema
  file for website keys (q2 has none for `page-footer` either, so there is no
  schema step).

## Design

### D1. Config surface

Accept exactly the Q1 shapes, read through `resolve_website_value(meta,
"announcement")` so both `website.announcement` (Q1 spelling, what the docs
site will use) and a top-level `announcement:` work, like other website
features:

- `false` (or `null`) — no banner. This is the per-page opt-out.
- a string — the content, all other fields default.
- a map — `content` plus the optional fields in the Q1 table, same defaults.
- `true`, or a map with empty/missing `content` — no banner, plus a warning
  diagnostic (Decision 4; almost certainly a config mistake).

One q2 addition to the Q1 map (Decision 3): `remember-dismissal: boolean`,
default `true` (Q1 behavior: a dismissal is stored in localStorage and sticks
sitewide until the content changes). With `false`, the close button hides the
banner for the current page view only; nothing is stored, so the next page
shows it again. Ignored when `dismissable: false`.

### D2. Typed model in `quarto-navigation`

New `crates/quarto-navigation/src/announcement.rs`, next to `footer.rs`:

```rust
pub struct Announcement {
    pub id: String,              // stable dismissal key, see D3
    pub content: ConfigValue,    // PandocInlines / PandocBlocks / plain string
    pub dismissable: bool,       // default true
    pub remember_dismissal: bool, // default true (q2 addition, Decision 3)
    pub kind: AnnouncementType,  // `type:`; default Primary
    pub position: AnnouncementPosition, // default AboveNavbar
    pub icon: Option<String>,
}
pub fn resolve_announcement(meta: &ConfigValue) -> Option<Announcement>;
impl Announcement { fn to_config_value(..); fn from_config_value(..); }
```

plus `render_html::announcement_to_html(&Announcement, close_label: &str) ->
String` emitting the Q1 markup with the same ids and classes (so Q1 user CSS
keeps working), with two deliberate differences:

- The close control is a real button (Decision 5), keeping the Q1 classes so
  the CSS/JS hooks are unchanged:

  ```html
  <button type="button" class="quarto-announcement-action"
          aria-label="Close announcement"><i class="bi bi-x-lg"
          aria-hidden="true"></i></button>
  ```

  The SCSS resets the button chrome (no border/background, inherit color) so
  it looks like the Q1 icon. The label is a new localized term
  (`announcement-close`, working name) added to `resources/language/`, with
  the English string as fallback for languages that lack it.
- `data-announcement-remember="false"` is added when `remember-dismissal:
  false`.

Unknown `type`/`position` values fall back to the default with a warning
diagnostic.

### D3. Dismissal id

`sha2` (already a quarto-core dependency) over the *authored* content, taken
as plain text before any per-page href rewriting, truncated to 16 hex chars.
It must not hash rendered HTML: link rewriting makes the HTML differ per page
depth, which would make a dismissal on one page not stick on another. Matching
Q1 MD5 values exactly is not a goal (nobody carries localStorage across a Q1 to
Q2 migration of the same site).

### D4. Generate stage: `AnnouncementGenerateTransform`

`transforms/announcement_generate.rs`, `TransformPhase::Navigation`, name
`announcement-generate`, registered right after `FooterGenerateTransform`.
Format-agnostic. Skips when the resolved value is `false`/absent/empty, or when
`navigation.announcement` is already populated (user override, same convention
as footer). Resolves link targets inside the content against the YAML file they
were authored in (the `resolve_metadata_path` treatment `footer_generate.rs`
gives items), then stores `navigation.announcement`.

Markdown: add `["website", "announcement"]`, `["website", "announcement",
"content"]` and the two top-level equivalents to `MARKDOWN_CONFIG_PATHS`, so
shortcodes resolve and `**bold**` / `[links](…)` render.

### D5. Render stage: `AnnouncementRenderTransform` (HTML)

`transforms/announcement_render.rs`, name `announcement-render`, registered in
the render sub-phase before `NavbarRenderTransform` (so header predicates see
it). Reads `navigation.announcement`, rewrites hrefs inside the content the way
`footer_render.rs` does for text regions (page-relative at depth, Q-code
diagnostic for unknown `.qmd`), renders with `announcement_to_html`, and writes
**one of two keys** depending on position:

- `rendered.navigation.announcement-above`
- `rendered.navigation.announcement-below`

Two keys rather than HTML + a position flag, because the template language has
no equality test; each placement is then a plain `$if$`.

Non-reveal HTML only. For revealjs there is no site header; the generate half
still runs (harmless, metadata only) and nothing renders it. If we later want
dashboards or slides to show it, they add their own render stage, which is the
point of the split.

### D6. Template and header gating

- `QUARTO_HEADER_PARTIAL`: emit `announcement-above` before the navbar and
  `announcement-below` after the secondary nav (Q1 order).
- `FULL_HTML_TEMPLATE` header gate: add `$elseif$` arms for the two announcement
  keys, so an announcement-only page still gets a header; update the
  partial doc comment, which currently calls the missing announcement term a
  deliberate deviation.
- `QuartoNavJsTransform` (`quarto_nav_js.rs:106`): include the announcement
  keys in `has_header`, since the JS both pads the page for the fixed header
  and does the dismissal.
- `nav-fixed` body class: today composed only when a navbar renders. Decide
  during implementation whether an announcement-only header needs it (Q1 adds
  `nav-fixed` whenever the header is fixed; the JS offset makes it mostly
  moot). The docs site has a navbar, so this does not block the motivating
  use.

### D7. JS and SCSS

- Port `announceDismiss` / `announceRegister` into `quarto-nav.js`, with one
  change: when the element carries `data-announcement-remember="false"`, skip
  the localStorage read on register and the write on dismiss (the click still
  removes the element). Expose
  `window.quartoAnnounceRegister` so the preview can re-run it, and update the
  deviations list in the file header comment.
- Port the `#quarto-announcement` SCSS block into
  `resources/scss/bootstrap/_bootstrap-rules.scss` next to
  `#quarto-draft-alert`. Colors come from Bootstrap `alert-*` (solid, so the
  hub-client no-transparency rule is satisfied).

### D8. llms.txt

No code beyond tests: the banner lives in `meta.navigation.*` /
`meta.rendered.*` only, and `LlmsCaptureTransform` serializes the body with
empty metadata. Add a test that a page with an announcement produces a
companion without the content, and leave a comment in `announcement_generate.rs`
saying *why* the banner must never be injected into `ast.blocks`.

### D9. Pipeline bookkeeping

- `BUCKETS`: both transforms `Bucket::B4`, like `footer-generate` /
  `footer-render`.
- Pandoc-profile exclusion list (`pipeline.rs` around line 2942): add both
  names (no Pandoc-writer slot for site chrome).
- Preview: keep both in the preview pipeline (not in
  `Q2_PREVIEW_TRANSFORM_EXCLUDED`); add an `AnnouncementSlot` in
  `chromeSlots.tsx`, render it inside the React `#quarto-header` at the two
  positions, widen the header gate in `PreviewDocument.tsx`, and call
  `window.quartoAnnounceRegister?.()` from the slot effect. Without that call
  the preview would show nothing: the banner arrives `hidden`, and
  `DOMContentLoaded` fires once per iframe, not once per re-render.
- Phase-ordering test and bucket totality test will flag anything missed.

### D10. Docs site

In `docs/_quarto.yml`:

```yaml
website:
  announcement:
    content: "This page has not yet been reviewed by a human. It may contain errors."
    type: warning
    icon: info-circle
    remember-dismissal: false   # close per page view; never stored (Decision 3)
```

Reviewed pages add `website: {announcement: false}` to their front matter.
Add a guide page section documenting the option (with the opt-out pattern),
since the docs site is also where users will learn the feature.

## Work plan

- [x] **1. Model + HTML** in `quarto-navigation`: `announcement.rs`,
  `resolve_*`, round-trip, `announcement_to_html` (button close control,
  remember attribute), the `announcement-close` language term; unit tests for
  the string form, the map form, defaults, `false`, `true`/empty content
  (warning), bad `type`/`position`, `remember-dismissal: false`.
- [x] **2. Generate transform** + `MARKDOWN_CONFIG_PATHS` entries; unit tests
  including a merge test: project map + front-matter `false` resolves to no
  banner; project map + front-matter `{type: danger}` merges field-wise.
- [x] **3. Render transform**, template partial + gate, nav-js predicate,
  pipeline registration, buckets, Pandoc exclusion; unit tests for position
  keys, href rewriting at depth, skip conditions.
- [x] **4. JS + SCSS** port.
- [ ] **5. llms test**: companion has no banner text.
- [ ] **6. Smoke-all** fixtures under `crates/quarto/tests/smoke-all/navigation/`
  (website project): banner above navbar on page A, opted out on page B via
  front matter, opted out on directory C via `_metadata.yml`, below-navbar
  variant, announcement-only (no navbar) header.
- [ ] **7. Preview** slot + integration test
  (`PreviewDocument.integration.test.tsx`). Use the `preview-render-parity`
  skill to compare DOM.
- [ ] **8. Docs**: `docs/_quarto.yml` + guide page; render `docs/` and eyeball.
- [ ] **9. Verification**: `cargo xtask verify`; the Windows `cargo clippy
  --target x86_64-pc-windows-msvc` cross-check for touched crates;
  `q2 render --strict claude-notes`.

### Progress log

(Appended as work lands; newest last.)

- **Step 1 (2026-10-26).** `quarto-navigation/src/announcement.rs`
  (`Announcement`, `AnnouncementType`, `AnnouncementPosition`,
  `AnnouncementWarning`, `resolve_announcement`, `content_id`) and
  `render_html::announcement_to_html`; 18 unit tests. Notes:
  - `resolve_announcement` returns `(Option<Announcement>, Vec<AnnouncementWarning>)`,
    the `repo_action_links` pattern: the crate stays diagnostic-free and
    quarto-core maps warnings to Q-codes.
  - The stored form carries `id`; `from_config_value` keeps a present `id`
    and otherwise hashes the content, so a user-populated
    `navigation.announcement` works too.
  - Blocks content renders one `<p>` per block (Q1 parity), inlines render
    inline; the footer `render_text` flattening is not reused for blocks.
  - The icon `<i>` also gets `aria-hidden="true"` (decorative).
  - Language term: `resources/language/` is a vendored Q1 copy with no
    q2-only mechanism, so `announcement-close` is added to `_language.yml`
    under a comment, listed in a new README "Quarto 2 additions" table,
    noted on the `language` asset in `resources/vendored.toml`, and pinned in
    the `language_catalog` guard test so a re-copy that drops it fails loudly.
    Other languages fall back to English.

## Decisions (user review, 2026-10-26)

- **Decision 1. Config spellings.** Accept both `website.announcement` and a
  top-level `announcement:` via `resolve_website_value`, for consistency with
  other website keys. A page-level top-level `announcement: false` wins over a
  project `website.announcement`, which is the behavior we want.
- **Decision 2. No llms opt-in now.** The structural exclusion (D8) is the
  default; an opt-in such as `announcement: {llms: true}` is additive later.
- **Decision 3. Close without remembering, for the docs banner.** The banner is
  meant to be short enough not to be a nuisance, so it stays closable, but a
  dismissal must not hide it on the other unreviewed pages. Expressed as the
  new `remember-dismissal: false` key (D1, D2, D7); the default stays `true`
  for Q1 parity.
- **Decision 4. `true` / empty content warns.** No banner, plus a warning
  diagnostic.
- **Decision 5. Close control is a `<button>`** with a localized `aria-label`,
  keeping the Q1 classes (D2).

- **Decision 6. `remember-dismissal` defaults to `true`** (Q1 behavior); the
  docs site sets `remember-dismissal: false` for now.
- **Step 2 (2026-10-26).** `transforms/announcement_generate.rs`
  (`announcement-generate`), registered after `footer-generate`, bucket B4,
  in the Pandoc exclusion list; exact-list pipeline tests and the
  Navigation-phase count tripwire (21 → 22) updated. Markdown paths added to
  `MARKDOWN_CONFIG_PATHS`. New codes, each with a catalog entry, a docs page
  under `docs/errors/navigation/`, and a sidebar entry:
  - `Q-13-15` Announcement references missing document (new
    `NavSurface::Announcement`, used by the render step);
  - `Q-13-16` Announcement has no content;
  - `Q-13-17` Invalid announcement option (`type`/`position`).

  **Deviation from D4:** links inside the content are *not* resolved against
  the authoring file at generate time. Only nav *item* hrefs get that
  treatment today; Link/Image targets in config *text* (footer text regions,
  navbar title) are project-root-relative everywhere, and the announcement
  follows that convention rather than special-casing it. The cross-surface
  gap is filed as **bd-3ys3nkw7**.
- **Step 3 (2026-10-26).** `transforms/announcement_render.rs`
  (`announcement-render`), registered right before `navbar-render`, B4,
  Pandoc-excluded, Navigation count tripwire 22 → 23. Self-gates with the
  draft banner's predicate (`format.is_html()` and not a reveal target).
  Header gating is now single-sourced in Rust as
  `transforms::HEADER_CONTENT_KEYS` / `page_has_header` (navbar,
  secondary-nav, announcement-above/below), used by `quarto_nav_js`; the
  template gate is an `$elseif$` chain over the same keys. Both helpers are
  `pub` and re-exported, because their only Rust consumer
  (`quarto_nav_js`) is native-only and a crate-private helper would be dead
  code on wasm32. **D6 `nav-fixed` resolved:** no change. Q1 adds it only
  when the fixed header contains `nav.navbar`, which q2 already matches, so
  an announcement-only header gets none (pinned by a template test).
  - The new language term flows into every Pandoc request (the language
    table rides in metadata and `QUARTO_FILTER_PARAMS`), so the parity-book
    input golden was re-recorded (`BOOK_REQUEST_GOLDEN=record`) and the
    pandoc recordings regenerated with `cargo xtask
    capture-pandoc-recordings` (pandoc 3.12.1). The regeneration also picked
    up a pre-existing share-tree drift from `main` (the fig-alt docx
    backport, bd-ykjwiqcc, had not been re-captured); the orphaned share
    tree `60d82cb8f21d33d2` was removed, as the v1.11.6 resync did.
- **Step 4 (2026-10-26).** `quarto-nav.js` gains the banner logic
  (reveal unless a remembered dismissal applies; close removes the element
  and, unless `data-announcement-remember="false"`, stores
  `quarto-announce-<id>`; localStorage access is try/catch-guarded).
  **Deviation from D7/D9:** no `window.quartoAnnounceRegister` hook.
  `quarto-nav.js` already runs a body-wide MutationObserver that rebinds
  when the preview re-mounts `#quarto-header`; registration is keyed off the
  `#quarto-announcement` element identity in that same observer, so a
  re-mounted banner is revealed again with no React-side call. Covered by
  `ts-packages/preview-renderer/src/q2-preview/quarto-nav-announcement.integration.test.ts`
  (8 jsdom cases running the real script in a fresh window each). SCSS
  block added to `_bootstrap-rules.scss` next to `#quarto-draft-alert`, with
  a button-chrome reset for the `<button>` close control; the dart-sass
  parity fixtures cover Bootstrap only and need no regeneration.
