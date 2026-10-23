---
title: 'Replay drawer UI redesign (bd-belnj273)'
date: 2026-10-23
description: 'Professional polish pass on the hub-client replay drawer, implementing the options approved in the 2026-10-23 HTML mockup review.'
status: implemented
braid:
  strand: bd-belnj273
  priority: P2
---

## Overview

The replay drawer (`hub-client/src/components/ReplayDrawer.tsx`) works but
reads as rough: Unicode transport glyphs that render inconsistently across
platforms, a waveform with hard-coded theme-blind colors (`#1f3460` track,
`rgba(0,0,0,.6)` dim, white hairline playhead — all inline in the JSX, not
tokens), a mobile-style drag handle as the only close affordance, an
all-caps "REPLAY" label, and a green "Restore" button that undersells the
one consequential action in the drawer.

An HTML mockup presenting six decision points was reviewed and the
recommended option approved for each, with one modification: D5-B ships
**without** a confirmation dialog (Automerge history is preserved on
restore, so the action is non-destructive).

Approved decisions:

| # | Decision | Approved option |
|---|----------|-----------------|
| D1 | Close affordance | **B** — explicit ✕ button, right-aligned, `Esc` hint in tooltip; drag handle removed |
| D2 | Expanded layout | **A** — keep two rows (title/info row + transport/timeline row), refined |
| D3 | Progress & playhead | **B** — theme-aware track, desaturated "future" at 30% opacity, 2px playhead with caret |
| D4 | Waveform bars | ~~B~~ **reverted in review** — gaps read as missing data at every density; columns tile contiguously as before |
| D5 | Restore action | **B** — teal primary "Restore this version" with undo icon; **no confirm dialog** (non-destructive) |
| D6 | Title | **B** — history icon + "Version history", sentence case |

Non-decision polish items folded in: SVG transport icons from `icons.tsx`
(design-system rule), keyboard-shortcut hints in tooltips, one size rhythm
(20px pills / 26px transport / 28px play / 26px timeline / 28px collapsed
bar / 72px expanded — the original compact heights, per review feedback),
`(you)` suffix on the current-user actor chip, and token-driven waveform
colors in `theme.css` (both themes).

## Checklist

### Phase 1 — Tests first (TDD)

- [x] Create strand (bd-belnj273), branch, plan file
- [x] Update `ReplayDrawer.test.tsx`: title "Version history", restore
      label, close button replaces handle, position format `43 / 100`,
      speed `1×`, waveform past/future clip groups, gaps, playhead, `(you)`
      suffix, tooltip texts, scrubber tooltip with change number
- [x] Run tests; verify the new/updated tests fail against the old code
      (22 failed, 45 passed — the expected red state)

### Phase 2 — Implementation

- [x] `icons.tsx`: HistoryIcon, ChevronLeftIcon, ChevronRightIcon,
      SkipBackIcon, SkipForwardIcon, PlayIcon, PauseIcon, XIcon, UndoIcon
      (+ gallery entries in `DevGalleryPage.tsx`)
- [x] `strings.ts`: title, restore label, tooltip strings
- [x] `theme.css`: `--replay-track`, `--replay-tooltip-bg`,
      `--replay-tooltip-text` in both themes; drop `--replay-handle*`
- [x] `ReplayDrawer.tsx`: new header (title + close ✕), transport with
      icons + tooltips, waveform (track class, past/future clip groups,
      gaps, playhead+caret overlay), position format, restore button,
      `(you)` chip suffix
- [x] `ReplayDrawer.css`: full style pass per mockup
- [x] `DevHarness.tsx`: sync the static "me"-chip replica with the new
      `(you)` suffix markup

### Phase 3 — Verification

- [x] `npm run test` (unit) green — 142 files / 1679 tests;
      `npm run lint:css`, ESLint, `npm run typecheck` clean (two stale
      entries removed from `lint-css-exceptions.json`)
- [x] `npm run build:all` green (hub-client rule)
- [x] Visual end-to-end: vite dev server + Playwright screenshots of
      `#/dev/replay` in both themes (the Positron Viewer was misbehaving —
      0x0 webview layout — so verification went through Playwright);
      output inspected, matches the mockup
- [x] Harness a11y baseline: full `baseline-a11y` + `gallery-states-a11y`
      suites green (74 passed, chromium, both themes)
- [x] `cargo xtask lint` green; `q2 render --strict claude-notes` green
- [x] Two commits (code `148124758`, then `hub-client/changelog.md` with
      the hash); report — **no push without approval**

## Notes

- Waveform SVG keeps the `0 0 100 1` viewBox (fractions stay 0..1) and
  columns tile contiguously — the D4-B hairline gaps were reverted after
  review (distracting at every density, not just sparse histories).
  Past/future split is two `<g>` copies of the chunk rects under
  `clipPath`s at the playhead x; the future group carries
  `opacity: 0.3` via CSS.
- The playhead is an HTML overlay (not SVG), so the caret and the 3px
  overshoot stay crisp under `preserveAspectRatio="none"` stretching.
- Tooltip colors invert per theme (dark chip on light, light chip on
  dark) — `--replay-tooltip-bg` / `--replay-tooltip-text`.
- Restore keeps calling `controls.apply()` directly (no dialog); the
  tooltip states that history is preserved.
- Transport is a uniform 24px ghost row (second review round): the filled
  teal play button made the drawer read as a media player and competed
  with Restore. The scrubber is the primary affordance; Restore keeps
  the drawer's only accent.
- Keyboard shortcuts and all existing aria-labels are unchanged; the ✕
  button reuses `replay.close` ("Close replay").
