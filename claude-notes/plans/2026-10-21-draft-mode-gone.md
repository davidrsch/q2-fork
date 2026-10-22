---
title: 'Websites: draft-mode (gone default) — Quarto 1 parity (bd-2hy80x1j)'
date: 2026-10-21
description: 'Port Quarto 1''s website draft-mode option (gone / unlinked / visible) and website.drafts list to q2: draft-mode resolved per document through ordinary metadata merging, one draft predicate, unlinking across nav/links/listings/sitemap/search/llms, blank noindex pages for gone drafts, and authoring views always visible.'
status: in-progress
braid:
  strand: bd-2hy80x1j
---

**Status:** approved 2026-10-22; in progress on `braid/bd-2hy80x1j-websites-draft-mode-gone`.

## Goal

Support `draft-mode` in q2 websites with Quarto 1 semantics. **`gone` is
the default.**

| mode | page output | banner | linked from nav / body links / listings / search / sitemap / llms |
|---|---|---|---|
| `gone` (default) | blank `noindex` page at the draft's URL | no | no |
| `unlinked` | fully rendered | yes | no |
| `visible` | fully rendered | yes | yes |

The `draft-mode` mode applies only to **reading views**: `q2 render` and
publishing. **Authoring views** always behave as `visible`. Authoring views
are every hub-client render (including `q2-html-render` and the
printable/export view) and `q2 preview --static`.

The strand also adds `website.drafts` (a list of globs that mark pages as
drafts).

User docs: `external-sources/quarto-web/docs/websites/website-drafts.qmd`.

## Where we are today

q2 already has `draft: true`. It can be set in front matter or in
`_metadata.yml`, and is folded into `DocumentProfile.draft`
(`document_profile.rs:728/1128`). q2 has no `draft-mode` and no
`website.drafts`. Its effective behaviour is Q1's `visible` mode, with
exclusion added by hand in a few places. The full audit is
`claude-notes/research/2026-08-19-draft-visibility-audit.md`.

| site | today | Q1 under `gone`/`unlinked` |
|---|---|---|
| page body + banner (`transforms/draft_alert.rs`) | full body + banner always | `gone`: emptied, no banner |
| auto sidebar (`sidebar_auto.rs:219,228,246`) | drafts always dropped | dropped (kept in `visible`) |
| explicit navbar/sidebar/footer entries (`navigation_href.rs:184`, `NavigationItem` has no `draft`) | **linked** | dropped |
| prev/next, breadcrumbs | follow sidebar | follow sidebar |
| body links (`link_rewrite.rs` → `navigation_href.rs:352`) | **linked** (bd-p4sc) | `<a>` replaced by its content |
| listings + RSS (`listing_generate.rs:570` `item_visible` stub = `true`) | **included** | dropped |
| sitemap (`website_post_render.rs:499-580`) | **included** (bd-4zdf) | dropped |
| search (`search/capture.rs:96`) | always excluded | excluded (included in `visible`) |
| llms (`llms.rs:154` `profile_has_companion`) | always excluded | excluded (included in `visible`) |
| alias stubs (`aliases.rs:340`) | never written for drafts | n/a in Q1 (q2-only behaviour; keep) |

This strand absorbs bd-zeormbsa (Phase 1), bd-p4sc and bd-4zdf (both
Phase 4), and bd-98lngao9 (Phase 6). It supersedes bd-w0o9. Each of those
is closed when its phase lands (D6). bd-4c7n9o1h (banner in reveal.js
decks) is out of scope and unaffected.

## How Quarto 1 does it

Paths are under `external-sources/quarto-cli/src/`.

- **Mode resolution** (`project/types/website/website-utils.ts:47-59`).
  `projectDraftMode` returns `visible` when `project.previewServer` is set.
  Otherwise it returns `website.draft-mode`, or `gone` if that is unset.
  `isDraftVisible` is true for `visible` (and for an unschematized
  `preview` value). `unlinked` means "anything else".
- **Draft status.** A page is a draft through front matter or directory
  metadata (`project-index.ts:110`). It is also a draft if its path
  exactly matches an entry in `website.drafts` (`website.ts:384-393`,
  `website-utils.ts:35-45`). The source has a "TODO: convert to glob"
  there.
- **`gone` body** (`resources/filters/normalize/draft.lua`). For HTML that
  is not slides, it sets `doc.blocks = {}` and adds `<meta
  name="quarto:status" content="draft-remove">`. Other modes get
  `content="draft"`, which drives the banner (`format-html.ts:901-927`).
