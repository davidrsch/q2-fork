---
title: 'hub-client revealjs: cursor→slide sync lands on the previous slide after an untitled slide'
date: 2026-10-20
date-modified: 2026-10-20
description: 'The editor-cursor → reveal-slide mapping pairs slides with LSP header symbols by index; an untitled heading (empty `##`, image-only, or attribute-only) has no symbol, so every later slide inherits the next header''s start line and the preview shows the slide before the cursor''s.'
status: in-review  # implemented and verified 2026-10-20; PR pending
braid:
  strand: bd-cursor-slide-untitled-heading-p2wuuc13
  priority: P2
  labels: [hub-client, revealjs]
---

**Owner:** cscheid
**Precedents:** bd-mwbsdmel (cursor↔slide channel over `SET_SLIDE` /
`SLIDE_CHANGED`), bd-vwp4y5ku (revealjs → q2-slides convergence),
bd-baiwm4pb (open: retire the editor-side `parseSlides` duplicate)

## Symptom

In hub-client's split view for a `format: revealjs` document, moving the
editor cursor normally drives the preview to the slide the cursor is in.
When the deck contains a slide whose heading has no text, every section
*after* that slide is off by one: the preview shows the slide immediately
before the one the cursor is in. Sections before the untitled slide are
unaffected.

The verbal report said "a slide with an empty `##`". Both readings of
"empty slide" trigger it, and so does a third common idiom:

- a bare `##` line (slide with content but no title);
- `## ![alt](image.png)` (an image as the whole heading);
- `## {background-image="x.png"}` (attribute-only heading, the Quarto idiom
  for a full-bleed image slide) — same code path as the bare `##`, not
  separately browser-verified.

A slide with a title and no body does *not* trigger it.

## Reproduction (browser, 2026-10-20)

Local stack: `cargo build --bin hub`, `npm run build:wasm`,
`npm run build:sandboxed`, then `npm run local-dev` from `hub-client/`
(Vite on `localhost:5173`, hub on `:3000`, sandboxed preview on `:8081`).
New project from the **Presentation** template, `index.qmd` replaced with:

```markdown
---
title: empty-slide-repro
format: revealjs
---

## First slide

Alpha paragraph.

##

Untitled slide body.

## Third slide

Gamma paragraph.

## Fourth slide

Delta paragraph.
```

Cursor placed with Monaco's go-to-line (`Ctrl+G`):

| Cursor line | Section the cursor is in | Slide the preview shows |
| ----------: | ------------------------ | ----------------------- |
|           8 | First slide              | First slide ✓           |
|          12 | (untitled) slide         | **First slide** ✗       |
|          16 | Third slide              | **(untitled) slide** ✗  |
|          20 | Fourth slide             | **Third slide** ✗       |

Replacing line 10 with `## ![Picture alt text](missing.png)` gives the
identical table (line 16 shows the picture slide). In both variants the
**Outline** panel lists only First / Third / Fourth — the untitled slide is
missing from it, which is the visible trace of the root cause.

## Diagnosis

The mapping lives in `hub-client/src/hooks/useCursorToSlide.ts`. It takes
two inputs that are computed independently and pairs them **by position**:

