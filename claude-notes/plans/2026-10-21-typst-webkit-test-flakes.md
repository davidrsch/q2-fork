# CI test flakes: typst CDN fetch + WebKit pdf-preview status (bd-qr8ryv79, bd-ynqwyyi7)

Date: 2026-10-21
Branch: `bugfix/bd-qr8ryv79-bd-ynqwyyi7-test-flakes`
Strands: bd-qr8ryv79, bd-ynqwyyi7 (both discovered-from bd-yx63o8wo, observed on PR #874)

## Overview

Two unrelated CI flakes observed on the same PR run:

1. **bd-qr8ryv79** — `quarto-preview::integration
   book_downloads::an_orange_book_chapter_downloads_as_a_pdf` failed on
   macOS CI when `packages.typst.org` DNS resolution failed. Root cause: the
   orange-book template (`typst-show.typ` in the embedded extension subtree)
   imports `@preview/orange-book:0.7.1`, which is **not** among the 5
   vendored packages in `resources/typst-packages/`. `TypstCompileStage`
   stages the vendored set into a fresh per-render temp cache, then
   `typst_gather::gather_packages` downloads anything missing — no retry, no
   offline fallback. Every orange-book render requires network.

2. **bd-ynqwyyi7** — hub-client e2e WebKit harness,
   `pandoc-pdf-viewer.harness.spec.ts:112`:
   `getByTestId('pdf-preview-status')` not visible within 10 s. The 10 s
   budget is the tightest assertion in the file (all siblings are 30-120 s);
   on WebKit CI runners the recompile chain (React render → 100 ms debounce
   → worker acquisition → `working` phase) can exceed 10 s before the status
   appears. The Playwright retry got past that point but failed on
   `__empty==2` (the no-empty-frame flash guard) — a separate, watch-listed
   symptom (filed as its own strand).

## Fix decisions

### Flake 1: vendor orange-book 0.7.1 (offline fixture, per the strand)

Vendor the package into `resources/typst-packages/packages/preview/orange-
book/0.7.1/`, copying from the identical bytes already in the repo at
`resources/extension-subtrees/orange-book/_extensions/orange-book/typst/
packages/preview/orange-book/0.7.1/` (MIT-0 license).

- **Working set only, per repo convention**: the 5 existing vendored
  packages ship no `template/` dir or `thumbnail.png` (only needed by
  `typst init`, which q2 never runs). Vendor `typst.toml`, `lib.typ`,
  `my-outline.typ`, `my-index.typ`, `theorems.typ`, `LICENSE`, `README.md`
  (~52 KB, not the full 728 KB). `lib.typ`\'s only asset reference is a
  commented-out `images/background2.jpg` line.
- **Dependency closure is complete**: orange-book's own `.typ` files have no
  `@preview` imports; `typst-show.typ`\'s other import,
  `@preview/marginalia:0.3.1`, is already vendored.
- Mechanism: `extract_typst_packages` (bundle.rs) stages the vendored set
  via `include_dir!`; `gather_packages` skips any package whose cache dir
  exists (typst-gather lib.rs:419 `cached_path.exists()`, no network).
- Rejected alternatives: (a) CDN-reachability test gate — silently passes
  during real outages; (b) retry with backoff — helps transient DNS but not
  genuinely offline runners, and is cross-crate scope; (c) staging from the
  already-embedded extension subtree — couples typst staging to extension
  internals and the subtree is `cfg(not(wasm))`-gated, unlike
  `TYPST_PACKAGES_DIR`.

### Flake 2: raise the status-visibility timeout to the file's own scale

Line 112 `{ timeout: 10_000 }` → `60_000`, matching lines 87/113/115.
Late appearance is benign; absence during compile is still guarded by line
113. Repo convention is generous explicit per-assertion timeouts (no
`test.slow()` anywhere in hub-client). The `__empty==2` flash-guard hit on
the retry is **not** weakened — filed separately.

## Checklist

### Phase 1 — bd-qr8ryv79 (Rust)

- [x] Write failing test in `typst_compile.rs` tests: extract vendored
  packages, run `gather_packages` against a `.typ` input with the
  orange-book template's exact `@preview` import set, assert
  `stats.downloaded == 0 && stats.failed == 0`
- [x] Run test, verify it fails (downloads orange-book today) — failed with
  `stats.downloaded == 1`; log shows `Downloading @preview/orange-book:0.7.1`
- [x] Vendor the 7 files into `resources/typst-packages/packages/preview/orange-book/0.7.1/`
- [x] Update `resources/typst-packages/README.md` (6 packages, orange-book
  provenance + template/ exclusion) and the "5 vendored" comments in
  `bundle.rs`, `pandoc_filters/mod.rs`, `typst_compile.rs`
- [x] Run test, verify pass (0.044 s, no network); `cargo nextest run -p
  quarto-core typst_compile` (25/25); clippy clean. NOTE: adding files to an
  `include_dir!` directory does not trigger a rebuild — had to `touch
  pandoc_filters/mod.rs` to re-glob. Pre-existing footgun affecting every
  embedded resource dir.
- [x] Run the flaky test itself: `cargo nextest run -p quarto-preview --test integration book_downloads` (12/12)
- [x] E2E: `cargo run --bin q2 -- render` the orange-book fixture to PDF —
  produced `_book/Test-Typst-Book.pdf`, a valid 28-page PDF; output inspected

### Phase 2 — bd-ynqwyyi7 (hub-client)

- [x] Bump `pandoc-pdf-viewer.harness.spec.ts:112` timeout to 60_000
- [x] `npm run build:all` (required for hub-client changes) — green
- [x] Run the spec locally: chromium 2/2 (6.2 s); `--project=webkit --workers=1` 2/2 (11.4 s)

### Phase 3 — bookkeeping + commits

- [x] File strand for the WebKit `__empty==2` flash-guard observation
  (discovered-from bd-ynqwyyi7) — bd-e09w5yhd
- [ ] braid comments on both strands
- [ ] Pre-commit review checklist (`claude-notes/instructions/review.md`)
- [ ] Commit 1: Rust fix (bd-qr8ryv79); commit 2: hub-client fix
  (bd-ynqwyyi7); commit 3: `hub-client/changelog.md` entry with commit-2
  hash
- [ ] Ask before pushing

## Incidental findings

- Full `quarto-core` + `quarto-preview` suites: 6533 pass; the one failure
  (`editor_ui_serves_pdfjs_from_the_viewer_bundle`, 404 on
  `/pdfjs/web/viewer.html`) was stale **gitignored** local dist artifacts
  (`hub-client/dist-preview-embed` from Aug 27, predating the pdfjs tree;
  `q2-preview-spa/dist` missing pdfjs). CI never sees it: test-suite.yml
  builds no dist, so the embed is a placeholder and the test skips; the
  release pipeline builds fresh dists. Fixed locally with
  `cargo xtask build-q2-preview-spa` + `npm run build:preview-embed`;
  quarto-preview then 290/290.
