---
title: 'Custom project type: extension-fragment navbar logo doubled `_extensions/` prefix (bd-74ktvzv7)'
date: 2026-10-22
description: 'Paths and hrefs that a `contributes.project` fragment sets in `website.*` get mis-resolved: the navbar logo picks up a doubled `_extensions/<ext>/` prefix, hrefs resolve against the extension dir, and the favicon is wrong in subdirectories. This plan sketches a fix that treats fragment values as project-root-relative.'
status: in-progress
braid:
  strand: bd-74ktvzv7
---

**Branch:** `braid/bd-74ktvzv7-navbar-logo-prefix-doubled` (topic branch in the main checkout, based on `main` @ `7584d6276`)
**Go-ahead given 2026-10-22.** Decisions are recorded under § Decisions.

## Triage verdict

**Ready to design.** The bug reproduces at HEAD with a minimal fixture. It is
broader than the strand says, though: the same root cause also breaks fragment
navbar hrefs, and a second, related cause breaks the favicon in
subdirectories. The fix shape needs a short design decision (Q1–Q3) before
implementation.

## Issue context

bd-74ktvzv7 (P1 bug, filed 2026-10-22 by Claude Fable 5.1 out of the
product-doc-theme assessment). When an extension's `contributes.project` sets
`website.navbar.logo` to a bundled file, the `<img src>` comes out as
`_extensions/posit-docs/_extensions/posit-docs/…` and 404s. The merged config is
correct, so the bug is in render-time resolution. The strand's analysis
holds: `rebase_fragment_paths` (`crates/quarto-core/src/project/mod.rs:823`)
rewrites the value but keeps its `_extension.yml` span. Then
`navbar_generate.rs:121–145` calls `resolve_metadata_path`
(`transforms/navigation_href.rs:769`), which joins the value onto the
directory of its source file. The source file is resolvable because
`metadata_merge.rs:260` registers extension manifests in the per-doc
`SourceContext` (bd-r64mj1aa).

Acceptance criteria: single-prefix src/href for `navbar.logo`, `logo-href` and
`sidebar.logo`, a regression test, and the product-doc-theme fixture renders
with no broken navbar logo.

## Dependency graph

- **discovered-from**: bd-0i6fii0k (in_progress), the product-doc-theme (posit-docs)
  assessment. This strand is its finding F1. The assessment plan
  `claude-notes/plans/2026-10-21-product-doc-theme-q2.md` lives on branch
  `braid/bd-0i6fii0k-product-doc-theme-q2` (commit 9f3c57e44), not on `main`.
  Sibling findings are F3 bd-t3n1nghb, F4 bd-rfz4e3bw, F6 bd-7u2xu4gp,
  F7 bd-hfghrqvh, F8 bd-3z5bubrf, F9 bd-f5l0n6oa and F10 bd-2pmjmjmj. None of them
  touch this code path.
- No blocks or related edges.
- Not linked, but relevant:
  - **bd-ht0n** (open), "\[websites\] Sidebar logo / subtitle / header / footer".
    `quarto_navigation::Sidebar` has no `logo` field, so q2 never renders
    `website.sidebar.logo` at all. The acceptance criterion "sidebar.logo
    emits a single-prefix src" cannot be met until bd-ht0n lands.
  - **bd-r64mj1aa** registered manifests in the SourceContext. That made
    the bug reachable: before it, the manifest FileId lookup failed and
    `resolve_metadata_path` fell through to `raw`.

## What the code looks like today

All the paths in the strand still exist with the shape it describes.
Repro: `claude-notes/plans/navbar-logo-prefix-doubled-investigation/`
(fixture + README with the observed output table). Summary:

