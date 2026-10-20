---
title: 'q2-preview-spa e2e: ''an edit recompiles the PDF'' flakes on two PDF preview iframes (bd-e4q7juv0)'
date: 2026-10-20
description: 'The typst PDF preview e2e locator matches both iframes of the viewer''s by-design recompile swap; scope it to the live frame (test-side, or by marking the live frame in @quarto/pdf-viewer).'
status: draft  # Investigation — pending design alignment with user; do not implement before the go-ahead
braid:
  strand: bd-e4q7juv0
---

**Branch:** `braid/bd-e4q7juv0-pdf-preview-iframe-flake` (topic branch in the main checkout, based on `main` @ `9758c3a2e`)
**Do not start implementation until the user gives the go-ahead.**

## Triage verdict

**Ready to design.** The cause is clear from the code: the double-iframe swap is intentional, and the
test's locator doesn't allow for it. The fix is small; what's left to decide is whether it lives only in
the test or also in the viewer.

## Issue context

Filed 2026-10-20 (today) by Carlos, P3 bug, label `flaky-test`. Seen on PR #848 (KaTeX snyk bump),
run 39435016486 job 118669637915, chromium: `q2-preview-spa/e2e/typst-pdf-preview.spec.ts:83`
failed all 3 attempts with

> strict mode violation: `locator('iframe[title="PDF preview"]')` resolved to 2 elements

A rerun passed; other branches pass. The job is `continue-on-error: true`
(`.github/workflows/q2-preview-spa-e2e.yml:59`), so it does not block merges.

## Dependency graph

- **discovered-from**: bd-7gqr148v (lmd AST desugar contract, in_progress). This only records which
  session saw the flake while babysitting CI. The two strands share no code and no design.
- No `blocks`, `related` or parent edges. Nothing depends on this, so the only pressure is CI noise.

## What the code looks like today

- `ts-packages/pdf-viewer/src/pdfViewer.ts:10-13, 162-184`: **by design**, a recompile loads into a
  *second* `iframe` titled `PDF preview` (z-index 0, underneath). It is promoted (z-index 2), and the old
  frame removed, only after pdf.js has painted the current page at its final layout (invariant from
  bd-c72wsugj: the frame on top is never an empty viewer). So **every** recompile has a window with two
  matching iframes, lasting from pdf.js start-up until first paint: hundreds of ms locally, seconds on a
  loaded CI runner. This is not a bug in PdfPane. The issue's "make PdfPane remove the old iframe
  atomically" alternative would bring back the flash the design exists to prevent, so it is out.
- `q2-preview-spa/e2e/typst-pdf-preview.spec.ts:51-52`: `textLayer()` =
  `page.frameLocator('iframe[title="PDF preview"]').locator('.textLayer')`, which is strict. Line 83's
  `toContainText('Beta heading')` polls straight through the swap window. Playwright (1.60) raises a
  strict-mode violation as an error, not as a "not yet" result to retry, so the test passes only when
  every poll misses the window. That fits "fails on a slow runner, passes on rerun": the longer the swap,
  the likelier a poll hits it, and all 3 retries ran on the same slow runner. *(The claim that strict
  violations aren't retried is from memory of Playwright's behaviour; Phase 0 confirms it.)*
- Other uses of `textLayer()` in the same spec: lines 74, 89, 95, 116, 127, 128. They are only exposed if
  something recompiles while they poll (for example a second `contentTick` right after the first load). The
  422 path in the "broken edit" test never swaps, because nothing new is loaded.
- `q2-preview-spa/e2e/embed-typst-pdf.spec.ts:60-67` (hub-client embed) already sidesteps this with
  `frameLocator(...).first()`. During a swap the **live frame is always first in DOM order**: `current`
  was appended before any `pending`, and a new pending is always appended last. So `.first()` is the
  correct "live frame" selector today, but only because of an implementation detail nobody wrote down.
  `pdfText()` in that spec also reads the old frame until the swap completes. That's harmless under its
  `expect.poll`.
- `allPdfText()` (spec line 56) uses `page.frames().find(...)`, which takes the first match and is never
  strict, so it is fine for the same DOM-order reason.
- hub-client unit tests (`hub-client/src/pandoc/pdfViewer.test.ts:65,114,135`) count iframes by tag and
  never use the title. A viewer-side change would have to keep them passing (and might extend them).

Not reproduced locally. The window is timing-dependent, and the code reading above explains the error
message exactly. A deterministic repro is possible, though: slow the pending frame's first paint (for
example with a route that delays `/pdfjs/web/viewer.mjs` for the second load) and assert on the text layer
during the window. That is Phase 0's test.

## Proposed phases (draft)

- **Phase 0 — Failing test.** Make the swap window long and deterministic in the e2e (delay the second
  viewer load via `page.route`, or slow pdf.js's first `pagerendered`). Show that line 83's assertion fails
  with the strict-mode error, then that it passes after the fix.
- **Phase 1 — Scope the locator to the live frame.** Option A or B below (design question 1).
- **Phase 2 — Align the embed spec** (`embed-typst-pdf.spec.ts`) to the same helper or selector, so the
  two specs don't drift. Optional.
- **Phase 3 — Docs.** If B: one paragraph in the `pdfViewer.ts` header naming the marker as the contract
  for "which frame is live". No user-facing docs or changelog.

Options for Phase 1:

- **A. Test-only.** `page.locator('iframe[title="PDF preview"]').first().contentFrame()`, with a comment
  saying that during a swap the live frame comes first in DOM order. One-line change; no product code
  touched; relies on the undocumented DOM-order fact.
- **B. Mark the live frame in the viewer.** The pending frame gets no `title` (or a distinct one) and
  `aria-hidden="true"`/`inert` until it is promoted; promotion sets `title="PDF preview"`. The test's
  existing selector then matches exactly one frame with no change. This also fixes a small accessibility
  issue: screen readers currently see two frames titled "PDF preview", one of them hidden underneath and
  half-loaded. Cost: touches `@quarto/pdf-viewer`, which both hub-client and the SPA use, so it needs
  unit tests in `hub-client/src/pandoc/pdfViewer.test.ts`.

## Open design questions for the user

1. **Test-only (A) or viewer-side marker (B)?** B fixes the a11y issue and turns "which frame is live"
   into an explicit contract. A is the minimal CI fix. My recommendation: B. The swap is a deliberate,
   long-lived design, and anything that inspects the pane (tests, assistive tech) should see one frame.
2. **If B: drop the title on the pending frame, or rename it** (e.g. `PDF preview (loading)`)? Dropping
   it and adding `aria-hidden` hides it from the accessibility tree completely. Renaming keeps it findable
   by tests that want to watch the pending load (none do today).
3. **Include the embed spec (Phase 2) in this strand**, or leave it, since `.first()` works there today?

## Risks / tradeoffs (draft)

- With option A, a future change to the swap order (e.g. inserting the pending frame *before* the live
  one) would silently make `.first()` read the pending frame. Detecting that would take a dedicated test.
- With option B, setting the title after load means code that waits for `iframe[title="PDF preview"]`
  to *appear* now waits until first paint instead of until creation. Today that is only the embed spec's
  `toBeAttached` (90 s timeout), which is fine.
- The Phase 0 repro depends on being able to slow one of two identical viewer loads through `page.route`.
  If that turns out to be fiddly, settle for a unit-level assertion on the viewer (B) and rely on the
  e2e as-is.
