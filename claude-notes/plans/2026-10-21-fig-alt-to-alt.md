---
title: 'fig-alt never reaches the image alt text (bd-ykjwiqcc, GH #877)'
date: 2026-10-21
description: 'q2 leaves `fig-alt` as a custom attribute, so HTML writes `data-fig-alt` with an empty or caption-valued `alt`, and docx writes an empty or caption-valued `descr`. Fold `fig-alt` (and the `alt` key-value) into the Image alt inlines in one format-neutral Normalization-phase transform, ahead of every render-side consumer.'
status: draft  # awaiting review; not yet executed
braid:
  strand: bd-ykjwiqcc
---

**Branch:** `braid/bd-ykjwiqcc-fig-alt-to-alt`. This is a topic branch in the main checkout, based on `main` at `f6e950d29`.
**Issue:** <https://github.com/quarto-dev/q2/issues/877>

## Implementation status

- [ ] Commit A — canonical alt (D1 + D2 + D3 + D4 + D6)
  - [ ] Tests written and failing: transform unit tests (1, 2), HTML/revealjs integration (3), docx (4), typst regression (5), preview (6), pampa writer guard (7), book cover flip (8), ordering pins (9), qmd round-trip (11)
  - [ ] D1 `image-alt-normalize` transform + registration + `BUCKETS`
  - [ ] D2 float-level `fig-alt`
  - [ ] D3 HTML writer `alt` guard
  - [ ] D4 book cover canonical + assertion flip
  - [ ] D6 qmd writer inverse
  - [ ] `Image.tsx` comment
- [ ] Commit B — D5 caption/alt dedupe (tests 10 first)
- [ ] Docs note + changelog entries
- [ ] `cargo xtask verify` green; post-fix reproduction table recorded here
- [ ] Discovered strands filed (`--output-dir` resources, `figure-img`)
- [ ] Strand comment; push + PR (ask first)

## Decisions (2026-10-21, review round 1)

1. **Case C: match Pandoc/Q1.** Omit `alt` on a figure's `<img>` when its alt text equals the figcaption. This goes in a separate commit, in the same session and the same PR (D5).
2. **Div-float `fig-alt` is in scope (D2).** A uniform treatment of every attribute on div-shaped figures is a later session; `fig-alt` sets the baseline.
3. **Canonicalize early, not after the post-filters.** D1 is a `Normalization`-phase transform that runs right after `FloatRefTargetSugarTransform`, ahead of the crossref, navigation, and finalization phases. The assessment of both placements is in § "Placement: early transform vs. late stage".

## Reproduction (2026-10-21)

I extended the issue's `figalt.qmd` with F, a captioned `#fig-` image, and G, an id'd image with no caption. A second file, `alt2.qmd`, covers a `{alt=…}` key-value, `fig-alt=""`, and a div float. I rendered both with `target/debug/q2` built from `f6e950d29` and with Quarto 1 (`quarto` 99.9.9 dev).

**HTML** (`--to html`):

| Case | Quarto 1 | q2 |
|---|---|---|
| A bare image | `alt="Alt A"` | `alt="" data-fig-alt="Alt A"` |
| B caption + fig-alt | `alt="Alt B"` | `alt="Caption B" data-fig-alt="Alt B"` |
| C caption only | no `alt` | `alt="Alt C"` |
| D inline | `alt="Alt D"` | `alt="" data-fig-alt="Alt D"` |
| E with class | `alt="Alt E"` | `alt="" data-fig-alt="Alt E"` |
| F `#fig-f` + caption | `alt="Alt F"` | `alt="Caption F" data-fig-alt="Alt F"` |
| G `#fig-g`, no caption | `alt="Alt G"` | `alt="" data-fig-alt="Alt G"` |
| `![](x.png){alt="KV A"}` | `alt="KV A"` | `alt="" … alt="KV A"`, **two `alt` attributes** (invalid HTML; browsers keep the first, the empty one) |
| `![Cap I](x.png){fig-alt=""}` | `alt=""` | `alt="Cap I" data-fig-alt=""` |
| `::: {#fig-div fig-alt="Div alt"}` | no `alt` (Q1 bug) | `alt=""` (fig-alt dropped) |