1. `astJson` — the *transformed* AST the preview rendered (the
   `revealjs → q2-slides` path in `ReactPreview.tsx` forwards
   `result.ast_json` up through `onAstChange` to `Editor.tsx`). After
   `RevealSlidesTransform` (`crates/quarto-core/src/revealjs/slides.rs`,
   `transform.rs`) its top-level blocks are one `Div.section` per horizontal
   reveal slide, with a generated `#title-slide` Div inserted at index 0
   when the metadata has a title or author. `parseSlides` (from the
   editor's legacy `ReactAstSlideRenderer.tsx`) turns those into a
   `Slide[]`.
2. `symbols` — the LSP document outline from `useIntelligence`
   (`quarto-lsp-core`\'s `analyze_document`). The hook keeps the top-level
   entries with `kind === 'string'` (headers) as `slideHeaders`.

It then walks the slides in order and assigns each content slide the start
line of the *next unused* header symbol:

```ts
let headerIndex = 0;
for (let slideIndex = 0; slideIndex < slides.length; slideIndex++) {
  if (slides[slideIndex].type === 'title') { mappings.push({ startLine: 0, slideIndex }); continue; }
  if (headerIndex < slideHeaders.length) {
    mappings.push({ startLine: slideHeaders[headerIndex].range.start.line, slideIndex });
    headerIndex++;
  }
}
```

The symbol side drops untitled headers. In
`crates/quarto-lsp-core/src/analysis.rs`:

```rust
fn header_to_symbol(header: &Header, ctx: &SourceContext) -> Option<Symbol> {
    let name = inlines_to_text(&header.content);
    if name.is_empty() {
        return None;
    }
    ...
}
```

and `inline_to_text` has a `_ => {}` arm, so `Inline::Image` contributes
nothing — an image-only heading has an empty name too. The outline panel
is built from the same symbols, which is why the untitled slide is absent
from it.

So for the repro deck the slide side has five horizontal slides (title,
First, untitled, Third, Fourth) and the symbol side has three headers
(First, Third, Fourth). The by-index walk assigns:

| Slide (reveal `h`)   | Header it is paired with | Start line used |
| -------------------- | ------------------------ | --------------: |
| 0 title              | —                        |               0 |
| 1 First              | First                    |               5 |
| 2 untitled           | **Third**                |              13 |
| 3 Third              | **Fourth**               |              17 |
| 4 Fourth             | (none left)              |       unmapped  |

From line 13 onwards the lookup ("last slide whose start line ≤ cursor
line") returns slide 2 for the Third section and slide 3 for the Fourth
section: each is exactly one slide too early, and the final slide can
never be reached from the editor. The symptom is "precisely the previous
slide" because exactly one symbol is missing; two untitled slides would
shift later sections by two.

### Why the baseline works at all

The pairing survives on ordinary decks only through two coincidences
that are worth writing down, because any fix that keeps `parseSlides` in
the loop inherits them:

- `parseSlides` adds a synthetic `title` slide from `ast.meta` **and**
  then counts the transform's `#title-slide` Div as a content section
  (its classes are `section title-slide center`). That is one slide more
  than reveal has; the extra one sits at the *front*, so each content
  section is shifted by one — which happens to match reveal's indices
  because reveal's index 0 is the title slide.
- A `# Section` divider becomes a vertical stack (one top-level Div, one
  horizontal index) on the slide side, and on the symbol side the h2s
  nest under the h1 (`build_symbol_hierarchy` nests by level), so the
  top-level symbol list also has one entry for it. Again by luck, not by
  construction.

Anything that breaks either independent count breaks the sync: an
untitled heading (this bug), a heading that the crossref pipeline absorbs
into a target (the analysis emits a `class` symbol for the target instead
of a `string` one for the heading, and the hook filters it out — not
browser-verified), and `slide-level` other than 2 (the transform honours
it; the symbol hierarchy does not).

### Same defect, second consumer

`hub-client/src/hooks/useSlideThumbnails.tsx` pairs slides with
symbols with the same loop (its comment is copied verbatim) and keys
each thumbnail by the symbol's start line, so with an untitled slide
each outline entry after it would get the *previous* slide's thumbnail.
No thumbnails rendered in the reproduction session (the outline panel
had no `data:` images), so this is unverified in the current reveal
path; it is listed so the fix covers it rather than as a confirmed
symptom.

## Fix

### Recommended: derive slide start lines from the AST itself

Stop pairing two independently computed structures. The transformed AST
already carries everything the mapping needs:

- Its top-level blocks are, one for one, the horizontal slides
  `RevealDeck.tsx` renders (`props.ast.blocks.map((block, i) => …)`), so
  the array index *is* the reveal `h` index that `SET_SLIDE` uses. No
  `parseSlides`, no synthetic title slide, no stack special-casing.
- Every node that came from the source carries an `l` location
  (`include_inline_locations: true` on the preview path in
  `crates/wasm-quarto-hub-client/src/lib.rs`; shape documented in
  `ts-packages/preview-renderer/src/framework/sourceLoc.ts`, 1-based
  `l.b.l`). The slide Divs themselves are generated (no `l`), but the
  heading inside each keeps its original `source_info`
  (`section_attr_and_header` clones it), and so does every content
  block. The title slide is wholly generated and has no location.

New mapping, in `useCursorToSlide.ts`:

1. For each top-level block `i`, find the first descendant (pre-order)
   with an `l` field and take `l.b.l - 1` as the 0-based start line.
2. A slide with no located descendant (the generated title slide; a slide
   made only of generated nodes) gets start line 0 when `i === 0`, and is
   otherwise skipped (it is unreachable from the cursor, which is the
   correct answer for a slide with no source text).
3. Lookup stays as it is: last mapping with `startLine <= line`.

This makes untitled, image-only, attribute-only, preamble (content before
the first heading) and `---`-split slides all reachable, since each
contains at least one located block. It also removes the hook's
dependence on `symbols` (the `[astJson, symbols]` memo becomes
`[astJson]`), and makes the `.section` class irrelevant to the mapping.

Apply the same helper in `useSlideThumbnails.tsx` (iterate top-level
blocks, key by the derived start line) or, if bd-baiwm4pb is picked up at
the same time, delete the duplicate there instead. The helper belongs in
its own module (`hub-client/src/hooks/slideStartLines.ts` or similar) so
both hooks and a unit test share it.

Granularity note, unchanged by this plan: the channel mirrors only the
horizontal index (`RevealNavSync` reads `getIndices().h`), so a cursor in
a vertical sub-slide of a `# Section` stack shows the stack's first
slide. Worth a follow-up strand once this lands, since the AST-based
mapping makes `(h, v)` straightforward (second-level Divs inside a stack).

### Not recommended: emit a symbol for untitled headers

Changing `header_to_symbol` to return a placeholder name (`(untitled)`)
would fix this specific repro and would, arguably, improve the outline
panel. But it keeps the by-index pairing and all the coincidences above,
and it changes LSP output for every editor (`quarto-lsp` tests in
`crates/quarto-lsp-core/src/symbols.rs` assert current behaviour). If
the outline *should* list untitled slides, that is a separate UX
decision for the outline panel, not the sync mechanism; it can be filed
as its own strand.

## Test plan

- Unit test for the new helper (`hub-client/src/hooks/*.test.ts`, vitest,
  node environment): a hand-written transformed-AST fixture with a title
  slide, a titled slide, an untitled slide, an image-only slide, a
  `# Section` stack with two vertical children, and a preamble slide;
  assert start lines and that `getSlideForLine` returns the expected
  `h` for lines in each section and for lines before the first slide.
- Regression test named after this strand: a deck with one untitled slide
  in the middle; assert that the sections after it map to their own
  index, not the previous one, and that the last slide is reachable.
- Existing `useScrollSync.test.ts` / `useSelectionSync.test.ts` are
  untouched (different hooks).
- Manual: rerun the reproduction above (both the bare `##` and the image
  heading) and the `{background-image=…}` idiom; confirm the outline panel
  is unchanged by this fix (it still omits the untitled slide — expected).
- `cargo xtask verify` (hub-client vitest is part of it).

## Execution notes

- Branch off `main` with `cargo xtask create-worktree
  bd-cursor-slide-untitled-heading-p2wuuc13` (hub-client is in scope, so
  `npm install` in the worktree).
- Files touched: `hub-client/src/hooks/useCursorToSlide.ts` (rewrite the
  memo), `hub-client/src/hooks/useSlideThumbnails.tsx` (same pairing, plus
  the gate and rerun fixes below), new helper + tests under
  `hub-client/src/hooks/`, `getQ2Format.ts` / `printableDocument.ts`
  (shared `isSlidesFormat`), and the two hook calls in `Editor.tsx` (drop
  the `symbols` argument, use the predicate). No Rust changes.
- The sandboxed preview (`hub-client/quarto-hub-sandboxed-preview`) and
  `RevealDeck.tsx` need no change: `SET_SLIDE` already carries the
  horizontal index, which is what the new mapping produces.

## Decisions (review, 2026-10-20)

1. **Outline listing untitled slides: yes, separately.** Filed as
   bd-outline-untitled-slides-d525xvxv (discovered-from this strand), to
   be tackled right after this fix. This plan leaves the outline as is.
2. **Thumbnails: fix in place** with the shared helper. Retiring
   `parseSlides` stays with bd-baiwm4pb.
3. **Vertical index `(h, v)`: later.** Filed as
   bd-cursor-slide-vertical-index-wykcyjxb (discovered-from this strand).

## Discovered during execution (2026-10-20)

Two further defects in the thumbnail path surfaced while verifying the
thumbnail half of the fix in the browser. Both are pre-existing and both
are fixed on this branch, since without them the thumbnail change cannot
be exercised at all:

1. **Thumbnails were disabled for every `format: revealjs` deck.** The
   hook is gated on `currentFormat === 'q2-slides'`, but the resolver
   (`preview_pseudo_format` in
   `crates/quarto-core/src/pandoc_request/formats.rs`) reports `revealjs`
   for a revealjs document and only passes `q2-slides` through when the
   document literally asks for it. The gate predates the revealjs →
   q2-slides convergence. Fix: a shared `isSlidesFormat()` in
   `hub-client/src/components/render/getQ2Format.ts` (the module that
   already classifies preview modes), used by the thumbnail gate and by
   `isPrintableSlidesFormat` in `printableDocument.ts`, which had the same
   two-way comparison inlined.
2. **A capture requested mid-capture was dropped.** `captureThumbnails`
   returns early while `captureInProgressRef` is set, so a burst of typing
   could leave the outline showing thumbnails for an intermediate deck
   (observed: only the first of three slides had one until the next
   keystroke). Fix: remember the request and re-run once the running
   capture finishes, through a ref so the rerun sees the latest AST.
   Pinned by `useSlideThumbnails.test.tsx`, which fails on the old code.

Outcome in the browser after both: every titled slide in the outline has
a thumbnail keyed by its own heading line, including the slides after an
untitled one; vertical children of a `# Section` stack have none (they are
not horizontal slides, see bd-cursor-slide-vertical-index-wykcyjxb).

## Todo

- [x] Worktree `bd-cursor-slide-untitled-heading-p2wuuc13` off `main`,
      `npm install`.
- [x] Helper `hub-client/src/hooks/slideStartLines.ts`: walk the
      transformed AST's top-level blocks, first located descendant →
      0-based start line; title/unlocated at index 0 → line 0; other
      unlocated slides skipped. Fixture shapes checked against real
      `render_qmd_to_preview_ast` output for `q2-slides` (title slide and
      section Divs carry no `l`; headings, paragraphs and stack children
      do).
- [x] Unit tests for the helper and for the lookup
      (`slideStartLines.test.ts`: title, titled, untitled, image-only,
      attribute-only, `# Section` stack, preamble, no-title, generated
      middle slide; regression case named after this strand) and for the
      hook wrapper (`useCursorToSlide.test.ts`).
- [x] `useCursorToSlide.ts`: build mappings from the helper; drop the
      `symbols` parameter; update the call in `Editor.tsx`.
- [x] `useSlideThumbnails.tsx`: key thumbnails by the helper's start lines
      instead of by-index symbol pairing; `symbols` option dropped;
      `OutlinePanel.tsx` unchanged (still looks up by
      `symbol.range.start.line`). Plus the two fixes above.
- [x] Manual browser check (worktree Vite on :5174 against the running
      hub): bare `##`, `## ![alt](x.png)`, `## {background-image=…}`,
      preamble slide, `# Section` stack with `---` split, no-title deck
      lines, front matter → title slide. All lines land on their own
      slide; last slide reachable; outline unchanged (untitled slides
      still omitted, see bd-outline-untitled-slides-d525xvxv).
- [x] `cargo xtask verify` (all 14 steps green, 2026-10-20) and strict
      render of this plan.
- [ ] Commit, push, PR; close the strand.