- **`gone` page** (`website-draft.ts:12-27`). A post-processor runs
  `documentElement.innerHTML = ""`. The `.html` is still written,
  essentially empty.
- **Unlinking:**
  - `resolveProjectInputLinks` (`website-utils.ts:61-122`) replaces each
    `<a>` with its inner HTML.
  - The nav templates (`navitem.ejs`, `sidebaritem.ejs`,
    `nav-footer-navitem.ejs`) gate on `draftMode === "visible" ||
    item.draft !== true`.
  - Listings: `website-listing-read.ts:1106`.
  - Search: `website-search.ts:180-196`.
  - Sitemap: `website-sitemap.ts:176-179`.
  - llms: `website-llms.ts:56-83,338-365`.
- **Schema** (`resources/schema/definitions.yml:1213-1227`): `drafts:
  maybeArrayOf path` and `draft-mode: enum [visible, unlinked, gone]`.
- **Q1 is inconsistent here.** Navigation, listings, search and sitemap
  read a single project value from `website.draft-mode`. The Lua body
  filter, however, reads the hoisted value from *document* metadata, so
  front matter can change the body but not the linking. Under `quarto
  preview`, an explicit `draft-mode: gone` still blanks pages. q2 replaces
  both behaviours with the uniform rule below.

## Design

### Metadata discipline: draft-mode is ordinary merged metadata

q2 resolves metadata by merging every layer uniformly. The layers, lowest
to highest precedence, are:

1. project `_quarto.yml`
2. extension contributions
3. the `_metadata.yml` chain from the project root down to the document's
   directory
4. document front matter
5. `-M` runtime metadata

The merge is done by `stage/stages/metadata_merge.rs` over
`metadata_layers::collect_metadata_layers`, and produces each document's
`ast.meta`. The `website:` subtree merges like any other key. Website keys
that may be spelled either way are read with
`quarto_config::resolve_website_value(meta, key)`: either `<key>` or
`website.<key>`, with the top-level form winning. Navbar, sidebar and
page-footer already use it.

`draft-mode` follows that discipline exactly:

- **One reader.** `fn resolve_draft_mode(meta: &ConfigValue) ->
  (DraftMode, Option<Diagnostic>)` calls `resolve_website_value(meta,
  "draft-mode")` and parses the result. It defaults to `Gone` and warns
  on unknown values (D3).
- **It runs on merged metadata.** Every placement then behaves
  identically:
  - top-level `draft-mode:` in `_quarto.yml`
  - `website: draft-mode:` in `_quarto.yml`
  - either spelling in `posts/_metadata.yml`
  - either spelling in a document's front matter
  - `-M draft-mode=unlinked`
- **Indistinguishable placements.** A top-level `draft-mode` in
  `_quarto.yml` is equivalent to every document declaring it. A
  `_metadata.yml` scopes it to a directory, and front matter scopes it to
  one page.
- **The result is stored on the profile.** `DocumentProfile` gains
  `draft_mode: DraftMode`, set next to `draft` at
  `document_profile.rs:1128` from the same merged metadata. Pass 1 builds
  every profile before any page renders. So every consumer (nav and links on other pages,
  listings, sitemap, search, llms) reads the *target*
  page's own resolved mode from `ProjectIndex`. There is no
  project-global lookup, and none of those consumers reads raw
  `_quarto.yml`.
- **Whose mode governs.** A page's mode governs both how that page renders
  and whether other pages may link to it. Example: `draft-mode: unlinked`
  in `posts/_metadata.yml` means draft posts render with a banner and
  nothing links to them. Draft pages elsewhere stay `gone`. One value per
  page, used consistently everywhere.
- **Cache correctness is free.** The profile cache key already hashes the
  `_quarto.yml` bytes, the `_metadata.yml` chain and the `-M` metadata
  (`project/cache_key.rs`). Bump `DOCUMENT_PROFILE_VERSION` for the new
  field.
- **Rule for every consumer.** Do not read `website.draft-mode` from
  `project.config.metadata`. That is the non-uniform path some post-render
  code uses for other keys (`website_config.rs:28-32`), and it would make
  directory and front-matter overrides silently ineffective for sitemap
  and llms.

`website.drafts` (D1) follows the same discipline:

- **Reading.** It is read from merged metadata with
  `resolve_website_value(meta, "drafts")`. Arrays concatenate across
  layers, so a `_metadata.yml` can add patterns.
- **Matching.** Each pattern is resolved relative to the YAML file that
  declared it, using its `SourceInfo`. This follows the source-location
  precedent of bd-qor9a (`navigation_href::resolve_metadata_path`) and of
  listing `contents:` globs (bd-v7ixzsp5). The result is matched with
  `crate::glob` against the document's project-relative source path.