`--to revealjs` produces the same output as q2 HTML, with `r-stretch` in place of `img-fluid`.

**docx** (`wp:docPr descr=`): Quarto 1 writes `Alt A` … `Alt G`. q2 writes `""`, `Caption B`, `Alt C`, `""`, `""`, `Caption F`, `""`. **docx is broken too**, which the issue does not mention.

**typst:** q2 matches Quarto 1 byte-for-byte on every `image(…, alt: …)` call. The vendored `quarto-post/typst.lua` reads the `fig-alt` attribute itself.

LaTeX/PDF are not native q2 formats (`--to latex` → `Unknown format`), so they are out of scope.

## Why it happens

- The pampa reader keeps `fig-alt` as a key-value on the `Image`. For `![Caption](x.png)` it builds an implicit `Figure` whose caption **and** Image alt inlines both hold the caption (`pampa/src/pandoc/desugar.rs::implicit_figure`). This matches Pandoc's reader.
- Nothing in q2 reads `fig-alt` for images. Only the mermaid path folds it into `accDescr:` (`crossref/codeblock_shorthand.rs`).
- The HTML writer (`pampa/src/writers/html.rs:977`) writes `alt=` from the Image's alt inlines and then calls `write_attr`. `write_attr` data-prefixes unknown keys, giving `data-fig-alt`. It passes the known key `alt` through verbatim, which produces the duplicate.
- Pandoc-hybrid formats (docx, typst, …) run the vendored Q1 `main.lua`. q2 hands Pandoc the same AST Pandoc's own reader would produce (`Para [Image (fig-alt=…) [] …]`). In Quarto 1, a filter in the **render** group rewrites that Image into `Image ("",[],[]) [Str "Alt A"]` for docx. I confirmed this with a dump filter at each `at:` position: `pre-render` still has `fig-alt`, and `post-render` has the alt inlines. I could not find which Q1 filter does it. q2's pandoc-hybrid path loses that rewrite. Typst works only because `typst.lua` reads the attribute directly.

### The neutral representation

Pandoc's own model is: **the Image's alt inlines are the alt text, and the Figure caption is separate.** Pandoc's reader already folds `{alt="…"}` on an implicit figure into the alt inlines. Every writer we care about reads the alt inlines:

- q2's HTML writer: `alt=`.
- The hub preview's `Image.tsx`: `alt` comes from `inlinesToPlainText(altInlines)`.
- Pandoc's docx writer: `descr`.
- Pandoc's HTML/EPUB writers.
- Vendored `typst.lua`: falls back to `image.caption`, i.e. the alt inlines. It skips that fallback only when they equal the figure caption, which stops being true once the alt is folded in.

So the fix is to put `fig-alt` into the alt inlines and remove the attribute. No writer changes are needed for correctness.

## Proposed design

### D1. One format-neutral transform: `image-alt-normalize`

A new `AstTransform` in `transforms/image_alt_normalize.rs`. It is pushed in `build_transform_pipeline` at the end of the Normalization phase, right after `FloatRefTargetSugarTransform` / `EquationLabelTransform` and before `CrossrefIndexTransform`. It is classified **B1** in `BUCKETS`. It is not on any exclusion list, so it runs for HTML, revealjs, q2-preview, q2-slides, and every pandoc-hybrid format. It also runs in the book chapter partial renders (`..=Normalization`).

For every Image reachable from the body (via `transforms::image_walk::for_each_image_mut`, which already covers captions, tables, custom-node slots, and notes):

