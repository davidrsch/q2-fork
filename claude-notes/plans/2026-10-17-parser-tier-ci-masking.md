---
title: 'Parser-tier CI step masks wasm-qmd-parser build failures (bd-yt3x1gwk)'
date: 2026-10-17
description: 'The quarto-hub-mcp parser-tier CI step reports green when wasm-qmd-parser fails to compile, because the vitest global setup turns every build failure into a skip; make the build fatal on legs that declare the toolchain present, and add a cheap derived feature-set check of pampa to CI and verify.'
status: implemented  # Phases 0–5 landed 2026-10-17; PR CI confirmation pending
braid:
  strand: bd-yt3x1gwk
---

**Branch:** `braid/bd-yt3x1gwk-pampa-fails-compile-without` (topic branch in the main checkout, based on `main` @ `ad283a2cb`)

## Implementation status (2026-10-17)

- [x] Phase 0: xtask unit tests (`check_feature_sets::tests`, plus verify gate and wiring tests); vitest `src/vitest-global-setup.test.ts`.
- [x] Phase 1: `prepareQmdParser` in `vitest-global-setup.mjs`.
- [x] Phase 2: `QUARTO_QMD_PARSER_REQUIRED: "1"` on the parser-tier step.
- [x] Phase 3: `cargo xtask check-feature-sets [--profile P]`.
- [x] Phase 4: verify step 3 sub-step (`--skip-feature-sets`), plus the CI step *Check pampa consumer feature sets* (`--profile ci`, `-D warnings`) after Clippy.
- [x] Phase 5: `crates/wasm-qmd-parser/AGENTS.md`, pampa `[features]` comment, `.claude/rules/xtask.md`.
- [ ] Phase 6: local negative tests done; full verify, and PR CI confirming the parser tier runs (not skips) on both legs, still to come.
  - An ungated `&mlua::Error` probe in pampa makes `check-feature-sets` fail on the `{}` set and name the consumers.
  - `QUARTO_QMD_PARSER_REQUIRED=1 npx vitest run src/qmd-ast.test.ts` on a machine without wasm-pack exits 1 with the build error, and writes no marker. The compile-error path itself needs a wasm-pack machine, so CI covers it.

## Goal

A PR whose CI checks pass, and whose local `cargo xtask verify` passes, must not then fail the nightly or release build. v0.34.0 broke exactly this way. CI compiled pampa without `lua-filter`, the compile failed, and the step still reported success.

## Decisions (2026-10-17)

1. **Make parser setup fatal through an opt-in variable (option A).** A CI step that has the wasm toolchain sets `QUARTO_QMD_PARSER_REQUIRED=1`. With the variable set, a failed parser build fails the run. Without it, the setup keeps today's skip. The setup does not try to classify errors as "toolchain missing" versus "compile error". This mirrors the existing `TYPST_PARITY_REQUIRED`.
2. **`cargo xtask verify` does not set `QUARTO_QMD_PARSER_REQUIRED`.** Verify keeps working for developers without LLVM/wasm-pack. The feature-set check (decision 3) gives verify its coverage of this class of break.
3. **Add a pampa feature-set check to both CI and verify.** Verify exists so developers can see CI's result ahead of time, so the check runs in both places. It is cheap (measurements below).
4. **Derive the feature sets from consumer manifests.** Don't hardcode them. xtask reads every manifest that depends on pampa with `default-features = false`, so the list follows the consumers\' `Cargo.toml` and cannot go stale.
   *Implementation note:* xtask reads the manifests with `cargo metadata --no-deps` rather than parsing TOML (no `toml` dependency). Cargo then applies workspace inheritance itself, which matters because the root `[workspace.dependencies.pampa]` sets `default-features = false`. It also matches by package name, so renamed dependencies are covered. It works on the excluded wasm crates too, at ~0.02 s per manifest. Discovery also finds quarto-core\'s `{json-filter, lua-filter}` set, so the check runs three sets: `{}`, `{lua-filter}`, `{json-filter, lua-filter}`.
5. **Leave `--features terminal-support` alone.** `cargo check -p pampa --no-default-features --features terminal-support` fails (`unresolved import quarto_system_runtime`). No consumer uses that combination. Wait until a real consumer needs it, then design the fix with that use case in hand. No strand will be filed. Decision 4 checks only the sets that consumers actually use, so this combination will not trip the new check.