- **Effect.** `profile.draft = draft: true || any pattern matches`.
- **Scope consequence.** A pattern only affects documents whose merged
  metadata contains it. A pattern in `_quarto.yml` (or `metadata-files`,
  or a profile) reaches every page, as in Q1. A pattern in
  `posts/_metadata.yml` only reaches pages under `posts/`, the same scope
  rule as any other key there. Document this rather than special-case it.

### Authoring views vs reading views

This introduces a distinction q2 has not formalized before. Some renders
produce **reading views**: what the site's audience sees (`q2 render`,
publish). Others produce **authoring views**: what an author sees while
working (every hub-client render, including `q2-html-render` and the
printable/export view, and `q2 preview --static`). `draft-mode` governs
reading views only. Authoring views always behave as `visible`.

Mechanism:

- Add an explicit `RenderAudience { Reading, Authoring }`, defaulting to
  `Reading`. It is threaded like `RenderArgs.execution_policy`:
  `commands/render.rs:113` → `render_to_file.rs:131` → pipeline config →
  `RenderContext` and `ProjectIndex`.
- Do not infer it from `PipelineProfile::HtmlPreview`. That covers only
  `q2-preview`/`q2-sandboxed-preview`, and misses `q2-html-render`, the
  printable view and `q2 preview --static`.
- Callers:
  - `q2 preview --static` and the WASM hub-client render entry points set
    `Authoring`.
  - `q2 render` and publish leave the default.
- The audience is **not** metadata and never enters profiles. Profiles
  stay cacheable across audiences, and the override is applied in the
  predicate.
- Document the concept briefly in the module docs of the new drafts
  module. The draft policy is its first consumer, and later
  authoring/reading splits can reuse it.

### One predicate

New module `crates/quarto-core/src/project/drafts.rs`:

```rust
pub enum DraftMode { Gone, Unlinked, Visible }
pub enum RenderAudience { Reading, Authoring }

/// The page's effective mode for this render.
fn effective_mode(p: &DocumentProfile, audience: RenderAudience) -> DraftMode {
    match audience { Authoring => Visible, Reading => p.draft_mode }
}
pub fn is_linkable(p, audience) -> bool { !p.draft || effective_mode(p, audience) == Visible }
pub fn is_gone(p, audience) -> bool     {  p.draft && effective_mode(p, audience) == Gone }
pub fn shows_banner(p, audience) -> bool { p.draft && !is_gone(p, audience) }
```

- **`ProjectIndex` exposes it.** `ProjectIndex` stores the
  `RenderAudience` it was built for and exposes
  `index.is_linkable(profile)` and `index.linkable_profiles()`, the shape
  bd-zeormbsa proposed. Pass 2 transforms reach it through
  `ctx.project_index`. Post-render hooks (sitemap, llms, search writer,
  aliases) receive the index directly.
- **Test constructors are unchanged.** The audience is set through a
  builder, so the many test `ProjectIndex::new(profiles)` calls keep
  working (default `Reading`).
- **Non-website projects.** The view also records whether the project
  is a website. Outside websites, `gone` is clamped to `unlinked`
  (D5): such drafts keep their banner and are never blanked, which
  preserves Q1's "banner for a lone `draft: true` document". The page's
  own transform takes the view from the index it renders with; a
  render with no index never blanks.
  *(Implementation note, Phase 1: the website flag lives on
  `DraftView` next to the audience, set by the orchestrator from
  `ProjectContext::project_kind()`.)*
- **Guard test** (from bd-zeormbsa): a source-scan test asserting that no
  non-test code outside `drafts.rs` and `document_profile.rs` reads
  `.draft` or `.draft_mode`.

### Gone pages: what gets written

Q1 writes an empty `<html></html>` shell. q2 has no DOM post-processor
and must not grow one (AGENTS.md).

1. **Normalization phase.** `DraftAlertTransform` (renamed
   `DraftTransform`) consults the predicate. This is the gate that
   `draft_alert.rs:70-77` already anticipates.
   - `gone`: clear `ast.blocks`, skip the banner, and record `gone` for
     the template stage.
   - otherwise: today's banner path (`content="draft"`).
2. **Template stage** (`stage/stages/apply_template.rs`). For a gone page,
   emit a fixed constant shell instead of the full template, so no title,
   nav, scripts or CSS leak:
   ```html
   <!DOCTYPE html>
   <html><head><meta charset="utf-8"><meta name="robots" content="noindex"><meta name="quarto:status" content="draft-remove"></head><body></body></html>
   ```
