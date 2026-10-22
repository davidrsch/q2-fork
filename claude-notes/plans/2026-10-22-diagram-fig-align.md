---
title: 'Diagram cells: honour fig-align (bd-2pmjmjmj)'
date: 2026-10-22
description: 'Stop rejecting fig-align on mermaid cells with Q-2-47, and decide whether q2 should apply it as alignment (Quarto 1 mostly does not in HTML).'
status: draft  # Investigation — pending design alignment with user; do not implement before the go-ahead
braid:
  strand: bd-2pmjmjmj
---

**Branch:** `braid/bd-2pmjmjmj-diagram-fig-align` (topic branch in the main checkout, based on `main` @ `8ddbf8b17`)
**Do not start implementation until the user gives the go-ahead.**

## Triage verdict

**Ready to implement once the remaining question below is answered.** The 2026-10-22 design answers are recorded under § Decisions. One premise in the strand does not hold up: Quarto 1 never honours the **cell option** `fig-align` on a diagram in HTML, whether the cell is labelled or not. Quarto 1 honours only the **document-level** `fig-align`, and only on labelled floats, where it even overrides a conflicting cell option. Fixtures: `q1-cell-option.qmd` and `q1-document-option.qmd`.

**Pre-flight at HEAD** (`cargo xtask verify --skip-hub-build`): all Rust tests passed (17226). Two hub-client wasm tests failed, both outside this area: `importDocument … writer-bugs-docx` (footnote text) and `applyNodeEdit … DefinitionList round-trip`. The likely cause is a stale wasm build under `--skip-hub-build`. This is unconfirmed.

## Issue context

Strand bd-2pmjmjmj (feature, P3, label `diagrams`). Filed 2026-10-22 by Claude Fable 5.1 as finding F10 of the product-doc-theme assessment. Status before this investigation: open.

Diagram cells accept only `label`, `fig-cap`, `fig-scap` and `fig-alt` (`DIAGRAM_OPTION_KEYS`, `crates/quarto-core/src/crossref/codeblock_shorthand.rs:395`). Every other option draws `Q-2-47` "Cell option ignored on a diagram cell". The product-doc-theme example site uses `%%| fig-align: center`, so it gets this warning. The strand asks q2 to accept `fig-align: left|center|right|default` and to render it the way image `fig-align` is rendered. Inapplicable options should still warn.

Acceptance: a mermaid/dot cell with `%%| fig-align: center` aligns like an image with `fig-align: center` and draws no Q-2-47. Other inapplicable options still warn. A codeblock_shorthand unit test and a smoke-all fixture cover the change.

## Dependency graph

- **discovered-from**: bd-0i6fii0k (in_progress, P1) is the exploratory assessment of `posit-dev/product-doc-theme` on q2. Its plan, `2026-10-21-product-doc-theme-q2.md`, lives on `braid/bd-0i6fii0k-product-doc-theme-q2`, which I could not find locally. Per the strand comment, that branch is commit `9f3c57e44`. This strand is one of ten findings; none of the others blocks it.
- **blocks / related**: none. Nothing depends on this strand, which fits its P3 priority.
- Prior art in this area: `2026-08-10-mermaid-cell-options.md` (the `%%|` options design and the origin of Q-2-47) and `2026-07-20-mermaid-regular-rendering.md`. The `DIAGRAM_OPTION_KEYS` doc comment already says to grow the list when a feature starts honouring a key.

## What the code looks like today

The strand's file references still hold. Repros: `diagram-fig-align-investigation/q1-repro.qmd` (brace `{mermaid}` cells for Quarto 1) and `diagram-fig-align-investigation/q2-repro.qmd` (plain fences for q2).

**q2 at HEAD.** Each of the three cells (bare, unlabelled with a caption, labelled float) draws Q-2-47 for `fig-align`. The unconsumed `%%| fig-align: …` line stays in the diagram source, where it is a harmless mermaid comment. Alignment comes out as:

| Cell | q2 wrapper |
|---|---|
| bare (no caption) | no wrapper; `<pre class="mermaid">` alone |
| `fig-cap`, no label | `div.quarto-figure.quarto-figure-center` › `figure` |
| `label` + `fig-cap` | `div#fig-m.quarto-float.quarto-figure.quarto-figure-center` |

Alignment is always `center`. `crossref_render.rs` gets the alignment from `harvest_figure_attrs`, which reads only an `Image` `fig-align` key, and a diagram has no image (`crossref_render.rs:268`, around line 713). So `fig-align: center`, the case product-doc-theme uses, already renders correctly. Only the warning is wrong.

**Quarto 1 (dev build on PATH), HTML output.**

