---
title: 'Table caption classes → Bootstrap table-* classes (bd-hfghrqvh)'
date: 2026-10-22
description: 'Map Q1''s Bootstrap table shorthands (.striped .hover .sm .responsive-md …) written in a table caption''s attribute block to table-* classes on the `<table>`, for unlabelled, #tbl-labelled and ::: {#tbl-} div-form tables.'
status: implemented  # PR open; see Implementation notes
braid:
  strand: bd-hfghrqvh
---

**Branch:** `braid/bd-hfghrqvh-table-caption-bootstrap-classes` (topic branch in the main checkout, based on `main` @ `c68647049`)

## Triage verdict

**Ready to design** (design questions answered 2026-10-22; see Decisions). The gap reproduces at HEAD, Q1's behaviour is small and
fully specified by two Lua filters, and the natural home
(`table_bootstrap_class.rs`) already exists. The strand's framing needs one
correction (below): the classes are **not** lost at parse time. A handful of
placement/scope questions remain.

## Issue context

P3 feature, label `parity`, filed 2026-10-22 by Claude Fable 5.1 while
assessing `posit-dev/product-doc-theme` on q2 (F7 in that assessment). Q1
rewrites `: caption {.striped .hover}` to
`<table class="table-striped table-hover …">`. The strand says q2 "drops the
attribute block entirely".

Acceptance criteria (verbatim): *A caption attribute block {.striped .hover}
yields `<table class="… table table-striped table-hover">` with the classes
removed from the rendered caption; all Q1 shorthands in table-classes.lua are
mapped; .sm adds small; non-shorthand classes are forwarded verbatim; works for
labelled and unlabelled captions; smoke-all fixture.*

## Dependency graph

