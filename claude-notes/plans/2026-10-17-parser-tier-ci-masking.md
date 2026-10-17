---
title: 'Parser-tier CI step masks wasm-qmd-parser build failures (bd-yt3x1gwk)'
date: 2026-10-17
description: 'The quarto-hub-mcp parser-tier CI step reports green when wasm-qmd-parser fails to compile, because the vitest global setup turns every build failure into a skip; make the build fatal on legs that declare the toolchain present.'
status: draft  # Investigation — pending design alignment with user; do not implement before the go-ahead
braid:
  strand: bd-yt3x1gwk
---

**Branch:** `braid/bd-yt3x1gwk-pampa-fails-compile-without` (topic branch in the main checkout, based on `main` @ `ad283a2cb`)
**Do not start implementation until the user gives the go-ahead.**

## Triage verdict

**Ready to design.** Defect 1 (the compile break) is fixed and v0.34.0 shipped. Only defect 2 remains: CI swallowed the break. The mechanism is understood. The repo already has a precedent for the fix (`TYPST_PARITY_REQUIRED`). The design questions below are small.

## Issue context

Filed 2026-10-17 by Carlos (P1 bug, labels `ci`, `release`). The strand records two defects:

1. **pampa does not compile without `lua-filter`.** `ModuleCompileError::find_in` (73588f3e5, bd-76ogq4ew) was in an ungated `impl`. The bug blocked the v0.34.0 release in `web-payloads`. **Done:** #862 (c73213d7a) fixed it. v0.34.0 was re-tagged at 8c1eed493 and published (strand comment, 2026-10-17).
2. **CI masked the break. Still open; this plan covers it.** Per-PR CI compiled pampa without `lua-filter` in the parser-tier step and the compile failed. The step still reported `success`. The PR #861 ubuntu log shows:
   ```
   [qmd-parser] unavailable (wasm-pack build failed for wasm-qmd-parser ...) — parser-tier tests will skip.
   ↓ src/qmd-ast.test.ts (21 tests | 21 skipped)
   ```

The strand also proposes an optional extra: a cheap `cargo check -p pampa --no-default-features` in the Rust suite.

Incidental, out of scope: `cargo check -p pampa --no-default-features --features terminal-support` fails (`unresolved import quarto_system_runtime`). No shipped consumer uses that combination.

## Dependency graph

- **discovered-from**: bd-76ogq4ew (in_progress). Its phase 1 introduced `find_in`. The release run surfaced the break, not the work on bd-76ogq4ew itself. That strand gives no design constraints for this one.
- No `blocks`/`related` edges. The release that created the urgency is out, so nothing downstream is waiting. P1 still fits: the same masking can hide the next feature-gating mistake until a release.

## What the code looks like today

All paths in the strand still exist with the same shape:

- `ts-packages/quarto-hub-mcp/scripts/vitest-global-setup.mjs`: `setup()` wraps `ensureQmdParserPkg({ quiet: true })` in a bare `try/catch`. Every throw writes `.qmd-parser-unavailable` and warns.
- `ts-packages/quarto-hub-mcp/scripts/qmd-parser-pkg.mjs`: `ensureQmdParserPkg` throws in three distinguishable situations:
  - `findLlvmBin()` on darwin: Homebrew LLVM missing. This is a toolchain gap.
  - `build()` with `res.error` set: `wasm-pack` not on PATH (spawn ENOENT). Also a toolchain gap.
  - `build()` with `res.status !== 0`: wasm-pack ran and failed. This covers real compile errors, but also a missing or wrong wasm32 clang on Linux, which surfaces as a cc-rs failure inside the build.
- `src/test-setup.ts` exports `PARSER_UNAVAILABLE` (the marker exists). Seven test files gate on it: `qmd-ast`, `qmd-outline`, `qmd-sections`, `conformance` (per case), `bundle`, `npm-pack`, `mcpb-pack`.
- `.github/workflows/ts-test-suite.yml`:
  - `test-suite` job (ubuntu + macOS) has the toolchain (Clang/LLVM, wasm-pack) and runs the step *Run quarto-hub-mcp parser-tier tests* (line ~254). This is where the skip is wrong.
  - `workspace-ts-suites` job is toolchain-free by design and runs `npm test -w ts-packages/quarto-hub-mcp` (line ~405). The skip is correct there.
- **Precedent:** the same workflow sets `TYPST_PARITY_REQUIRED=1`, and `ts-packages/typst-host/src/parity.test.ts:26,40` throws instead of skipping when it is set. The strand's `QUARTO_QMD_PARSER_REQUIRED=1` proposal follows that pattern.
- `cargo xtask verify` step 12 runs the quarto-hub-mcp tests locally, so local behaviour depends on whether verify sets the variable.
- The Rust `test-suite.yml` has no `--no-default-features` check for pampa. Its WASM job builds pampa with `--features lua-filter`, which is why it missed the break.