| Cell | Q1 output |
|---|---|
| `{mermaid}`, `fig-align: left`, no caption | `div.cell[data-layout-align=left]` › `figure.figure` › `pre.mermaid` with no alignment class |
| `{mermaid}`, `label` + `fig-cap`, `fig-align: right` | `div.cell[data-layout-align=right]` › `div#fig-m.quarto-float.quarto-figure.quarto-figure-center` |
| plain ` ```mermaid ` | not a cell; options are ignored and the `%%|` line stays in the source |

The handler turns `fig-align` into `layout-align` on the `.cell` div (`src/core/handlers/base.ts:849`). Only the panel-layout filters (`layout/*.lua`, `panellayout.lua`) read that attribute, and only for cells inside a `layout-ncol`-style layout. Revealjs is the exception: there, `fixupAlignment` applies the alignment to the SVG (`mermaid.ts:242`, `dot.ts:144`).

**Does a `fig-` label change this?** Only for the document-level option. A float picks its class in `get_figure_attributes` (`customnodes/floatreftarget.lua:289`), which calls `figAlignAttribute(float)` (`common/figures.lua:26`). That function reads `fig-align` from the float's own attributes and otherwise falls back to `param("fig-align")`, the document-level option. The cell option never reaches the float's attributes, because it was diverted to `layout-align` on the outer `.cell` div. Q1 HTML output:

| Cell | Cell option only | Document `fig-align: right` |
|---|---|---|
| `label: fig-a`, no caption, cell `right` | `quarto-figure-center` | n/a |
| `label` + `fig-cap`, cell `left` (`fig-b`) | `quarto-figure-center` | n/a |
| `label` + `fig-cap`, no cell option (`fig-c`) | n/a | `quarto-figure-right` |
| `label` + `fig-cap`, cell `left` (`fig-d`) | n/a | `quarto-figure-right`: document beats cell |
| `fig-cap`, no label | bare `figure.figure`, no class | bare `figure.figure`, no class |

**In standalone HTML, Q1 ignores the cell option entirely.** A label matters only because floats are the one place where Q1 consults the document-level `fig-align`.

**q2 has no document-level `fig-align` at all.** The only reader is `harvest_figure_attrs`, which reads an `Image` attribute (`crossref_render.rs:539`). This gap affects images as well as diagrams.

**`dot` is not a q2 diagram language.** `is_diagram_language` matches only `mermaid`, so ` ```dot ` cells get no `%%|` handling and no Q-2-47. That part of the strand's title does not apply today.

## Decisions (2026-10-22, with the user)

1. **Apply, not just accept.** `fig-align: left|center|right` aligns the diagram. This is a deliberate improvement over Q1, which ignores the cell option. Record it in the strand.
2. **Accept `fig-align` on every mermaid cell, including the bare cell with no label or caption.** Do not emit a "no effect in this position" warning. Q1 users are widely confused here, so q2 is lenient on purpose; we may revisit this.
3. **Revealjs is in scope** for this strand. Use CSS, since q2 renders mermaid client-side.
4. **Mermaid only.** Drop `dot` from the strand title and acceptance criteria.
5. **`default` means `center`.** Any value other than `left`, `center`, `right` or `default` warns.

## Proposed phases (draft)

- Phase 0: tests first. Add a codeblock_shorthand unit test where `fig-align` on a diagram cell draws no Q-2-47, and keep the existing `echo` test showing that unknown keys still warn. Add a smoke-all fixture that asserts the wrapper class for left, center and right on the caption and float paths.
- Phase 1: accept the key. Add `fig-align` to `DIAGRAM_OPTION_KEYS` and consume it. Fix the hint text, which lists the accepted keys.
- Phase 2 (if Q1 = apply it): carry the value to `crossref_render`. One option is a `fig-align` attribute on the wrapper (`Wrapper::Float` attributes / `Figure` attr), which `render_float_ref_target` and `wrap_standalone_figure` read when no image supplied one. Check that `.quarto-figure-left > figure > div` gives a `pre.mermaid` SVG the expected `text-align`.
- Phase 3: the bare cell (decision 2). `fig-align` alone wraps the diagram in an uncaptioned `quarto-figure-<align>` div, or puts a class on the `pre.mermaid`. Choose whichever stays closer to the existing wrapper shapes.
- Phase 3b: revealjs (decision 3). Add CSS so the client-rendered SVG follows the wrapper class.
- Phase 3c: validate the value (decision 5). `default` becomes `center`; any other value warns.
- Phase 4: docs. Mention `fig-align` wherever the diagram cell options are documented.

## Open design questions for the user

6. **Document-level `fig-align`.** Q1 honours the document-level `fig-align` on labelled floats, including diagram floats, and lets it override a cell option. q2 does not read the document-level option for any figure, image or diagram. Should this strand add it for diagrams? If so, what precedence applies: does the cell option win, unlike Q1? Or should it be a separate strand covering images and diagrams together?

### Answered (kept for the record; see § Decisions)

1. **Accept or apply?** Quarto 1 accepts `fig-align` on diagram cells but ignores it in HTML; the labelled float stays centered even for `right`. Options:
   (a) consume it silently, matching Q1. That is a one-line change, and it fixes product-doc-theme, which only uses `center`.
   (b) make left/right actually align the figure, a deliberate improvement over Q1 that the strand would record as such.
   I lean toward (b): the key reads as a promise, and the wrapper plumbing already exists.
2. **Bare cell (no caption, no label).** q2 builds no figure wrapper here, so there is nothing to put an alignment class on. Should `fig-align` alone be enough to wrap the cell in an unnumbered, uncaptioned `quarto-figure-<align>` div? Or should it keep warning ("no effect in this position") the way `fig-scap` does on a float? Q1 also has no effect here.
3. **Revealjs.** Q1 aligns diagram SVGs in revealjs only. Should q2 cover revealjs in this strand, or file it separately? (q2 renders mermaid client-side, so this would be CSS, not SVG fix-ups.)
4. **`dot`.** Should the strand drop "dot" from its title and acceptance criteria, since q2 does not treat ` ```dot ` as a diagram today?
5. **`default` value.** The strand lists `left|center|right|default`. Should `default` map to `center`, which is what q2 falls back to? Should any other value warn?

## Risks / tradeoffs (draft)

- With Q1 = (b), q2 output diverges from Q1 for `left`/`right` diagram figures. The change is visible but is arguably a fix. Any smoke-all fixture that compares against Q1 output would need a note.
- `harvest_figure_attrs` treats the image as the source of truth. A wrapper-level `fig-align` needs a precedence rule for a float that contains both a diagram and an image (an unlikely shape).
- The preview pipeline (`Q2_PREVIEW_TRANSFORM_EXCLUDED`) renders mermaid in React. Check whether alignment classes on the wrapper survive into hub-client preview.