## Issue context

Filed 2026-10-17 by Carlos (P1 bug, labels `ci`, `release`). The strand records two defects:

1. **pampa does not compile without `lua-filter`.** Commit 73588f3e5 (bd-76ogq4ew) left `ModuleCompileError::find_in` in an ungated `impl`. **Done:** #862 (c73213d7a) fixed it, and v0.34.0 was re-tagged at 8c1eed493 and published. `cargo check -p pampa --no-default-features` passes at `ad283a2cb`.
2. **CI masked the break. This plan covers it.** The PR #861 ubuntu log shows the masking:
   ```
   [qmd-parser] unavailable (wasm-pack build failed for wasm-qmd-parser ...) — parser-tier tests will skip.
   ↓ src/qmd-ast.test.ts (21 tests | 21 skipped)
   ```

## Dependency graph

- **discovered-from**: bd-76ogq4ew (in_progress). Its phase 1 introduced `find_in`. The release run surfaced the break, not the bd-76ogq4ew work itself, and that strand puts no design constraints on this one.
- No `blocks`/`related` edges. The release is out, so nothing is waiting on this strand. It stays P1 because the same masking would hide the next feature-gating mistake until a release.

## What the code looks like today

- `ts-packages/quarto-hub-mcp/scripts/vitest-global-setup.mjs`: `setup()` wraps `ensureQmdParserPkg({ quiet: true })` in a bare `try/catch`. On any throw it writes `.qmd-parser-unavailable` and prints a warning.
- `ts-packages/quarto-hub-mcp/scripts/qmd-parser-pkg.mjs`: `ensureQmdParserPkg` throws when LLVM is missing (darwin), when `wasm-pack` is missing (spawn error), and when the build fails (non-zero exit). The non-zero exit also covers a missing wasm32 clang on Linux.
- `ts-packages/quarto-hub-mcp/src/test-setup.ts`: exports `PARSER_UNAVAILABLE`, which is true when the marker file exists. Seven test files gate on it: `qmd-ast`, `qmd-outline`, `qmd-sections`, `conformance` (per case), `bundle`, `npm-pack`, `mcpb-pack`.
- `.github/workflows/ts-test-suite.yml`:
  - The `test-suite` job (ubuntu + macOS) has the toolchain. Its step *Run quarto-hub-mcp parser-tier tests* (~line 254) is where the skip is wrong.
  - The `workspace-ts-suites` job is toolchain-free by design and runs `npm test -w ts-packages/quarto-hub-mcp` (~line 405). The skip is right there.
- **Precedent:** `TYPST_PARITY_REQUIRED=1` in the same workflow, read by `ts-packages/typst-host/src/parity.test.ts:26,40`.
- **pampa consumers that turn off default features:**

  | Consumer | In workspace? | pampa features |
  | --- | --- | --- |
  | `crates/wasm-qmd-parser` | no (excluded) | none |
  | `crates/wasm-quarto-hub-client` | no (excluded) | `lua-filter` |
  | `crates/experiments/reconcile-viewer` | yes (`workspace = true`) | none |

  The two wasm crates are excluded from the workspace, so `cargo check -p <consumer>` does not resolve. The check therefore runs on pampa itself, once per distinct feature set.
- `crates/xtask` has no TOML parser. The workspace pins `toml = "0.9.11"`, and xtask already uses `walkdir`.
- The Rust `test-suite.yml` has no `--no-default-features` check. Its WASM job builds pampa with `--features lua-filter`, which is why it missed the break.

**Measured cost of the check** (warm `target/`, M-series Mac): with nothing changed, under a second per set. After an edit to pampa: ~0.5 s with no features, ~13 s with `lua-filter`. The cold CI cost is unmeasured. Clippy runs earlier in the same job and already builds most of the check-mode dependency metadata.

**Limit:** a native check catches feature-gating mistakes. It does not catch errors in `#[cfg(target_arch = "wasm32")]` code. The required parser tier covers that. The two guards complement each other; neither replaces the other.

## Phases

### Phase 0 — Tests first