No repro artifact is needed. The CI log quoted in the strand is the repro. To simulate it locally, introduce a compile error in pampa under `cfg(not(feature = "lua-filter"))` and run the parser-tier vitest command: it reports skips and exits 0.

## Proposed phases (draft)

- **Phase 0: Test plan.** Unit-test the setup decision (required vs. not × success / toolchain-missing / build-failed). This probably means extracting a pure `decide(err, required)` helper from `vitest-global-setup.mjs` that vitest can test without wasm-pack.
- **Phase 1: Make setup fatal when required.** Read `QUARTO_QMD_PARSER_REQUIRED`. When it is `1`, rethrow (vitest aborts the run) and do not write the marker. Also delete any stale marker, so a later run cannot inherit a skip.
- **Phase 2: CI wiring.** Set the variable in the parser-tier step's `env:` on both matrix legs. Update the step comment and the setup's doc comment, which currently describe the skip as unconditional.
- **Phase 3: cheap Rust guard, in both CI and verify (decided, Q3).** Check pampa natively under each feature set its out-of-workspace consumers use, so a feature-gating mistake fails in seconds, locally and in CI. See "Q3 resolution" below for the measured cost and the open sub-question on how to list the feature sets.
- **Phase 4: Docs.** Note the variable in `crates/wasm-qmd-parser/AGENTS.md` and/or the quarto-hub-mcp README next to the existing toolchain notes.

## Open design questions for the user

1. **Opt-in variable, or classify the error?** Option A: `QUARTO_QMD_PARSER_REQUIRED=1` (the strand's proposal, mirrors `TYPST_PARITY_REQUIRED`). Option B: skip only on "toolchain missing" (LLVM not found, wasm-pack ENOENT) and fail on any non-zero wasm-pack exit. B also protects local dev, but on Linux a missing wasm32 clang shows up as a non-zero exit, so B would break toolchain-less Linux developers. I recommend **A, optionally combined with B**: fail when required, and otherwise keep skipping but print the full error.
2. **Should `cargo xtask verify` set the variable?** Pro: local verify catches the break before CI does. Con: verify would then fail for anyone without LLVM/wasm-pack, unless it sets the variable only after probing for the toolchain. My lean: don't set it in verify. Instead add the cheap `cargo check --no-default-features` from Q3, which needs no wasm toolchain.
3. **Include the Rust `--no-default-features` check?** *Resolved (2026-10-17): yes, in both CI and `cargo xtask verify`.* The goal is that a PR whose CI and local verify pass cannot then fail the nightly/release build. Verify is how developers see CI's result ahead of time, so the guard belongs in both. Sub-question 3a below remains open.

   **3a. How should the guard list the feature sets?** These are the out-of-workspace consumers that turn pampa's defaults off:

   | Consumer | pampa features |
   | --- | --- |
   | `crates/wasm-qmd-parser` | none |
   | `crates/wasm-quarto-hub-client` | `lua-filter` |
   | `crates/experiments/reconcile-viewer` | none (same set as wasm-qmd-parser) |

   Both wasm crates are outside the workspace, so `cargo check -p wasm-qmd-parser` does not resolve. Checking them via `--manifest-path` would need their own lockfile, target dir and toolchain config, which is too heavy for verify. So the guard checks pampa itself with each set. Options:
   - **(i) Hardcode** the two `cargo check -p pampa --no-default-features [--features …]` lines in verify and CI, with a comment naming the consumer manifests. Simple, but it drifts when a consumer's features change.
   - **(ii) Derive** the sets in xtask by reading each `crates/wasm-*/Cargo.toml` `pampa` entry. Expose them as one xtask step (e.g. `cargo xtask check-feature-sets`) that verify calls and CI runs, so there is one source of truth and no drift. Recommended.

   **Measured cost** (warm `target/`, M-series Mac): with nothing changed, under a second per set. After an edit to pampa: ~0.5 s with no features and ~13 s with `lua-filter`. Cold CI cost is unmeasured, though Clippy already builds most of the check-mode dependency metadata.

   **Limit:** a native check catches feature-gating mistakes (the bd-76ogq4ew class). It does not catch errors inside `#[cfg(target_arch = "wasm32")]` code. The required parser tier (Q1) still covers those, so the two guards complement each other.
4. **The `terminal-support` side observation.** File it as a separate low-priority strand, or ignore it because no consumer uses the combination?

## Risks / tradeoffs (draft)

- **Fatal setup changes the failure surface.** A flaky wasm-pack download or a cc-rs hiccup on a runner now turns the step red, where it used to pass silently. That is the intended trade, but expect some infra-flavoured reds early on.
- **Other suites use the same skip-when-absent pattern.** `hub-client/src/pandoc/*.wasm.test.ts` skip when assets are missing. In CI those assets come from `fetch-pandoc-wasm.mjs --require`, so they fail loudly already. A wider audit is not needed for this strand. It is worth a sentence in the PR so the reasoning is on record.
- Rethrowing from a vitest `globalSetup` aborts the entire run, including the non-parser tests in the step. That is acceptable in the parser-tier step, which runs only parser-tier files.