1. **Navbar logo doubled** (the reported bug) at every page depth.
2. **Fragment navbar hrefs resolve against the extension dir.** `href: index.qmd`
   in the fragment becomes `_extensions/x/index.qmd`, raises Q-13-2 and
   produces a dead link. This has the same root cause, and also applies to
   sidebar hrefs, footer hrefs and anything else that goes through
   `resolve_metadata_path`. Quarto 1 treats non-file fragment strings as
   project-relative (`resolveExtensionPaths` → `toInputRelativePaths` in
   `external-sources/quarto-cli/src/extension/extension.ts:450` only rewrites
   strings that exist under the extension dir).
3. **Favicon wrong in subdirectories.** `sub/page.html` gets `../../_extensions/…`.
   The strand's "the favicon is correct" holds only at the project root. The cause is
   separate from (1): rebased leaves become `ConfigValueKind::Path`, which the
   per-document merge rewrites to document-relative (`../_extensions/…`), and
   the favicon consumer then page-relativizes it a second time. A `_quarto.yml`
   favicon stays a plain scalar and renders `../logo.svg` correctly. Once (1)
   is fixed, the navbar logo inherits this bug too, because it is also a `Path` leaf.
4. **`logo-href` is right by accident.** It is not in `FRAGMENT_PATH_PATTERNS`, so
   the raw `assets/logo.svg` gets resolved against the extension dir. Fixing (1)
   generally would break it unless it is added to the rebase table.
5. **Sidebar logo is unimplemented** (bd-ht0n).

## Proposed phases (draft)

- **Phase 0 — Failing tests.** Turn the fixture into a test, either a smoke-all
  fixture under `crates/quarto/tests/smoke-all/extensions/` or an integration test in
  `crates/quarto-core/tests/integration/extension_metadata.rs`. It asserts the
  root and `sub/` outputs for favicon, navbar logo, logo-href and the navbar item
  href, and that no Q-13-2 is raised. Add unit tests for `resolve_metadata_path` with a
  manifest-sourced value.
- **Phase 1 — Fragment values are project-root-relative at render time.**
  `resolve_metadata_path` returns `raw` when the value's source file is an
  extension manifest (fix option (a), broad form; see Q1). This fixes (1) and (2).
- **Phase 2 — Stop document-relativizing `website.*` rebased leaves.** Emit
  rebased `website.*` leaves as plain string scalars (project-root-relative,
  as if authored in `_quarto.yml`). Keep `ConfigValueKind::Path` for the
  `format.*` leaves, whose consumers want document-relative paths. This fixes (3).
- **Phase 3 — `logo-href`** added to `FRAGMENT_PATH_PATTERNS` so a bundled-file
  value is rebased explicitly instead of being right by accident (fixes (4)).