- xtask unit tests for feature-set derivation:
  - fixture manifests covering `default-features = false` with and without `features`;
  - `workspace = true` inheritance (reconcile-viewer's form);
  - a pampa dependency with defaults on (ignored);
  - duplicate sets (deduplicated);
  - the `[dependencies]` vs. `[target.'cfg(...)'.dependencies]` placement of the dependency.
- A test that runs the derivation on the real repo and asserts it finds at least the `{}` and `{lua-filter}` sets. This catches a silent "found nothing" regression.
- A vitest test for the setup decision. Extract a pure helper from `vitest-global-setup.mjs` (e.g. `onParserBuildFailure(err, { required })`) and assert that it rethrows when required and writes the marker otherwise.

### Phase 1 — Fatal parser setup

- In `vitest-global-setup.mjs`, read `process.env.QUARTO_QMD_PARSER_REQUIRED === '1'`. When it is set and the build fails:
  - delete any stale marker;
  - rethrow with a message naming the variable, like typst's `TYPST_PARITY_REQUIRED=1 but …`.

  Unset keeps the current behaviour.
- Update the doc comments in `vitest-global-setup.mjs` and `src/test-setup.ts`. They currently describe the skip as unconditional.

### Phase 2 — CI wiring for the parser tier

- Add `env: QUARTO_QMD_PARSER_REQUIRED: '1'` to the *Run quarto-hub-mcp parser-tier tests* step in `ts-test-suite.yml`. That step covers both matrix legs.
- Extend the step's comment block with the reason (this incident, bd-yt3x1gwk).
- Leave `workspace-ts-suites` untouched.

### Phase 3 — Derived pampa feature-set check

- New xtask module (working name `check_feature_sets.rs`) and subcommand (`cargo xtask check-feature-sets`):
  1. Walk the repo's crate manifests (`crates/**/Cargo.toml`, skipping `target/`, `vendor/` and `node_modules/`).
  2. Parse each manifest with `toml`. Collect every `pampa` dependency entry that has `default-features = false`.
  3. Collect the distinct `features` lists from those entries.
  4. For each list, run `cargo check -p pampa --no-default-features [--features a,b]`. Print the consumers that require each set, so a failure tells the developer which consumer it protects.
- Add `toml = { workspace = true }` to `crates/xtask/Cargo.toml`.
- Scope is pampa only for now. Generalising to "any workspace crate with an out-of-workspace `default-features = false` consumer" is a natural follow-up, but it is not needed to close this strand.

### Phase 4 — Hook the check into verify and CI

- `cargo xtask verify`: add the check as a step right after the Rust build step, since it is cheap and fails fast. Give it a `--skip-feature-sets` flag consistent with the other skip flags, and update `TOTAL_STEPS` and the module doc list.
- `test-suite.yml`: add a *Check pampa feature sets* step running `cargo xtask check-feature-sets` next to Clippy, on both matrix legs.

### Phase 5 — Docs

- `crates/wasm-qmd-parser/AGENTS.md` (toolchain section): document `QUARTO_QMD_PARSER_REQUIRED`.
- Document the feature-set check, in pampa's `Cargo.toml` `[features]` comment or a `dev-docs/` note: "consumers\' feature sets are checked by `cargo xtask check-feature-sets`; add a consumer and it is picked up automatically".

### Phase 6 — Verification

- `cargo xtask verify --skip-hub-build` is green.
- **Negative test (local, not committed):** temporarily reintroduce an ungated `mlua` reference in pampa and confirm that both of these fail:
  - `cargo xtask check-feature-sets`;
  - the parser-tier vitest command run with `QUARTO_QMD_PARSER_REQUIRED=1`.

  Then revert.
- PR CI: confirm the parser-tier step still runs its tests (not skipped) on ubuntu and macOS, and that the new Rust step is present.

## Risks / tradeoffs

- **Fatal setup changes the failure surface.** A flaky wasm-pack install or cc-rs hiccup on a runner now turns the step red, where it used to pass silently. That is intended, but expect some infra-flavoured reds early on.
- **Rethrowing from a vitest `globalSetup` aborts the whole run.** That is acceptable here: the parser-tier step runs only parser-tier files.
- **Manifest discovery is lexical.** It matches the `pampa` dependency key. A renamed dependency (`package = "pampa"` under another key) would be missed. Match on `package` too.
- **Other suites skip when assets are absent** (`hub-client/src/pandoc/*.wasm.test.ts`). In CI those assets come from `fetch-pandoc-wasm.mjs --require`, which fails loudly. A sentence in the PR is enough; no audit is needed for this strand.
