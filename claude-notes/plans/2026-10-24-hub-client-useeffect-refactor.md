---
title: 'hub-client `useEffect` audit: phased refactor'
date: 2026-10-24
description: 'Phased refactor of the refactor-worthy useEffect sites in hub-client: stale-dep fixes, useSyncExternalStore conversions, derived-state elimination, then measured wide-dep performance work.'
status: in-progress  # Phases 1–4 done and open as stacked PRs (#924, #925, #931, #936). Phase 5 not started.
---

## Overview

A static audit (2026-10-24) inventoried 160 `useEffect` call sites across 67
files in `hub-client/src` (plus four `useLayoutEffect` sites: three in
`Menu.tsx`, one in `hooks/usePresence.ts`). Roughly 60 are legitimate external-system
synchronization — Monaco listeners, Automerge subscriptions, window/document
events, timers with proper cleanup — and stay as-is. About 30 sites fall into
four refactor-worthy buckets (Phases 1-4), plus ~12 ref-mirror effects swept
optionally in Phase 5. The remaining ~55 are small single-purpose effects
(one-shot initialization, unmount-only cleanup, already-correct prop or
preference mirroring) that the audit reviewed and left as-is. This plan
sequences the work reliability-first, performance-second, with empirical
measurement gating the performance work per
`claude-notes/instructions/performance-profiling.md`.

Context: React 19.2 (so stable `useEffectEvent` is available),
`react-hooks/exhaustive-deps` lint is active with 8 suppressions, and six
files already use `useSyncExternalStore` successfully (`useMediaQuery`,
`useDownloadAs`, `downloadController`, `pwaPrompt`, `UpdateAvailableToast`,
`DocumentViewer`), giving a proven in-repo template.

Caveat: findings are from static analysis. The per-keystroke cost claims in
Phase 4 are hypotheses until instrumented. Except for `App.tsx:830` (a
confirmed stale-closure bug — see Phase 1), the Phase 1 items are not known
user-visible bugs today; they are latent fragility.

File:line references are as of the audit; re-locate before editing. All
references were re-verified against the tree on 2026-10-26 (one audit false
positive, `FileSidebar.tsx:280`, was dropped from Phase 1).

## Conventions

- TDD per repo policy: behavior-changing fixes (Phase 1) get a failing
  regression test first. Pure refactors (Phases 2, 3, 5) rely on the existing
  suite as the safety net — where a site lacks coverage, add a
  characterization test before touching it.