- **Phase 4 — Verify against product-doc-theme** (strand's Fixture section),
  with headless Chromium `naturalWidth > 0` on the navbar logo at root and in a
  subdirectory.
- **Phase 5 — Changelog** entry (and a docs note if the extension-authoring
  guide says how fragment paths resolve).

## Decisions (2026-10-22)

1. **Broad short-circuit.** `resolve_metadata_path` returns `raw` for every value
   whose source file is an extension manifest. The more general fix is to defer
   resolution and carry the base directory with the value, so that an
   extension's (or any) subdirectory `_metadata.yml` resolves correctly. That
   is filed as follow-up **bd-0x1znhgb**.
2. **`contributes.metadata` / `contributes.formats` included.** The broad check
   is the desired behaviour for them too.
3. **Sidebar logo dropped** from this strand's acceptance criteria. bd-ht0n has
   a note to cover the extension-fragment case with this fixture.
4. **The subdirectory favicon fix (Phase 2) is in scope.**

## Open design questions for the user (answered, see § Decisions)

1. **Scope of the `resolve_metadata_path` short-circuit.** Narrow it to
   `ConfigValueKind::Path`-rebased values (the strand's (a), narrow form), or
   apply it to every value whose source file is an `_extension.yml`? I recommend the
   broad form. The narrow form leaves bug (2) in place: fragment navbar and
   sidebar hrefs keep resolving against the extension dir, and
   Quarto 1 treats them as project-relative.
2. **`contributes.metadata` too?** The broad check also catches hrefs that come from
   `contributes.metadata` / `contributes.formats` manifests. Should those also be
   project-root-relative? Quarto 1 makes them input-relative via the same
   `toInputRelativePaths` mechanism. Or should we gate the short-circuit on
   "came from a `contributes.project` fragment" specifically? The second
   option would need the merge to record that origin somewhere.
3. **Sidebar logo acceptance criterion.** It can't be met until bd-ht0n
   implements `sidebar.logo`. Should I drop it from this strand and add a
   note on bd-ht0n ("when you implement it, cover the extension-fragment case,
   see the bd-74ktvzv7 fixture"), or make bd-74ktvzv7 blocked-by bd-ht0n?
4. **Phase 2 in scope here?** The subdirectory favicon bug (3) is a separate
   cause, but the navbar-logo fix exposes it at depth. Should I fix it in this strand
   (recommended, since the acceptance criteria want the logo to load on every page)
   or file it separately?

## Risks / tradeoffs (draft)

- Phase 2 touches the bd-of20unsb / rebase contract shared with
  `contributes.formats` marking. Splitting `website.*` from `format.*` leaves
  needs care that `project.resources` / `brand` (also in the table) keep
  whatever form their consumers expect. Audit each table entry's consumer.
- The broad Q1 form changes behaviour for every manifest-sourced href. That is
  probably only fixes in practice, but `contributes.metadata` with doc-relative
  intent (if any extension relies on it) would shift.
- Embedded built-in extensions rebase to absolute paths (`rebase_candidate`
  fallback). The short-circuit must not turn those into broken page-relative
  links. Check whether any built-in extension contributes `website.*` paths.

## Implementation log (2026-10-22)

- **Phase 0.** Added `extension_fragment_logo_favicon_and_logo_href_single_prefix`
  and `extension_fragment_navbar_href_resolves_against_project_root` to
  `crates/quarto-core/tests/integration/metadata_path_resolution.rs`. Both
  failed for the expected reasons: the doubled logo src, and the
  `_extensions/x/index.qmd` href. Also added the unit test
  `metadata_path_extension_manifest_is_project_root_relative`.
- **Phase 1.** `resolve_metadata_path` resolves values whose source file is an
  `_extension.yml` inside the project against the project root, so `/`
  stripping and `..` normalization match `_quarto.yml` values. A manifest
  outside the project (embedded built-in) keeps the old return-`raw`
  behaviour, so absolute rebased paths are not mangled. No built-in extension
  contributes a project type today. TODO points at bd-0x1znhgb.
- **Phase 2.** Split `FRAGMENT_SITE_PATH_PATTERNS` (`website.favicon`,
  `navbar.logo`, `navbar.logo-href`, `sidebar.logo`) out of
  `FRAGMENT_PATH_PATTERNS`. Site leaves are rebased in place as plain string
  scalars (span kept). `format.*` / `project.*` / `brand` leaves stay `Path`.
  `custom_project_type::extension_favicon_and_logo_rebased_to_project_root` now
  asserts the scalar kind and covers `logo-href`.
- **Phase 3.** `logo-href` is in the site table (above).
- **Phase 4.** The repro fixture renders correct links at root and `sub/`.
  Headless Chromium shows `naturalWidth > 0` for the navbar logo on the fixture
  (root + sub) and on product-doc-theme `index.html` / `admin.html`. The
  product-doc-theme diagnostics are unchanged versus the old binary: Q-13-2
  for the unparsable grid.qmd (F5), Q-2-9 ×24 (F9) and Q-2-47. Its remaining
  broken images are the footer's raw `/_extensions/…` `<img>` tags, which are
  the extension-side finding F2.
- **Phase 5.** Added a paragraph on non-bundled fragment paths to
  `docs/guides/authoring/extensions.qmd` § Bundled files, and a
  hub-client changelog entry.