- **discovered-from**: bd-0i6fii0k (product-doc-theme assessment, in
  progress). Its plan (`2026-10-21-product-doc-theme-q2.md`) lives on branch
  `braid/bd-0i6fii0k-product-doc-theme-q2`, not on `main`. The example site uses
  `{.striped .hover}` on its tables, so this is one of eight per-finding strands
  filed from it (F1 bd-74ktvzv7 has already merged as #903).
- No `blocks` edges and no children. Nothing waits on this.
- **Follow-ups filed (discovered-from this strand, 2026-10-22):**
  - bd-7zw93czl: all-format class normalization, as Q1 does (P4).
  - bd-44t9rqtp: crossref table caption is placed at the bottom instead of the top (P3).

## What the code looks like today

Repro files: `table-caption-bootstrap-classes-investigation/caption-forms.qmd`
and `div-form-and-small.qmd`. q2 and Q1 output side by side are in
`html-comparison.txt`.

**The parser does its job.** `pampa -t native` puts caption-attribute classes
on `Table.attr`, which matches Pandoc. Nothing is in the caption text, so
"remove the classes from the rendered caption" is already true:

```
Table ( "" , ["striped", "hover", "my-class"] , [] ) (Caption Nothing [ Plain [Str "Plain", Space, Str "caption"] ]) …
Table ( "tbl-one" , ["bordered", "sm"] , [] ) …
```

What goes wrong depends on the table form:

| Form | q2 today | Q1 |
|---|---|---|
| `: cap {.striped .hover .my-class}` | `<table class="striped hover my-class caption-top table">`, **unprefixed** | `table-striped table-hover my-class caption-top table` |
| `: cap {#tbl-one .bordered .sm}` | div `bordered sm quarto-float …`; `<table class="caption-top table">`, **classes not forwarded** | the div keeps `bordered sm`; the table gets `table-bordered table-sm small caption-top table` |
| `::: {#tbl-div .striped}` div form | div `striped …`; table bare | the div keeps `striped`; the table gets `table-striped` |
| `: cap {.responsive-md .primary}` | `responsive-md primary caption-top table` | `<div class="table-responsive-md"><table class="table-primary caption-top table">` |
| `: cap {.small}` | `small caption-top table` | `small table-sm caption-top table` |

Q1 handles this in two places:

- `quarto-pre/table-classes.lua` runs in the **pre** stage for **every
  format**:
  - It prefixes the shorthands `primary secondary success danger warning info
    light dark striped hover active bordered borderless sm responsive
    responsive-{sm,md,lg,xl,xxl}` with `table-`.
  - It adds `small` next to `table-sm`, and `table-sm` next to `small`.
  - On a `tbl` FloatRefTarget it forwards the float's classes and the caption
    classes, normalized, onto the inner table (or onto every table inside a
    non-table float, without descending into subfloats). The float keeps its
    unprefixed copies.
- `quarto-post/responsive.lua` `responsive_table()` is **HTML only**, in the
  post stage. It takes the first `table-responsive*` class off the table and
  wraps the table in `Div(.table-responsive*)`.

On the q2 side:

- `crates/quarto-core/src/transforms/table_bootstrap_class.rs` adds `table` and
  `caption-top`. It runs in the Finalization phase, is HTML only, and is
  excluded from the Pandoc path. It touches only `Table.attr`, and the doc
  comment cites bd-2c8rg.
- `float_ref_target.rs::maybe_wrap_bare_table_into_div` (bd-4ly7ne01) moves the
  *whole* attr, classes included, from a `#tbl-` table onto the new float Div.
  That is why labelled tables end up bare.

Incidental observation, not in scope: for crossref tables, q2 emits
`quarto-float-caption-bottom` and Q1 emits `quarto-float-caption-top`. See
Design question 6.

### Where floats live in the pipeline

The pipeline facts below are what decide the design:

- **`crossref-render` runs before `table-bootstrap-class`** (`pipeline.rs`:
  line 2624 vs. 2699, both in the Finalization phase of the HTML-only
  pipeline).
  - Until `crossref-render` runs, a `tbl` float is a
    `CustomNode("FloatRefTarget")`. Its `node.attr` holds *only* the
    author's classes (`bordered sm`, `my-class`).
  - `render_float_ref_target` then mixes those classes into the outer
    `Div(.quarto-float .quarto-figure …)`. By the time
    `table-bootstrap-class` runs, the author's classes can no longer be told
    apart from Quarto's own.
- **Floats render children-first.** `render_block` recurses into the custom
  node's slots before converting the node itself. So a parent float's
  content already holds its sub-floats as rendered `Div.quarto-float`
  blocks, and that class is the marker for "skip sub-floats".
- **`crossref-render` is in `Q2_PREVIEW_TRANSFORM_EXCLUDED`.**
  - In q2-preview, `FloatRefTarget` reaches React
    (`ts-packages/preview-renderer/src/q2-preview/custom/FloatRefTarget.tsx`)
    as a custom node.
  - `table-bootstrap-class` is *not* excluded. But `apply_to_block` has no
    `Block::Custom` arm, so it never reaches tables inside a float in the
    preview. They likely get no `table`/`caption-top` classes today, a
    pre-existing preview gap to confirm in Phase 4.
- **The table logic in `crossref-render` only looks at bare top-level tables.**
  - It drops the table's own caption (bd-4m2n6qf1) only for a
    `Block::Table` sitting directly in the float content.
  - It skips image-attribute harvesting only when `content.first()` is a
    `Block::Table`.

  If the responsive `Div` wrapper went in *before* `crossref-render`, the
  caption would render twice. So the wrapper has to stay late, as in Q1,
  where `responsive_table()` is a post filter.

## Decisions (user, 2026-10-22)

1. **HTML only**, in the HTML pipeline, as a reviewed divergence (needed for
   Posit's product docs). Q1's all-format normalization is filed as
   **bd-7zw93czl** (P4).
2. **Duplicate classes on labelled floats, as Q1 does.** The float `<div>`
   keeps the author's unprefixed classes, and the `<table>` gets normalized
   copies. Existing Q1 CSS that targets the div keeps working.
3. **Forward every float class verbatim**, not just recognized shorthands.
   Only shorthands are prefixed.
4. **The responsive wrapper is in scope:** `Div(.table-responsive*)` around
   the table, built from the first `responsive*`/`table-responsive*` class,
   which is removed from the table.
5. **Floats that are not a single table:** apply the classes to every nested
   table, and skip sub-floats.
6. **Caption location** (`-bottom` vs. Q1 `-top`) is filed separately as
   **bd-44t9rqtp** (P3).

## Design

There are two steps, because the float's own classes are only visible early,
while the wrapper has to go in late:

- **A. `table-float-class-forward`** is a new, small transform placed
  immediately **before** `crossref-render` in the Finalization phase (after
  `cross-chapter-crossref-resolve`). It could live as a second struct in
  `table_bootstrap_class.rs`, since it is the same feature. It runs in both
  render and preview.
  1. It walks the AST. For each `FloatRefTarget` with
     `plain_data.ref_type == "tbl"`, it appends `node.attr.1` (the author's
     classes, unnormalized) to every `Block::Table` in the `content` slot.
  2. It recurses through Div, Figure, list, and table-cell structure, but
     never into a nested `FloatRefTarget`.
  3. It deduplicates, so it is idempotent.
  4. It leaves `node.attr` alone, so the float div keeps its copies
     (Decision 2).
- **B. Extend `TableBootstrapClassTransform`**, which stays where it is (late,
  Finalization). For every `Block::Table` it:
  1. maps each class in Q1's shorthand list to `table-<x>`;
  2. adds `small` if `table-sm` is present, otherwise adds `table-sm` if
     `small` is present (Q1's `elseif`);
  3. adds `caption-top` and `table` as today;
  4. in `apply_to_blocks`, takes the first
     `responsive*`/`table-responsive*` class off the table and replaces the
     table with `Div(.table-responsive*) > Table`. Q1 matches the
     *unprefixed* `responsive-md` or an explicit `table-responsive-md`. By
     the time responsive.lua runs, table-classes.lua has already prefixed
     the class, so after mapping we match on `table-responsive*`;
  5. adds a `Block::Custom` arm that recurses into slots, so tables inside
     preview `FloatRefTarget` nodes are reached.

Q1 forwards the float classes *and then* runs its Table handler again on the
same tables. Its double normalization is harmless because `table-x` is not in
the shorthand list, and our single late pass gives the same result.

## Phases

- **Phase 0: tests first.**
  - [ ] Unit tests in `table_bootstrap_class.rs`:
    - every shorthand maps, and a non-shorthand class (`my-class`) passes
      through;
    - `.sm` gives `table-sm small`, and `.small` gives `small table-sm`;
    - the transform is idempotent, including the wrapper: a second run does
      not double-wrap;
    - `.responsive-md` wraps and leaves `table-responsive-md` off the table;
    - a table inside a `Block::Custom` slot is reached.
  - [ ] Unit tests for the forward pass:
    - a single table in a tbl float gets the float's classes;
    - a layout of two tables in a tbl float: both get them;
    - a nested tbl sub-float: its table gets *only* the sub-float's classes;
    - a `fig` float: untouched;
    - idempotent.
  - [ ] Smoke-all fixture
    `crates/quarto/tests/smoke-all/markdown/table-caption-bootstrap-classes.qmd`
    with the five forms from `html-comparison.txt`, plus a multi-table tbl
    float. Use `ensureFileRegexMatches` anchored on `<table class="…"` and on
    the float `<div id="tbl-…" class="…"`. Assert that the float div still
    has `bordered sm`.
  - [ ] Confirm the tests fail at HEAD.
- **Phase 1: normalization and `sm`/`small` in `TableBootstrapClassTransform`**
  (fixes unlabelled captions).
- **Phase 2: the responsive wrapper**, late, in the same transform.
- **Phase 3: the forward pass**, registered before `crossref-render`.
  - [ ] Add the name to whatever exact-list drift validator `pipeline.rs`
    keeps (the "exclude-list superset trap" tests around line 4415).
  - [ ] It must not be in `Q2_PREVIEW_TRANSFORM_EXCLUDED` or
    `PANDOC_TRANSFORM_EXCLUDED` unless the validator demands it. Decide by
    reading the lists.
- **Phase 4: preview and wrap-up.**
  - [ ] Add the `Block::Custom` recursion.
  - [ ] Check in q2-preview (`/run` or the preview-render-parity skill) that
    float tables get `table caption-top table-striped …`.
  - [ ] Update the module doc and the pipeline comment (cite bd-hfghrqvh).
  - [ ] Re-render the product-doc-theme example and check its tables.
  - [ ] `cargo xtask verify`.
  - [ ] Add a braid comment on bd-hfghrqvh recording the HTML-only
    divergence (bd-7zw93czl).

## Risks / tradeoffs

- **The wrapper changes the AST shape for late transforms.** Anything after
  `table-bootstrap-class` that pattern-matches a float's content as a bare
  `Table` would now see a `Div`. Grep the transforms registered after it in
  Phase 2.
- **The preview `Custom` arm widens the existing transform's reach**, which
  could change preview snapshots for float tables. That is intended, but
  review the diffs.
- **Fixture regexes:** the float `<div>` and the `<table>` both carry
  classes, so anchor every pattern on the element name.
- **Verify environment:** the pre-flight run failed one test because the
  local `hub-client/dist-preview-embed/` is stale. It has no `pdfjs/` folder
  (built on Sep 9, before `7b9ac798b`), so `editor_ui_serves_pdfjs_from_the_viewer_bundle`
  gets a 404. Rebuild the embed before the final verify.

## Implementation notes (2026-10-22)

Phases 0–4 are done on this branch. The design above held. These details
were added or changed along the way:

- **Forward pass:** `TableFloatClassForwardTransform`
  (`table-float-class-forward`) lives in `table_bootstrap_class.rs`.
  - It is registered just before `crossref-render`.
  - It is listed in `PANDOC_TRANSFORM_EXCLUDED` and bucketed as B4, like
    `table-bootstrap-class`.
  - It is *not* preview-excluded, and it was added to the three exact-order
    pipeline tests.
  - It walks with `ast_walk::block_children_mut` / `ListVisitor` and stops at
    nested `FloatRefTarget` nodes.
- **`apply_to_blocks`** now walks with `ast_walk::for_each_block_list_mut`, so
  it also reaches custom-node slots. This fixed the pre-existing preview gap,
  which I confirmed: on `main`, a labelled table in the q2-preview AST had
  classes `[]`, not even `table`.
  - Pinned by `crates/quarto-core/tests/integration/table_bootstrap_classes_preview.rs`.
  - All three tests in that file fail against the old transform code.
- **Scope addition (parser).** product-doc-theme writes its caption directly
  under the table with no space after the colon:
  `:This table … {.striped .hover}`. Pandoc accepts that form. In q2 it goes
  through the caption-as-last-row fixup in `pampa` `postprocess.rs`, and the
  attribute block was dropped even earlier:
  `process_pipe_table_cell` discarded every `IntermediateAttr`.
  - Cells now keep the attribute as an `Inline::Attr`.
  - `process_pipe_table` strips it from every row except a final single-cell
    row starting with `:`, and re-trims those cells, so ordinary cells behave
    exactly as before.
  - The fixup then lifts it onto the table through the new
    `take_caption_attr` helper. The blank-line caption path now uses the same
    helper (the inline merge code was extracted into it).
  - Tests: `test_adjacent_no_space_caption_attr_merges_into_table` and
    `test_attr_in_ordinary_table_cell_is_still_dropped` in
    `crates/pampa/tests/integration/test_attr_source_parsing.rs`, plus the
    `:Adjacent caption` case in the smoke fixture.
- **product-doc-theme example:** `user.html` now renders
  `<table class="table-striped table-hover caption-top table">`, the same as
  Q1.

**Strands filed (all discovered-from bd-hfghrqvh):**

- bd-7zw93czl: do the class normalization for all formats.
- bd-44t9rqtp: float caption is placed at the bottom instead of the top.
- bd-ymyeef2e: a multi-table float drops the captions its inner tables carry. The
  bd-4m2n6qf1 elision is too broad. Because of this, the fixture does not
  anchor on `<caption>Left`.
- bd-0vje43lp: `:NoSpace` after a blank line is not parsed as a caption at
  all. This is a grammar issue.