1. If the Image has `fig-alt`: replace `image.content` with the alt text and remove `fig-alt`. The text becomes a single `Str`, or nothing if it is empty. Empty means decorative, so the result is `alt=""`, which matches Q1.
2. Otherwise, if it has an `alt` key-value: do the same with `alt`. This is Pandoc's semantics for `{alt=…}` and fixes the duplicate-attribute case.
3. Otherwise leave it alone.

If both keys are present, `fig-alt` wins and both are removed. Q1's `typst.lua` uses the same precedence.

**Why not in the reader/desugar.** The qmd writer's implicit-figure round-trip (`writers/qmd.rs::match_implicit_figure_shape`) requires caption == alt inlines, and the hub edits through that path. Rewriting alt inlines in the reader would break that round-trip.

### Placement: early transform vs. late stage

The first draft put D1 after `UserFiltersStage::post()`. Review proposed moving it next to the generate side of the generate/render split. I compared the two:

**Late stage (after post-filters).**
- *For:* every user filter, pre or post, sees the authored `fig-alt`, which is exactly Q1's `post-quarto` view. It also catches a `fig-alt` that a post-filter adds.
- *Against:* every in-pipeline consumer runs before it, so each one sees the un-canonical shape. Concrete instances in the code today:
  - `crossref-render` already harvests `fig-align`, style, and column classes off the contained Image and moves them to the figure (`crossref_render.rs`, `HarvestedFigureAttrs`). This is the "pick an attribute up near render and relocate it" pattern. Any future alt handling there, such as lightbox titles or D5's caption/alt dedupe, would see caption-as-alt and a stray `fig-alt`.
  - `llms-capture` serializes the Finalization-phase AST, before the stage would run.
  - The hub preview's `Image.tsx` has no `fig-alt` logic. It is correct only because it reads the alt inlines.
  
  Every new consumer would have to remember to look at both representations.

**Early transform (end of Normalization).**
- *For:*
  - Every image source is already in place by then: engine output, includes, shortcodes, user pre-filters, the book cover (Normalization), and the float sugar. So the canonical shape exists before every consumer: crossref index/render, navigation, llms, preview, and the Pandoc cut.
  - D2 becomes uniform: div floats and `Figure` floats are both `FloatRefTarget` nodes at that point, with the float attrs still in hand. That is the baseline the later "all attributes on div figures" session wants.
  - It sits in the B1 neutral core, where the bucket taxonomy says semantic canonicalization belongs.
- *Against:*
  - **Explicit post-filters** (listed after `quarto` in `filters:`) no longer see `fig-alt`; they see the alt inlines instead. Default filters run pre (`user_filters.rs`: only entries after the `quarto` sentinel are post), and those still see `fig-alt`. A Q1 post-quarto filter that reads `fig-alt` would need to read the alt inlines (`image.caption` in Lua). I judge that rare and acceptable, and the docs note covers it.
  - A `fig-alt` created *after* Normalization (by a post-filter, or by future q2 code) is not folded, and the HTML writer data-prefixes it as it does today. q2's own transforms should build the canonical shape directly. I'm not adding a second late sweep: two normalization sites would blur where the canonical AST is defined.
  - **qmd serialization of the canonical shape.** `llms-capture` writes the post-transform AST with the qmd writer. Today an implicit figure serializes as `![Caption](x.png){fig-alt="…"}`. After D1 its caption ≠ alt inlines, so `match_implicit_figure_shape` fails and the writer falls back to the fenced-div form, which doesn't round-trip (bd-emr4). Fix: teach the qmd writer the inverse of D1 (D6).

**Assessment: the early transform is the better design.** Its costs are bounded and fixable (D6) or documented (post-filters). The late stage's cost is open-ended: every render-side consumer, present or future, has to handle both shapes.

### D2. A float-level `fig-alt` reaches its image

Example: `::: {#fig-x fig-alt="…"}` wrapping a single image. Q1 propagates float `fig-alt` down to the float's Images for non-HTML formats (`quarto-pre/figures.lua`). For HTML, Q1 sets it on the float and it never reaches the `<img>`, which is a Q1 bug.

