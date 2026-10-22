---
title: 'Table caption classes → Bootstrap table-* classes (bd-hfghrqvh)'
date: 2026-10-22
description: 'Map Q1''s Bootstrap table shorthands (.striped .hover .sm .responsive-md …) written in a table caption''s attribute block to table-* classes on the <table>, for unlabelled, #tbl-labelled and ::: {#tbl-} div-form tables.'
status: draft  # Investigation — pending design alignment with user; do not implement before the go-ahead
braid:
  strand: bd-hfghrqvh
---

**Branch:** `braid/bd-hfghrqvh-table-caption-bootstrap-classes` (topic branch in the main checkout, based on `main` @ `c68647049`)
**Do not start implementation until the user gives the go-ahead.**

## Triage verdict

**Ready to design.** The gap reproduces at HEAD, Q1's behaviour is small and
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
yields <table class="… table table-striped table-hover"> with the classes
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

## Proposed phases (draft)

- **Phase 0: tests first.**
  - Unit tests in `table_bootstrap_class.rs`: prefix mapping, `sm`↔`small`
    pairing, passthrough of non-shorthand classes, idempotence, responsive wrap.
  - A smoke-all fixture (e.g. `crates/quarto/tests/smoke-all/markdown/table-caption-bootstrap-classes.qmd`)
    covering the five rows of the table above, using
    `ensureFileRegexMatches`.
- **Phase 1: normalize classes already on `Table.attr`.** This fixes the
  unlabelled form, `.small`, and the colour and state shorthands.
- **Phase 2: forward float classes to the inner table(s)** for labelled and
  div-form `tbl` floats. The float keeps its own copies, as in Q1. Where this
  runs depends on Design question 1.
- **Phase 3: responsive wrapper.** Wrap the table in
  `Div(.table-responsive*)`, HTML only.
- **Phase 4: q2-preview check, docs, braid comment.**
  - Confirm the preview path picks this up. The transform is not in
    `Q2_PREVIEW_TRANSFORM_EXCLUDED`.
  - Update the module doc.
  - Note any deliberate divergence on the strand.

## Open design questions for the user

1. **Placement and formats.** Q1 normalizes in the *pre* stage for *all*
   formats, so LaTeX and Typst also see `table-striped`, and Q1's own later
   filters see the mapped names.
   - Recommendation: do it in the existing HTML-only Finalization transform.
     It is the smallest change, and `table-*` classes mean nothing outside
     Bootstrap.
   - Alternative: add an all-format normalization transform early in the
     pipeline, which is exact Q1 parity.
   - Is HTML-only acceptable as a reviewed divergence?
2. **Float classes: duplicate or move?** For `#tbl-` tables Q1 leaves the
   unprefixed `bordered sm` on the float `<div>` *and* puts the prefixed
   copies on the `<table>`. Should we mirror that duplication exactly
   (recommended for parity, since CSS and JS that target the div keep
   working), or move the classes onto the table only?
3. **Non-shorthand float classes.** Q1 also copies arbitrary classes, such as
   a user `.my-class` on `::: {#tbl-x .my-class}`, onto the inner table. Do we
   want that too, or only the recognized shorthands? Q1-faithful is
   recommended, and the acceptance criteria say "forwarded verbatim".
4. **Responsive wrapper in scope?** The acceptance criteria say "all
   shorthands mapped", but in Q1 `.responsive*` is a *wrapper div*, not a
   table class. Should the wrapper be in this strand (recommended: it is about
   15 lines, and the class is useless without it), or a follow-up strand?
5. **Non-table floats.** For a `tbl` float whose content is not a single
   table (e.g. a layout of several tables), Q1 applies the classes to every
   nested table and skips subfloats. Mirror that now, or limit to the
   single-table case and file a follow-up?
6. **Incidental: caption location.** For crossref tables q2 renders
   `quarto-float-caption-bottom` and Q1 renders `-top`. Should I file this as
   its own parity strand?

## Risks / tradeoffs (draft)

- Phase 2 needs to know what a `tbl` float looks like at Finalization: whether
  it is still a `Div(#tbl-…)` carrying classes, or already rendered into
  figure markup. If the float classes are gone by then, forwarding has to
  happen earlier. One option is in `maybe_wrap_bare_table_into_div` /
  `convert_div`, which would also cover the `::: {#tbl-}` form. This is the
  main unknown, and it bears on Design question 1.
- Q1's float `<div>` and `<table>` both carry the classes, so naïve
  `ensureFileRegexMatches` patterns can match the wrong element. Anchor
  fixture regexes on `<table class="…"`.
- Idempotence is required: the existing transform is idempotent and is tested
  for it.