3. **Execution is NOT skipped (D7).** Engine execution runs as usual, since
   its side effects may be expected by post-render scripts and similar.
   Because the blocks are cleared in Normalization, after execution, this
   falls out naturally.
4. **Resources** (images etc. referenced by the draft) are still copied,
   as in Q1. This is a known, harmless gap.

Pass 1 profiles are computed before Normalization. So other pages still
see the draft's title, `draft` and `draft_mode`, which they need to
identify and drop it.

### Unlinking (`gone` and `unlinked`)

Every consumer goes through `index.is_linkable(profile)`.

1. **Auto sidebar.** Replace the three `.filter(|p| !p.draft)` calls in
   `sidebar_auto.rs`. Drafts now appear in `visible` mode and in
   authoring views. That is a deliberate change: the hub sidebar will
   list drafts, which is what Q1 preview does.
2. **Explicit nav items.**
   - Add `draft: bool` (meaning "not linkable") to
     `quarto_navigation::NavigationItem`. Set it where hrefs resolve
     against the index (`navigation_href.rs:153-200`,
     `sidebar_generate.rs`, `navbar_generate.rs`, `footer_generate.rs`).
   - Drop those items in the generate stages, mirroring Q1's per-item
     `navitem.ejs` gate.
   - Prev/next and breadcrumbs derive from the finished sidebar, so they
     inherit this.
   - Q1 drops only the item and keeps an emptied section. Match that, and
     test a section whose every entry is a draft.
3. **Body links (bd-p4sc).**
   - In `link_rewrite.rs`, when a link resolves to a non-linkable
     profile, replace the `Inline::Link` with its content inlines.
   - `visit_inlines` (`:294`) currently mutates in place via `iter_mut`,
     so it must splice instead.
   - Keep Q1's opt-out (`data-noresolveinput`, or q2's equivalent).
4. **Listings + RSS.**
   - `item_visible` (`listing_generate.rs:570-576`) becomes
     `index.is_linkable`.
   - Q-12-19 interaction (flagged on bd-zeormbsa): count draft matches
     toward `matched_any` *before* filtering, so a glob matching only
     drafts does not warn "matched nothing".
   - Record items pointing at drafts are dropped silently (Q1 parity).
5. **Sitemap (bd-4zdf).** `write_sitemap` iterates `linkable_profiles()`.
6. **Search and llms.**
   - `profile_is_indexable` and `profile_has_companion` switch from
     `!profile.draft` to the predicate, which makes drafts indexable in
     `visible` mode.
   - Also audit the unchecked llms lookups (`llms_post_render.rs:442,602`)
     and `link_rewrite.rs:443`.
7. **Aliases.** Keep "drafts write no redirect stubs" in every mode (a
   q2-specific, documented behaviour, `aliases.qmd:91-95`). This goes
   through a `drafts::is_draft(profile)` accessor so the guard test holds.
8. **Enrichment** (`navigation_enrich.rs:57`). No change needed once
   item 2 lands.

### Preview↔render parity tests

Render (`Reading`) and hub preview (`Authoring`) now legitimately differ
for draft fixtures. Parity harnesses must either render with
`RenderAudience::Authoring` or exclude draft fixtures. Check
`ts-packages/preview-renderer` parity tests and the
`preview-render-parity` fixture set.

## Phases

All work is on topic branch `braid/bd-2hy80x1j-websites-draft-mode-gone`. Each phase
is one commit or a small group, with tests.

1. **Policy + config (bd-zeormbsa).** *Done 2026-10-22.*
   - Add `drafts.rs` (`DraftMode`, `RenderAudience`, predicate,
     `resolve_draft_mode`).
   - Add `DocumentProfile.draft_mode` and bump the profile version.
   - Make `ProjectIndex` audience-aware.
   - Route the existing `.draft` readers (auto sidebar, search, llms,
     aliases) through the predicate.
   - Add the warning code for unknown `draft-mode` values to
     `error_catalog.json`, with a `docs/errors/` page.
   - Add the guard test.
   - Tests: resolution from each placement (top-level and `website.` in
     `_quarto.yml`, `_metadata.yml`, front matter, `-M`), precedence
     between them, and an invalid value warning and falling back to
     `gone`.