D1 runs right after the float sugar, so it sees every float as a `FloatRefTarget` with its attrs. Its `FloatRefTarget` arm copies the float's `fig-alt` onto each contained Image that has neither `fig-alt` nor `alt`, removes it from the float, and then folds the images as usual. The Image's own `fig-alt` beats the float's. Subfigure floats nested inside a parent float each carry their own `fig-alt`, and the innermost one wins.

### D3. HTML writer guard

D1 removes `alt` key-values before the writer runs, so the duplicate cannot occur in the pipeline. `pampa` is also used standalone (the `pampa` CLI with `-t html`, and Lua `pandoc.write`). For those, make the `Inline::Image` arm skip an `alt` key-value in `write_attr`, using it in place of the alt inlines when present. This is a small change in `html.rs`, with a pampa unit test.

### D4. Book cover

`BookCoverImageTransform` runs in Normalization before D1 and emits `fig-alt`. D1 folds it, so the cover `<img>` gets `alt="Book cover"`. Flip the pinned assertion at `crates/quarto-core/tests/integration/book_multifile_html.rs:566-573` to `alt="Book cover"` with no `data-fig-alt`, and rewrite the "writer-level deviation" comment. Better still, the transform builds the canonical shape directly (alt inlines, no `fig-alt`), per the "q2 code builds canonical shapes" rule above. Either way the assertion is the same.

### D5. No duplicate caption/alt on figures (decision 1; separate commit)

Pandoc's HTML writer omits `alt` on a figure's image when the alt text equals the figcaption text. This is the canonical AST's presentation, not its content: the AST keeps alt == caption, as Pandoc's does. So D5 lives on the HTML render side.