- After each phase: `npm run test:ci` and `npm run build` from `hub-client/`.
  The production build (`tsc -b && vite build`) is stricter than
  `tsc --noEmit` and `vitest`; passing tests alone are not sufficient. Run
  the full `npm run build:all` (fetch:pandoc + WASM + sandboxed + precache)
  once at plan completion — no phase touches WASM or sandboxed inputs, so
  per-phase `build:all` buys nothing over `build`. (A deliberate deviation
  from a literal per-phase reading of the AGENTS.md `build:all` rule; "work
  is done" here means plan completion.)
- Batch same-file items into one change rather than one pass per phase:
  `NewAssetDialog.tsx:66` and `:79` are the same effect (the Phase 3
  `key`-remount deletes the Phase 1 suppression), `ShareDialog.tsx:36`/`:43`
  are one small component, and `Editor.tsx` has eight sites across Phases
  3-5 (:334, :379, :569, :743, :748, :765, :777, :1400) — one Editor-focused
  branch with shared characterization tests beats three separate passes.
- Anything touching the sync/preview paths also gets a manual smoke in the
  dev server; if browser verification was not done, say so explicitly in the
  completion report (per AGENTS.md end-to-end verification rules).
- File braid strands for each phase when work begins and reference this plan.

## Phase 1 — Correctness: stale deps, suppressions, leaks

**Done 2026-10-27 (bd-lsf2v5n0); open as PR #924.** Findings that shaped the landing:

- The App.tsx:830 fix landed as a factory
  (`src/services/connectionHandlers.ts#createConnectionChangeHandler`)
  rather than a purely inline functional updater: every exit path from an
  open project clears `connectionError`, so no UI-level test can observe
  the stale value — the extracted factory is the smallest testable seam.
  The stale-closure trap is reproduced by `connectionHandlers.test.ts`
  (long-lived handler over real React state); `App.test.tsx` covers the
  wiring (isOnline flips through the registered handler).
- ShareDialog's 100 ms select-all turned out to be **dead code**, not
  merely uncleaned: ModalDialog mounts the input a commit after the
  dialog, so the `[isOpen]` effect ran with a null ref and never
  scheduled. The fix tracks the input as state (`useState` as the ref
  callback) so the effect re-runs when the input mounts, and adds the
  cleanup — one rewrite closes both the dead-timer and no-cleanup
  defects.
- Suppression triage: `App.tsx:570` and `useAutoEstablishRoot.ts:88`
  became honest deps (the action methods are stable `useCallback`s — the
  actions *object* is rebuilt per render, which is why the object itself
  must stay out of dep arrays); `Menu.tsx:159` gained the stable `items`
  dep; `Menu.tsx:119`/`:205` and `DebugApp.tsx:162` keep their
  suppressions, now with written justifications. `NewAssetDialog.tsx:79`
  remains deferred to Phase 3.
- Pre-existing, unrelated: `pandocBook.wasm.test.ts` typst golden-parity
  fails on clean `origin/main` (announcement-string skew vs PR #922) —
  filed as a discovered strand.

- [x] `src/App.tsx:340` — effect reads `auth?.name` but deps are
  `[authLoading]` only. Works today because auth and loading flip in the same
  batch; a later auth change (re-login) silently fails to re-seed the screen
  name. Fix by depending on `auth` (guard against redundant IndexedDB writes)
  or restructuring. Regression test: auth change after initial load updates
  the screen name.
- [x] `src/App.tsx:830` — sync-callback registration effect has deps
  `[project]` but closes over `connectionError`. **Confirmed user-visible
  bug** (verified 2026-10-26): `onConnectionChange(true)` compares against
  the `connectionError` frozen when the effect last ran, so the "Connection
  lost - working offline" banner never clears on reconnect. Simplest fix: a
  functional updater (`setConnectionError(prev => prev === OFFLINE_MSG ?
  null : prev)`) so the callback never reads the value — no dep churn, no
  suppression, no `useEffectEvent` needed. Regression test: connection lost
  then restored clears the banner.
- `src/components/FileSidebar.tsx:280` — dropped on 2026-10-26 verification:
  the body reads only `currentFile.path`, so the `currentFile?.path` dep is
  already aligned (audit false positive).
- [x] `src/components/ShareDialog.tsx:43` — the 100 ms select-text timeout on
  open has no cleanup; clear it on close/unmount. Do this in the same pass
  as the Phase 3 `key`-remount of the same dialog (`ShareDialog.tsx:36`).
- [x] Triage the 8 `eslint-disable-next-line react-hooks/exhaustive-deps`
  sites: `App.tsx:570`, `Menu.tsx` (:119, :159, :205),
  `NewAssetDialog.tsx:79`, `useAutoEstablishRoot.ts:88`,
  `src/debug/DebugApp.tsx:162`, and the `useCallback` suppression at
  `ProjectsHome.tsx:1113`. For each: express the intent with `useEffectEvent`
  and remove the suppression, or add a comment documenting why the
  suppression is correct. Exception: `NewAssetDialog.tsx:79` sits on the
  same effect Phase 3 converts to a `key` remount — the remount deletes the
  suppression, so handle it there, not here.

## Phase 2 — `useSyncExternalStore` conversions

**Done 2026-10-27 (bd-06s9sufx); open as PR #925, stacked on #924.** Findings that shaped the landing:

- The `branchService` prerequisite landed as a version counter plus per-path
  snapshot cache (`getBranchSnapshot(path)`), covering `branches` and
  `activeBranchId` in one object. Every mutation already funneled through
  `notify()`, which now bumps the version; the cache also re-resolves when
  the path\'s docId changes (a file handle can appear without any
  branchService event, and must not pin the empty snapshot). The
  push-model escape hatch was not needed.
- `usePresence` needed the same memoization one layer down:
  `getRemotePresences()` returned a fresh array per call, so the service
  now memoizes it on a version bumped at the top of `notifySubscribers` —
  before the visibility gate, so data freshness is not deferred even when
  subscriber callbacks are. One intended semantic difference from the old
  push model: an unrelated re-render while the tab is hidden now reads
  fresh presence data rather than the last pushed array (fresher, never
  staler). `enabled: false` keeps its old behavior via a shared
  `EMPTY_REMOTE_USERS` snapshot.
- `usePreference` converted directly: all preference values are primitives,
  so `getPreference(key)` is a stable snapshot by value, and the writer\'s
  local echo was redundant (`setPreference` dispatches synchronously).
- `useNow(intervalMs)` is one shared ticking store per interval (interval
  runs only while subscribed; clock refreshes on resubscribe so a remount
  never reads a stale timestamp). `useOnlineStatus()` is the textbook
  `navigator.onLine` + online/offline `useSyncExternalStore`. Both
  `SyncStatusBadge` and `ConnectionStatusDialog` now use both hooks; the
  badge\'s old immediate-refresh-on-online-event is approximated by the
  connectivity flip re-rendering and the clock catching up within one
  tick (≤ 1 s, imperceptible for "n minutes ago" text).
- Characterization tests were pinned before touching each uncovered site:
  `useDocBranches.test.tsx` and `ConnectionStatusDialog.test.tsx` (the
  dialog had no coverage). A stale-cleanup memoization test was drafted
  and dropped: existing `stale presence cleanup` tests already cover
  expiry, and its interval predates `vi.useFakeTimers()` in the test.
- Verification: `npm run test:ci` unit and integration legs green; the
  WASM leg has 3 golden-parity failures (`pandocBook`, `pandocRequest`,
  `smokeAll`) that reproduce identically on the clean tree — the
  pre-existing announcement-string skew Phase 1 documented, unrelated to
  this phase. **Root-caused 2026-10-27 on rebase:** the skew was a stale
  local WASM build against post-announcement goldens; after `npm run
  build:wasm` all three files pass, and the full `test:ci` on the rebased
  stack is green (1720 unit + 225 integration + 434 WASM). `npm run
  build` green; eslint clean on touched files (one pre-existing
  `usePresence` ref-cleanup warning, unchanged). Not
  browser-smoked: no browser session was run for this phase (refactor
  with characterization tests; the badge/dialog render paths are covered
  by `SyncStatusBadge.test.tsx` and the new dialog test).

Only `useDocBranches` can actually tear under concurrent rendering: it bumps
a version counter in the subscription callback and re-reads `branchService`
during render. `usePreference` and `usePresence` instead push the data into
React state inside the callback, so their renders read React state — no
tearing; converting them is simplification, not a correctness fix, and they
are safe to defer. Replace with `useSyncExternalStore`, following the
existing in-repo template.

- [x] Prerequisite: `branchService` memoizes its snapshots (version counter
  plus cached per-path snapshot) so `getSnapshot` returns a stable reference
  — `getBranches(path)` currently returns a fresh array per call, which
  `useSyncExternalStore` cannot tolerate. The snapshot must cover everything
  the hook reads during render: `useDocBranches` also calls
  `getActiveBranchId(path)` at render time, so cache it in the same per-path
  snapshot (one object holding both). Escape hatch if the memoization proves
  invasive: a push model in `useDocBranches` — `useState` seeded from
  `getBranches(path)` and `getActiveBranchId(path)`, with `subscribe`
  updating both — also eliminates tearing and has no snapshot-stability
  requirement.
- [x] `src/hooks/useDocBranches.ts:15` — replace
  `useEffect(() => subscribe(...), [])` + `setVersion` with
  `useSyncExternalStore`.
- [x] `src/hooks/usePreference.ts:40` — convert the custom-event
  subscription (simplification, not a tear-fix; safe to defer or drop).
- [x] `src/hooks/usePresence.ts:185` — convert the `onPresenceChange`
  subscription (simplification, not a tear-fix; safe to defer or drop).
- [x] Add two shared hooks: `useNow(intervalMs)` (one ticking store on
  `useSyncExternalStore`) and `useOnlineStatus()` (`navigator.onLine` plus
  online/offline events — a textbook `useSyncExternalStore`). Replace the
  duplicated 1 s ticker + online/offline listener blocks in
  `SyncStatusBadge.tsx:85` and `ConnectionStatusDialog.tsx:217` — both
  components need both hooks: each ticks the clock, and each reads
  connectivity during render (`SyncStatusBadge.tsx:122` reads
  `navigator.onLine` at render time, today freshened only by the ticker
  re-render; `useOnlineStatus()` turns that into a proper subscription).
- [x] `src/components/debug-inspector/DebugInspectorPanel.tsx:75` and `:99` —
  document why polling is intentional in a debug-only panel (a code comment
  is enough); do not convert.

## Phase 3 — Derived state out of effects

**Done 2026-10-27 (bd-cp9wzu6o); open as PR #931, stacked on #925.** Findings that shaped the landing:

- `Editor.tsx` had zero direct test coverage, so the phase starts with a
  characterization harness: `src/components/Editor.characterization.test.tsx`
  (Monaco, the preview router, and the sync/presence/intelligence hooks
  mocked; BranchBar and the banner real) pinning the three refactored
  behaviors. Phases 4 and 5 extend it rather than building their own.
- The dialog resets landed as **conditional mounts at the call sites**
  (`{open && <Dialog … />}`) instead of a `key` counter — same
  fresh-mount-per-open semantics with no extra parent state. Call sites
  updated: Editor (all three dialogs), ProjectSelector and ProjectsHome
  (ShareDialog), and the DevHarness stateful NewFileDialog harness. The
  remount contract is pinned by open/dirty/close/reopen tests in each
  dialog\'s suite.
- `NewFileDialog` seeds from `initialFilename` in lazy `useState`
  initializers; `NewAssetDialog` ingests `initialFiles` in an initializer
  and keeps only a mount-only effect for the async FileReader thumbnails
  (shared with `ingestFiles` via `loadImagePreviews`) — the `:79`
  exhaustive-deps suppression is gone, closing the Phase 1 leftover.
- `ReactAstSlideRenderer`: uncontrolled mode self-corrects `internalSlide`
  during render; **controlled mode is display-clamped only** — the parent
  owns the index and self-corrects on the next navigation. The clamp effect
  is deleted, so a stale controlled index can no longer commit a `3 / 2`
  counter; pinned by a failing-first test (red before, green after).
- Editor diagnostics: `unlocatedErrors` is a `useMemo` over `diagnostics`;
  the effect keeps only the Monaco marker write, and the four
  `setDiagnostics([])` file-switch clear-sites no longer pair a
  `setUnlocatedErrors([])`.
- Default file selection, `comparingWithMain`, and ProjectsHome\'s
  `openingId` all use the React-docs render-adjust pattern (conditional
  setState during render, prev-value tracking where a transition matters).
- eslint on the touched files went from 23 errors / 7 warnings at the
  Phase 2 tip to 13 / 6 — every remaining issue pre-exists this branch
  (react-hooks v6 rules against untouched code).
- Verification: `npm run test:ci` all three legs green (1728 unit +
  229 integration + 434 WASM, 28 skipped); `npm run build` green. Not
  browser-smoked: no browser session was run (pure refactor behind
  characterization tests; no sync/preview-path behavior change).

Effects that compute state from other state cause an extra render pass and
can surface stale intermediate UI. Move the computation to `useMemo`, to
render-time adjustment, or to `key` remounts. Per the Conventions batching
rule, the `Editor.tsx` items here share one branch and one
characterization-test setup with the Phase 4 and Phase 5 `Editor.tsx` items.

- [x] `src/components/Editor.tsx:748` — split the diagnostics effect:
  `unlocatedErrors` becomes a `useMemo` over `diagnostics`; the effect keeps
  only the Monaco marker write. Eliminates a two-render derived-state chain.
- [x] `src/components/Editor.tsx:765` — default file selection: adjust state
  during render (the prev-value-tracking pattern) instead of
  setState-in-effect; `currentFile` stays user-settable state.
- [x] `src/components/render/ReactAstSlideRenderer.tsx:90` — clamp the slide
  index during render (adjust-state-during-render pattern).
- [x] `src/components/Editor.tsx:334` — `setComparingWithMain(false)` on
  file/branch switch: `key` remount or render-time adjust.
- [x] Dialog form resets via `key` remounts: `NewFileDialog.tsx:108`,
  `NewAssetDialog.tsx:66` (also deletes the `:79` exhaustive-deps
  suppression — one change closes two Phase 1 items), `ShareDialog.tsx:36`
  (same pass as the Phase 1 timeout cleanup).
- [x] `src/components/ProjectsHome.tsx:697` — clearing the opening spinner on
  `connectionError`: render-adjust candidate.

## Phase 4 — Wide-dependency performance (measure first)

**Done 2026-10-27 (bd-wfhd0dg5); open as PR #936, stacked on #931.** Findings
that shaped the landing:

- The instrumentation helper landed as `src/utils/perfCounters.ts`: enabled
  by `QUARTO_PERF_STATS=1` (Node drivers), `localStorage.quartoPerfStats=1`,
  or `?perf=1`, read once at startup; recording is one cached-boolean
  branch when off. Dumps are cumulative, machine-readable
  (`perf.<gauge> runs=N total_ms=X avg_ms=Y max_ms=Z [k=v ...]`), every
  5 s and via `window.quartoPerfDump()` in the browser. All seven sites
  below are instrumented and the gauges stay in the tree (same pattern as
  the Rust `perf.intern` counters).
- Measurement ran in throwaway Node drivers (tsx + jsdom +
  `@testing-library/react`, per the TS section of the profiling playbook)
  with the **real hooks**, a **real Automerge doc** behind `getFileContent`
  and the **real WASM `vfs_add_file`** behind the VFS shim. Monaco is
  emulated: `getValue()` returns a fresh O(n) string copy (an upper bound
  on the real piece-tree concat), `executeEdits` applies in O(1). The
  keystroke scenario is local typing on main: splice lands in the CRDT,
  Monaco already shows the char, React commits a fresh `fileContents` Map.
- **Verdict: no wide-dep fix is warranted anywhere.** Numbers below; the
  constraint on `Preview`/`ReactPreview` is moot since nothing is narrowed.

Reconcile effect (`useAutomergeSync`), per-keystroke avg vs document size
(40 keystrokes per scale after warmup):

| doc size | effect total | monaco seg | vfs seg | AM read |
|---|---|---|---|---|
| 20 KiB | 0.073 ms | 0.047 | 0.023 | ~0 |
| 41 KiB | 0.127 | 0.086 | 0.039 | ~0 |
| 82 KiB | 0.274 | 0.182 | 0.090 | ~0 |
| 163 KiB | 0.535 | 0.380 | 0.151 | ~0 |
| 327 KiB | 1.023 | 0.720 | 0.299 | ~0 |

O(n) confirmed (each doubling ~doubles the cost) with a small constant:
≤ 0.27 ms/keystroke at realistic sizes (≤ 82 KiB ≈ 2000 lines). The
Automerge read is ~free because the sync client materializes the text for
its own change callback, so the effect\'s `getFileContent` hits the
same-version cache. The VFS write *is* fully redundant on main (the sync
client\'s `onFileChanged` writes the same path first — bd-4jjckvwt), but at
≤ 0.3 ms even for a 327 KiB document, the branch-transition-tracking needed
to skip it safely is not worth the correctness risk.

Project search (`useProjectSearch`), per-keystroke avg vs project size:

| files | corpus rebuild | sync (1 changed file) | total |
|---|---|---|---|
| 25 | 0.006 ms | 0.902 | 0.911 |
| 100 | 0.008 | 0.686 | 0.696 |
| 400 | 0.030 | 0.790 | 0.828 |

The wide-dep cost (corpus-map rebuild) is ≤ 30 µs at 400 files. The
~0.7–0.9 ms is MiniSearch reindexing the single changed 10 KiB file —
intrinsic to a live index and independent of dep shape (as the plan
predicted: no fix needed).

Trivial-body class (route-nav, replay-VFS restore, both preview schedulers,
the intelligence refresh): five such effects together cost 0.036 ms per
keystroke *including* the React commit; bodies ≤ 1 µs each, size-independent
by inspection. `Preview`/`ReactPreview` also gain no extra effect fires
from the wide dep for active-file typing: `content` and `fileContents`
update in the same commit, so the effect runs once either way; sibling-edit
fires are the load-bearing intent (bd-4jjckvwt). No fix anywhere in this
class.

Two items resolved without measurement:

- `useSectionThumbnails` turned out to be **dead code** — no caller in the
  tree; `OutlinePanel` imported only its `ThumbnailMap` type, and the live
  thumbnail path (`useSlideThumbnails`, used by `Editor`) already has the
  100 ms debounce. Deleted the hook and retargeted the type import —
  closes the item more definitively than adding a debounce to dead code.
- `Editor.tsx:743` (now :759) `refreshIntelligence`: `useIntelligence`\'s
  `refresh` already debounces at 300 ms internally (`refreshTimeoutRef`),
  so the per-keystroke cost is one `clearTimeout`+`setTimeout` — the
  trivial-body class. No fold-in needed.

Verification: `npm run test:ci` all three legs green (1736 unit +
229 integration + 434 WASM, 28 skipped); `npm run build` green; eslint on
touched files at exact parity with the Phase 3 tip (9 errors / 7 warnings,
all pre-existing react-hooks v6 complaints against untouched code). Not
browser-smoked: the only behavior change is a dead-code deletion; the
instrumentation is inert unless enabled. The counters can be exercised in
any dev session via `localStorage.quartoPerfStats = '1'`.

Five effects depend on the whole `fileContents` map and therefore rerun on
every keystroke in any file. Per the performance-profiling workflow: build a
representative fixture, scale geometrically, add gated counters with a
`perf.<gauge-name>` output prefix, confirm the complexity class empirically,
then design the fix. Do not optimize in the browser. `QUARTO_PERF_STATS` is
a Rust-side env var with no presence in hub-client, so gate browser-side
counters on a URL param or `localStorage` flag (read once at startup, output
via `console`) — adding that tiny instrumentation helper is the first Phase
4 work item.

Constraint: for `Preview.tsx:367` and `ReactPreview.tsx:789` the wide dep is
load-bearing per the in-code comments — a sibling title edit shifts the
rendered website sidebar, and a synced `_brand.yml`/`_quarto.yml` change
recompiles theme CSS (bd-4jjckvwt). Never narrow those two to the current
file content; the only valid directions there are relevant-change detection
or moving invalidation into the sync-service callback.

- [x] Instrument the body and early-out path of each wide-dep effect:
  `src/hooks/useAutomergeSync.ts:221`, `src/components/Editor.tsx:777`
  (re-located to :789, the route-nav effect),
  `src/components/render/Preview.tsx:367`,
  `src/components/render/ReactPreview.tsx:789`,
  `src/services/search/useProjectSearch.ts:43`. Measured per-keystroke cost
  at increasing document sizes — see findings above.
- [x] ~~If measurement confirms the cost: narrow deps~~ — measurement did
  *not* confirm a significant cost at any site (see tables), so no dep was
  narrowed and no work was moved; the counters remain for future
  verification.
- [x] `src/hooks/useSectionThumbnails.ts:129` — hook was dead code; deleted
  (the debounce item is moot). `OutlinePanel`\'s `ThumbnailMap` import
  retargeted to `useSlideThumbnails`.
- [x] `src/components/Editor.tsx:743` — confirmed `refreshIntelligence`
  debounces at 300 ms inside `useIntelligence`; no fold-in needed.

## Phase 5 — `useEffectEvent` sweep (optional, low priority)

- [ ] Replace the ~12 ref-mirror effects
  (`useEffect(() => { ref.current = handler })`) where the ref exists only to
  give an effect or listener the latest handler: `useAuthProbe.ts:44`,
  `useSessionKeepAlive.ts:51`, `useWindowFileDrop.ts:22`, `useRouting.ts:80`,
  `Editor.tsx:379`, `Editor.tsx:569`, `Editor.tsx:1400`, `Preview.tsx:198`,
  `ReactPreview.tsx:514`, `useSlideThumbnails.tsx:182`,
  `useDownloadAs.ts:43` and `:47`.
- [ ] Leave alone any effect that intentionally runs on every render unless
  `useEffectEvent` semantics match exactly; convert only where the
  replacement is behavior-preserving.

## Out of scope

- The ~60 external-system sync effects (Monaco listeners, Automerge
  subscription/reconcile, window/document listeners, timers with cleanup) —
  correct `useEffect` usage; rewriting adds risk with no payoff.
- `src/components/render/experimental-components/**` — runtime-loaded demo
  fixtures, not app source.

## Verification

- Per phase: `npm run test:ci` and `npm run build` from `hub-client/`; both
  must be green before the phase is reported done. Full `npm run build:all`
  once at plan completion (see Conventions).
- Phase 1 items land with their regression tests (TDD ordering).
- Phase 4 fix lands only with before/after counter data in the completion
  report.
- This plan renders clean: `cargo run --bin q2 -- render --strict
  claude-notes/plans/2026-10-24-hub-client-useeffect-refactor.md` (CI runs
  the strict render on every PR).