2. **Authoring audience.** *Done 2026-10-22.* `RenderToFileOptions.audience`
   (applied by `ProjectPipeline::new`); `Authoring` is set by `q2 preview
   --static`, both hub-client WASM pipelines, the browser export
   (`pandoc_request`), book downloads, and the preview server's native
   PDF/download routes (`quarto-preview` `render_helper`).
   - Thread `RenderAudience` through `RenderArgs` → render options →
     pipeline → `RenderContext`/`ProjectIndex`.
   - Set it from `q2 preview --static` and the hub-client WASM entry
     points.
   - Integration test: `Authoring` overrides an explicit `draft-mode:
     gone`.
3. **Gone page output.**
   - Policy-aware `DraftTransform`, plus the constant noindex shell in
     `apply_template`.
   - Update `llms_txt.rs:568`, which asserts that drafts render fully.
   - Smoke-all website fixture: the draft output exists, contains
     `draft-remove` and `noindex`, has no `#quarto-draft-alert`, and the
     body text is absent.
   - A `_metadata.yml` fixture setting `draft-mode: unlinked` for one
     directory proves directory scoping.
4. **Unlinking.**
   - Nav items, body links (closes bd-p4sc), listings/RSS (with the
     Q-12-19 fix), sitemap (closes bd-4zdf), search, llms.
   - Smoke-all fixtures for `gone`/`unlinked`/`visible`, modelled on the
     quarto-web example (`navbar: [item1, item2]`, `item2` a draft).
   - Assertions: navbar item count, body link unwrapped, listing,
     sitemap, search.json.
5. **`website.drafts`.**
   - Read from merged metadata, resolve patterns against the declaring
     file, glob-match, and fold into `profile.draft`.
   - Tests: plain path, glob, pattern declared in `_metadata.yml`
     (relative to it), and a pattern via `metadata-files`.
6. **Docs + changelog (closes bd-98lngao9; closes bd-w0o9 as
   superseded).**
   - New `docs/guides/projects/drafts.qmd` covering:
     - `draft:` and `drafts:`
     - `draft-mode` and its per-directory/per-page scoping
     - the banner
     - authoring vs reading views
     - the alias behaviour
     - a sentence on spelling precedence across layers (Q-a), linking to
       the general metadata guide once bd-dia333nt lands
   - Fix the passing mentions in `aliases.qmd`, `llms-txt.qmd` and
     `website-search.qmd`.
   - Changelog entry calling out the **default behaviour change**:
     existing q2 sites with drafts will now publish blank draft pages
     and lose links to them.
   - File a follow-up strand for books (D5).

Final verification: `cargo xtask verify` (including the hub build, since
WASM entry points change), plus a manual `q2 render` and `q2 preview` of
a small draft website.

## Decisions (resolved 2026-10-22)

- **D1 — `website.drafts`: included, with globs.** Phase 5. Patterns
  resolve relative to their declaring YAML file (see the Metadata
  discipline section).
- **D2 — Gone-page shell: add `<meta name="robots" content="noindex">`.**
- **D3 — Where `draft-mode` is accepted: everywhere metadata is.**
  - Both spellings are accepted, via `resolve_website_value`.
  - It is read from per-document merged metadata, so `_quarto.yml`,
    `_metadata.yml`, front matter and `-M` all work and compose by the
    normal precedence.
  - The resolved value lives on each `DocumentProfile`. A page's mode
    governs both its rendering and its linkability.
  - Unknown values warn and fall back to `gone`.
- **D4 — Preview detection: explicit `RenderAudience`.** Hub-client
  (all renders, including printable/export) and `q2 preview --static` are
  authoring views and force `visible`.
- **D5 — Websites only.** File a follow-up for books.
- **D6 — Single strand.** Close bd-zeormbsa, bd-p4sc, bd-4zdf and
  bd-98lngao9 as their phases land; close bd-w0o9 as superseded.
- **D7 — Do not skip execution for gone pages.** Execution side effects
  may be expected by post-render scripts and similar.

## Follow-up questions (resolved 2026-10-22)

- **Q-a — precedence between spellings across layers.**
  `resolve_website_value` lets top-level win over `website.<key>` *after*
  merging. So a top-level `draft-mode: gone` in `_quarto.yml` beats
  `website: draft-mode: unlinked` in a higher-precedence `_metadata.yml`.
  This is the existing behaviour for navbar, sidebar and similar keys.
  - **Resolution:** the drafts docs page (Phase 6) gets a sentence or two
    on it, with a practical tip: use the same spelling in every layer.
  - The broader write-up is filed as **bd-dia333nt**. It covers a
    dev-doc on the metadata merging discipline, a user-facing guide, and
    whether to fix the wrinkle.
- **Q-b — `website.drafts` declared in front matter.** This is allowed.
  It is equivalent to `draft: true`, and no special case is needed.