Wherever q2 emits the `<figure>`/`<figcaption>` pair (the crossref-render HTML path and the writer's `Block::Figure` arm), drop the `alt` attribute from the single contained `<img>` when its plain-text alt equals the plain-text caption. An empty `fig-alt=""` still gives `alt=""`, because it differs from a non-empty caption. Whether this goes in `crossref_render` or the writer depends on which one has both strings in hand; I'll settle that while implementing.

This affects HTML and revealjs; the preview's `Image.tsx` needs the same rule if the preview renders figcaptions itself. Pandoc formats already behave this way in Pandoc's writers and `typst.lua`.

### D6. qmd writer emits the inverse of D1

So that `llms-capture`'s mirror (and any other post-transform qmd serialization) stays in the image form. In `writers/qmd.rs`, extend `match_implicit_figure_shape`: when the caption is a single `Plain` and the image's alt inlines differ from it, accept the shape as long as the alt inlines are plain text. Then emit `![<caption>](src){fig-alt="<alt>"}`. Re-reading that through D1 produces the same AST. This is a strict superset of today's match, so existing round-trips are unchanged. It touches bd-emr4's territory (caption ≠ alt) without claiming to close it.

## Out of scope (noting, not fixing)

- **`figure-img` class missing.** On captioned figures Q1 adds `figure-img` to the `<img>` and q2 doesn't. This is a separate parity gap; I will file a strand if none exists.
- **`--output-dir` single-file resource bug.** `q2 render figalt.qmd --to html --output-dir q2out` rewrites `src` to `../x.png`, which is correct relative to `q2out/`. The resource collector then resolves `../x.png` against the *source* directory, warns Q-5-6 "not found", and doesn't copy the image. This is unrelated to alt text; I will file it as a discovered-from strand.

## Tests (written first, failing before the fix)

1. **Stage unit tests** in the new module, one per D1 rule:
   - fig-alt on a bare image;
   - fig-alt on an implicit figure: caption unchanged, alt inlines replaced;
   - an `alt` key-value;
   - both keys present: fig-alt wins, both removed;
   - `fig-alt=""`: empty alt inlines;
   - an image inside a float slot / table cell / footnote, which exercises the walker;
   - no keys: untouched.
2. **D2 unit test:** a div float with `fig-alt` and a contained image → the image carries `fig-alt` and the float doesn't. Also: an image's own `fig-alt` beats the float's.
3. **HTML integration test** (`crates/quarto-core/tests/integration/`, new `fig_alt.rs` registered in `main.rs`): render the A–G + KV + `fig-alt=""` + div-float fixture to HTML. For each `<img>` assert the exact `alt`, no `data-fig-alt`, and exactly one `alt=`. Same for revealjs.
4. **docx:** assert `descr` for each case. This needs the pandoc-hybrid harness. I'll use whatever the existing docx integration tests use (pandoc recordings via `quarto-pandoc-recording` / `cargo xtask capture-pandoc-recordings`, or a pandoc-gated test) and record new fixtures if needed.
5. **typst regression:** the `.typ` output's `image(…, alt: …)` calls are unchanged for every case. They match Q1 today, so this pins against regressions from D1.
6. **Preview:** the q2-preview AST for case B has the alt inlines `[Str "Alt B"]`, the caption still "Caption B", and no `fig-alt` key-value. This goes wherever preview-AST tests live (`PreviewAstOutput`).
7. **pampa writer guard (D3):** `![](x.png){alt="X"}` through `pampa -t html` gives a single `alt="X"`.
8. Flip the `book_multifile_html.rs` assertion (D4).
9. **Ordering pins (decision 3):**
   - A user **pre**-filter sees `fig-alt` as an attribute.
   - A user **post**-filter sees the alt inlines and no `fig-alt`.
   - `crossref-render` receives an Image whose alt inlines are already the `fig-alt` text.
10. **D5:**
    - `![Caption C](x.png)` gives an `<img>` with no `alt` plus the figcaption.
    - `![Cap](x.png){fig-alt="Alt"}` gives `alt="Alt"`.
    - `![Cap](x.png){fig-alt=""}` gives `alt=""`.
    - An inline image with alt text keeps its `alt`.
    - Same for a `#fig-` float, and for revealjs.
11. **D6 round-trip:** for `![Cap](x.png){fig-alt="Alt"}`, reading, then D1, then the qmd writer gives back the same text. The plain implicit-figure round-trip is unchanged. An alt with markup (non-plain inlines) falls back as today.

## Implementation steps

1. Write tests 1–11 and watch them fail. Tests 5 and, partly, 3 must pass already.
2. Add `transforms/image_alt_normalize.rs` (D1 + D2) and register it after `FloatRefTargetSugarTransform`. Add it to `BUCKETS` as B1. Run the totality and neutral-core tests.
3. D4: build the cover canonically, then flip the assertion.
4. D3: HTML writer guard in `pampa/src/writers/html.rs`.
5. D6: qmd writer inverse.
6. Commit. Then D5 as its own commit.
7. Update `ts-packages/preview-renderer/src/q2-preview/inlines/Image.tsx`. Its doc comment says `fig-alt` is "silently ignored". Change it to say the pipeline folds `fig-alt` into the alt inlines upstream.
8. Add a changelog entry wherever CLI-facing fixes are recorded. The hub-client changelog also gets one, since the preview is affected. Add a sentence to `docs/guides/authoring/figures.qmd` "Alt Text" on what a user post-filter sees: the alt inlines (Lua `image.caption`), not `fig-alt`.
9. `cargo xtask verify`. Re-run the reproduction for HTML, revealjs, docx, and typst and record the new table here.
10. Comment on the strand. File the two discovered strands (`--output-dir` resources, `figure-img`). Push `braid/bd-ykjwiqcc-fig-alt-to-alt:bugfix/bd-ykjwiqcc-fig-alt-to-alt` and open a **single PR** referencing #877, after asking first.

## Open questions

None. Q1–Q3 from the first draft are resolved in § Decisions.
