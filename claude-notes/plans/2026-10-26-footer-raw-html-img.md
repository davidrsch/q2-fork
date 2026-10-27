---
title: 'page-footer: raw HTML `<img src>` neither copied nor warned about (bd-5r97wehd)'
date: 2026-10-26
description: 'Raw-HTML `<img>` in `_quarto.yml` footer regions is not copied to `_site`, which q2 documents as intended; resolved by rewriting the product-doc-theme footer in markdown that works on both q2 and Quarto 1.'
status: draft  # Investigation — pending design alignment with user; do not implement before the go-ahead
braid:
  strand: bd-5r97wehd
---

**Branch:** `braid/bd-5r97wehd-footer-raw-html-img` (topic branch in the main checkout, based on `main` @ `4d966ea05`)
**Do not start implementation until the user gives the go-ahead.**

## Triage verdict

**Needs a policy decision before design.** The symptom reproduces at HEAD exactly as
filed. But q2 documents it as intended: `docs/guides/projects/paths.qmd`, section
"Raw HTML is not rewritten", says files referenced only from raw HTML are not
collected and tells authors to list them under `project: resources:`. That rule
comes from the closed design strand bd-root-relative-paths-design-fc5pvkcv
(decision 3: "No diagnostic for root-absolute paths surviving in raw HTML. A
substring scanner would inevitably accrete bug reports whose fixes require real
HTML parsing"). The strand's fix sketch, an HTML-tag scanner over footer
`RawInline`/`RawBlock`, would reverse that decision for config regions. Fixing
this means changing that policy on purpose, not just fixing a bug.

**Update 2026-10-26 (user direction): fix the content, not q2.** Both footer
constructs that caused the report can be written in pure markdown that renders
correctly on **both** q2 and Quarto 1 (see "Markdown spellings" below). That
keeps fc5pvkcv's policy intact. The work moves to posit-dev/product-doc-theme
(README + `example/_quarto.yml`). Once that lands, bd-5r97wehd should close as
working-as-designed.

## Markdown spellings (verified on q2 and Q1)

Fixture: `footer-raw-html-img-investigation/markdown-spellings/`. Rendered with
`target/debug/q2` @ `4d966ea05` and Quarto 1 dev (`quarto` 99.9.9). Inspected the
footer of `deep/page.html`.

**Logo item.** Markdown images plus the existing `href`:

```yaml
- text: '![Posit](/_extensions/posit-docs/assets/images/posit-logos-2024_horiz-full-color.svg){.light-content .posit-footer-logo width="65px"}![Posit](/_extensions/posit-docs/assets/images/posit-logo-fullcolor-TM.svg){.dark-content .posit-footer-logo width="65px"}'
  href: "https://posit.co"
```

**Cookie-preferences item.** The id moves from the `<a>` onto a span around the
images, and the item gets a non-navigating `href`:

```yaml
- text: '[![Cookie Preferences](/_extensions/posit-docs/assets/images/cookie.svg){.light-content style="width: 1em; height: 1em;"}![Cookie Preferences](/_extensions/posit-docs/assets/images/cookie-copy.svg){.dark-content style="width: 1em; height: 1em;"}]{#open_preferences_center title="Update cookie preferences"}'
  href: "javascript:void(0)"
  aria-label: 'Cookie Prefs'
```

| Variant | q2 | Q1 |
|---|---|---|
| V1: `[imgs](#){#id title=…}`, no item `href` (the 2026-10-22 experiment) | ✓ renders, copies | ✗ emitted verbatim |
| V2: `[imgs]{#id …}` span, item `href: "#"` | ✓ | renders, but `href="#"` is rewritten to `../#` on deep pages, so a click navigates to the site root |
| V3: `[imgs]{#id …}` span, no item `href` | ✓ | ✗ verbatim (Q1 renders markdown in footer item text only when the item has an `href`) |
| **V4: `[imgs]{#id …}` span, item `href: "javascript:void(0)"`** | **✓** | **✓** (`href` left alone; images relocated to `site_libs/quarto-contrib/`) |

On q2, all four images are copied to `_site/_extensions/posit-docs/assets/images/`
and the emitted `src` values are page-relative. Cosmetic differences: Q1 wraps
the item content in `<p>` inside the `<a>` (it already does this for the logo
item), and q2's V4 anchor carries `class="nav-link"`.

**Behavior caveat (unverified).** `#open_preferences_center` is now a `<span>`
inside the `<a>`, not the `<a>` itself. The theme's own JS
(`assets/_analytics.html`) uses `li:has(#open_preferences_center)`, so it still
matches. The cookie-consent vendor's click handler is injected through GTM and
is not in the repo. A listener bound directly to the element still fires,
because clicks on the images bubble through the span. A delegated handler that
tests `event.target.id` would not fire. This needs a check on a
docs.posit.co / netlify.app deploy preview before the README changes.

### Revised next steps

1. Check the V4 cookie item against the real consent vendor on a
   product-doc-theme deploy preview.
2. Open a product-doc-theme PR that switches the README's footer snippet and
   `example/_quarto.yml` to the markdown spellings (outward-facing, so it needs
   the user's go-ahead).
3. Close bd-5r97wehd as working-as-designed, with a pointer to the PR (needs
   the user's confirmation).
4. Optional, in q2 docs: add the cookie-item pattern (id on a span, `href:
   "javascript:void(0)"`) to `paths.qmd` as a worked example of "write it in
   markdown".

## Issue context

bd-5r97wehd (bug, P2, labels `extensions` and `websites`), filed 2026-10-26 by
Carlos Scheidegger. Footer `<img src="/_extensions/posit-docs/assets/images/cookie.svg">`
(the form the posit-dev/product-doc-theme README recommends) is emitted as written,
but the file is never copied, so it 404s on every page. No Q-5-6 is raised. The
markdown form `![](/_extensions/.../cookie.svg){.light-content}` is copied and
works. The strand places the gap in `copy_footer_images` →
`collect_config_text_images` / `collect_items_images`, which only see `Image`
inlines.

## Dependency graph

- **discovered-from bd-0i6fii0k** (in_progress): the product-doc-theme
  assessment. This is finding F2(2). Plan:
  `claude-notes/plans/2026-10-21-product-doc-theme-q2.md` on branch
  `braid/bd-0i6fii0k-product-doc-theme-q2`, checked out in
  `~/rooms/room-2/q2`, not on main. Relevant points from it:
  - The 2026-10-22 experiment shows that markdown images fix the **logo** item
    on both q2 and Q1. The **cookie-preferences** item (a link with
    `{#open_preferences_center title=…}`, no `href`, wrapping two images) has
    no markdown spelling that Q1 renders, so that one item has to stay raw HTML
    for dual-tool sites. That item is the remaining real-world case.
  - Open question 1 there (F2(1)): keep q2's `_site/_extensions/<ext>/…` layout
    or mimic Q1's `site_libs/quarto-contrib/quarto-project/`. The assessment
    leans toward keeping q2's layout.
  - F9 / bd-f5l0n6oa: the Q-2-9 noise for the same raw HTML. It is a sibling
    symptom in the same footer text.
- **related bd-page-footer-items-f4th80mj** (in_progress, PR #507): made item
  `text:` and bare-string items parse as markdown. That is why raw `<img>` in
  items now arrives as `RawInline` html, rather than escaped text, and reaches
  the collectors this strand is about.
- **Not linked, but they define the policy:**
  bd-root-relative-paths-design-fc5pvkcv (closed; plan
  `2026-08-13-site-root-relative-paths.md`, Case C: "markdown-first incentive
  removal, no HTML parsing, docs carry the message") and
  bd-page-footer-image-items-stmpikgo (closed; plan
  `2026-08-18-page-footer-image-items.md`, which built `copy_footer_images`
  and noted "raw-HTML `{=html}` item … out of scope").
- **Preview-side twins** (open): bd-k5rxujiy (preview asset walker misses
  meta-driven raw-HTML images) and bd-960vrfbm (preview: images inside raw HTML
  blocks aren't served). Both have the same root as this strand: no one looks
  inside raw HTML. A policy change here should consider them too.

## What the code looks like today

The paths in the strand still exist with the same shape
(`crates/quarto-core/src/project/website_post_render.rs`: `copy_footer_images`
~L200, `collect_config_text_images` ~L277, `collect_items_images` ~L316,
`collect_inline_image_refs` ~L386). `copy_footer_images` is
`#[cfg(not(target_arch = "wasm32"))]`, and quarto-core already depends on
`scraper`/`html5ever` and `lol_html` for native builds, so an HTML tokenizer
adds no new dependency in this code path.

The body collector (`transforms/resource_collector.rs` ~L434) skips
`RawInline`/`RawBlock` as well. **Body raw `<img>` has the same gap.** This
matches the documented rule and Q1 does not share it (Q1 finds resources by
parsing the rendered HTML with deno-dom).

**Repro:** `claude-notes/plans/footer-raw-html-img-investigation/repro/` (run
`q2 render` there). With `target/debug/q2` @ `4d966ea05`:

| Reference | Location | Copied? | Emitted `src` |
|---|---|---|---|
| `![md](/images/md-control.svg)` | footer center (control) | ✓ | `../images/md-control.svg` (rebased) |
| `<img src="/images/center-raw.svg">` | footer center text | ✗ | `/images/center-raw.svg` (verbatim) |
| `<img src="/images/item-raw.svg">` | footer item `text:` | ✗ | verbatim |
| `<img src="/images/bare-raw.svg">` | footer bare-string item | ✗ | verbatim |
| `<img src="/_extensions/demo/assets/ext-raw.svg">` | footer bare-string item | ✗ | verbatim |
| `<img src="/images/navbar-raw.svg">` | navbar item `text:` | ✗ | verbatim |
| `<img src="images/body-raw.svg">` | document body | ✗ | verbatim |
| `<img src="/images/missing-raw.svg">` | footer center, file absent | no Q-5-6 | verbatim |

The documented workaround works today. `repro-resources/` is the same project
with `project: resources: [images/*-raw.svg, _extensions/demo/assets/*.svg]`,
and every raw-referenced file is copied. `contributes.project` →
`project.resources` is in the extension fragment rebase table
(`FRAGMENT_PATH_PATTERNS`, `crates/quarto-core/src/project/mod.rs` ~L806), so
**the extension itself can probably ship its footer images** by contributing
`project: resources:` entries. That rebase is existence-driven, so it needs
literal file names rather than globs. Not yet verified end to end.

Even with copying fixed, a raw `src="/…"` stays root-absolute, so it still
breaks on a subdirectory deploy. Copying alone fixes the 404 only at a domain
root.

## Proposed phases (draft)

These depend on the answer to design question 1.

**If option A (keep the policy):**
- Phase 0: verify the extension-side fix (`contributes.project.project.resources`
  with literal file names) on a copy of the product-doc-theme example.
- Phase 1: product-doc-theme PR: switch the logo item to markdown and add
  `resources:` for the cookie images. Record the result in bd-0i6fii0k's plan.
- Phase 2: docs: add a footer example to `paths.qmd` (and possibly
  `docs/guides/authoring/extensions.qmd`) showing `resources:` for raw-HTML footers.
- Close bd-5r97wehd as working-as-designed (user's call).

**If option B (narrow exception for config regions):**
- Phase 0: failing tests in `website_post_render.rs` for raw `<img>` in
  center text, item `text:`, bare-string item, and a missing file (Q-5-6 at
  the raw node's span).
- Phase 1: collect `src` from `RawInline`/`RawBlock` (format `html`) in the
  footer collectors with a real tokenizer (`lol_html` or `html5ever`), not a
  regex, feeding the existing probe/copy/Q-5-6 path.
- Phase 2: possibly the navbar (needs a navbar equivalent of
  `copy_footer_images`).
- Phase 3: update `paths.qmd` so it no longer says raw HTML is invisible
  for config regions. Cross-reference fc5pvkcv.

**If option C (general raw-HTML resource discovery, body included):** this is
a separate design strand, since it reverses fc5pvkcv for the whole pipeline
and pulls in the preview twins. File it and do not do it under this bug.

## Open design questions for the user

1. **Policy.** fc5pvkcv decided that q2 does not look inside raw HTML, and
   `paths.qmd` documents that. Should this strand (A) keep the rule and be
   closed after an extension-side and docs fix, (B) make a narrow exception
   that copies and warns for raw `<img src>` in `_quarto.yml` footer/navbar
   regions only, without rewriting, or (C) reopen the question for the whole
   pipeline, body included?
2. **If B: which attributes?** Only `<img src>`, or also `<source srcset>`,
   `<link href>`, `<a href>` to static files? Each added attribute is another
   step down the path fc5pvkcv warned about. `<img src>` alone covers every
   known real case (product-doc-theme, Connect docs).
3. **If B: rewrite too?** Copy-only leaves `/…` root-absolute, so it breaks on
   subdirectory deploys. Should the exception also rebase `src` per page,
   making it a real HTML rewrite? Or should it copy only, and warn when a
   raw `src` starts with `/`?
4. **Extension route.** Should the extension contributing
   `project: resources:` be the recommended fix for product-doc-theme
   regardless of 1? It works on q2 today if the rebase handles it, and Q1
   ignores it harmlessly because it relocates `_extensions/` anyway.
5. **Body gap.** Body raw `<img>` is not copied either, by the same documented
   rule. Is that known and accepted, or should it get its own strand now
   (perhaps folded together with preview twins bd-k5rxujiy / bd-960vrfbm)?

## Risks / tradeoffs (draft)

- Option B leaves an inconsistency that is hard to explain: raw `<img>` is
  copied from `_quarto.yml` but not from a document body. The docs need to say
  exactly where the line is.
- A Q-5-6 can only point at the whole raw node's span, not the `src` attribute.
  That is acceptable, but coarser than the markdown path.
- WASM/preview: `copy_footer_images` is native-only, so option B does nothing
  for hub preview. That is the same split as today's markdown-image copy.

## Direction chosen 2026-10-27: a `q2` branch in product-doc-theme

Instead of a dual-tool spelling, the user chose to keep a Quarto 2 version of
the extension on a `q2` branch of posit-dev/product-doc-theme. Both installers
accept `org/repo@ref` and try a tag first, then a branch: Q1's
`githubTagUrlProvider` / `githubBranchUrlProvider` in
`src/extension/extension-host.ts`, and q2's `parse_github` in
`crates/quarto-source-fetch/src/target.rs`. So
`q2 add posit-dev/product-doc-theme@q2` installs the branch. Caveat: a git tag
named `q2` would win over the branch.

Because the branch is q2-only, the cookie item uses the simpler spelling
(`[imgs](#){#open_preferences_center title=…}`, no item `href`). That keeps the
id on the `<a>`, so the open question about the consent vendor's click handler
no longer applies.

Local branch `q2` in `~/repos/github/posit-dev/product-doc-theme` (commit
`6e4d99c`, **not pushed**): footer → markdown images, `grid.qmd` `<style>` →
`{=html}` block, `website.title` `<small>` → `` `<small>`{=html} ``, README
(install `@q2`, a "Quarto 2 branch" section, markdown footer snippet,
`q2 preview`). The example renders 6/6 pages with no errors or warnings
(baseline: 5/6, 9 parse errors, 25 warnings). A fresh site that installs from
the local clone with `q2 add` and follows the README footer snippet renders
cleanly, and all four images are copied.
