---
title: 'Stop build scripts from forcing a q2 rebuild on every cargo invocation'
date: 2026-10-20
description: 'Build scripts that watch missing paths make cargo rebuild quarto-preview and quarto on every build. This plan fixes them and has the doc-example staging build q2 once.'
---

**Strand:** bd-7xlph6ki. **GitHub:** [#870](https://github.com/quarto-dev/q2/issues/870).
**Status:** implemented 2026-10-20 on `braid/bd-7xlph6ki-build-scripts-watch-missing`; full `build-all --release` timing pending.

## Overview

`cargo xtask build-all --release` spends about 16 seconds per example project in
step 10 (staging the docs for `q2 docs llms`). There are 24 example projects, so
that is several minutes, and the list keeps growing. Each example's
`cargo run --bin q2 -- render …` recompiles `quarto-preview` and `quarto` before
it renders. A plain `cargo run --bin q2 -- --help` pays the same cost. Both
symptoms have the same root cause.

## Findings

### 1. Root cause: `rerun-if-changed` on a path that doesn't exist (confirmed)

Cargo treats a `cargo:rerun-if-changed` path that does not exist as
*always stale*: the build script reruns on every invocation, and every dependent
recompiles. `crates/quarto-preview/build.rs` emits these lines unconditionally:

```rust
println!("cargo:rerun-if-changed={}", editor_dist.display()); // hub-client/dist-preview-embed
```

`hub-client/dist-preview-embed/` is produced only by
`cargo xtask build-hub-client-embed`. Neither `build-all` nor `verify` runs that
step, so the directory is missing on almost every dev machine.

Evidence from `CARGO_LOG=cargo::core::compiler::fingerprint=info cargo build --bin q2`
on a fully built tree:

```
dirty: FsStatusOutdated(StaleItem(MissingFile { path: ".../crates/quarto-preview/../../hub-client/dist-preview-embed" }))
   Compiling quarto-preview v0.34.0
   Compiling quarto v0.34.0
    Finished `dev` profile [optimized + debuginfo] target(s) in 16.07s
```

Experiment: after a temporary `mkdir hub-client/dist-preview-embed`, the first
build reran once and the **second build finished in 0.51s**. `cargo run --bin q2
-- --help` then took 0.95s wall time. The directory was removed afterwards.

The other build scripts that use the same pattern are latent instances of this
bug:

| build script | watched path, possibly missing | always-stale when |
|---|---|---|
| `quarto-preview` | `hub-client/dist-preview-embed` | editor embed not built (**the usual case**) |
| `quarto-preview` | `q2-preview-spa/dist` | SPA not built (fresh clone) |
| `quarto` | `agents-docs-dist` | docs not staged (fresh clone; also the state #870's log was in) |
| `quarto-trace-server` | `trace-viewer/dist` | trace viewer not built |
| `quarto-mcp-launcher` | `ts-packages/quarto-hub-mcp/dist-bundle` | MCP bundle not built |
| `quarto-wasm-embed` | `q2-preview-spa/dist/assets`, `…/dist-bundle/wasm_quarto_hub_client_bg.wasm` | either not built |
| `quarto-trace` | `../../.git/HEAD`, `../../.git/index` | **in every `.worktrees/*` checkout** (see 3) |

### 2. Secondary cost: the preview archive is always rewritten

When `quarto-preview`\'s build script reruns, `archive_embed_dir` recompresses
both embed dirs at zstd level 19 and writes the archives with `std::fs::write`.
The write always gives the archives a new mtime, even when the bytes are
identical. So every rerun recompiles `quarto-preview` and relinks `q2`, even if
nothing changed. `quarto-wasm-embed` writes `wasm.zst` the same way. Fixing 1
removes the spurious reruns. A write-if-changed would also make a legitimate
rerun with identical output cost almost nothing downstream.

### 3. `quarto-trace`\'s git-hash watch (inferred, verify in Phase 1)

`crates/quarto-trace/build.rs` bakes `QUARTO_GIT_HASH` into `BUILD_GIT_HASH` and
watches `../../.git/HEAD` and `../../.git/index`. There are two problems:

- **In a worktree, `.git` is a file, not a directory.** Both watched paths are
  missing, so by finding 1, `quarto-trace` is always stale in `.worktrees/*`.
  `quarto-core` depends on `quarto-trace`, so this would recompile `quarto-core`
  and everything above it on *every* build in a worktree. That is far worse than
  the 16s. It follows from confirmed cargo behavior but has not yet been
  observed directly.
- **In the main checkout, the `.git/index` watch cascades.** Any `git add`,
  commit, or checkout rewrites the index, which reruns the build script.
  `quarto-trace` then recompiles, and so do `quarto-core`, `quarto-lsp`,
  `quarto-publish`, and the rest. The log in #870 shows exactly this chain on its
  first example (`quarto-trace` → … → `quarto-core` → `quarto-lsp-core` → …).
  The `-dirty` suffix is also only approximately right: editing a file does not
  touch the index, so a working-tree edit alone never refreshes it.

### 4. The xtask renders through `cargo run`, once per example, in the dev profile

`stage_doc_examples::render_project` and `build_agents_docs::render_docs` each
spawn `cargo run --bin q2 -- render …`. That means 25 nested cargo invocations
per `build-agents-docs`. Even with finding 1 fixed, each one costs a fingerprint
pass (about 0.5s). `cargo run` also always uses the **dev** profile. Under
`build-all --release`, step 9 builds release, step 10 then builds a whole
separate dev `q2` (a cold dev build if the machine has none), and step 10 ends
with another release build. The release pipeline
(`.github/workflows/release-pipeline.yml`) runs `cargo xtask build-agents-docs`
directly and pays the same dev-profile build.

## Proposed fix

### Phase 1: tests and verification first

- [x] Add a "no-op rebuild" check: run `cargo build --bin q2
  --message-format=json` twice and fail if the second run reports any
  `compiler-artifact` with `"fresh": false`. Put it in a small xtask helper and
  run it from `cargo xtask verify` after the workspace build. It is cheap on a
  warm tree and it is the only mechanical guard against this bug class. Wire it
  into `test-suite.yml` as well (decision 3). Confirm that it **fails**
  on the current tree.
- [x] Unit-test the shared watch helper from Phase 2: for a missing path it
  watches the nearest existing ancestor and never the missing path. For an
  existing dir it watches every entry recursively.
- [x] Reproduce finding 3 in a scratch worktree (`cargo xtask create-worktree`):
  two consecutive `cargo build --bin q2` runs, with the fingerprint log showing
  `MissingFile …/.git/HEAD`.

### Phase 2: never watch a missing path

- [x] Add one helper, `watch_path(path)`. If `path` exists, emit
  `rerun-if-changed` for it, plus every entry under it when it is a directory.
  If it does not exist, emit `rerun-if-changed` for the **nearest existing
  ancestor directory** instead. That directory's mtime changes when the missing
  child is created, so creating the dist still triggers a rerun. The cost is an
  occasional spurious rerun when a sibling is added to that ancestor, for
  example a new top-level entry in `hub-client/`. That is rare and bounded.
- [x] Use it in all six build scripts above, replacing the four copies of
  `watch_recursive`.
- [x] Use write-if-changed for `quarto-preview`\'s two `.tar.zst` archives and
  `quarto-wasm-embed`\'s `wasm.zst` (finding 2).

### Phase 3: make `quarto-trace`\'s git watch worktree-correct and cheaper

- [x] Resolve the watched files with `git rev-parse --git-path HEAD` and
  `--git-path index`, so they exist in worktrees too. If git is missing, emit no
  watch at all, which is today's "unknown" behavior without the permanent
  staleness.
- [x] Decide how much of the cascade to remove: path fix only; leaf-binary
  hash is bd-0pwv0mhd (decision 2).

### Phase 4: build q2 once in the xtask, then run the binary

- [x] Add a `util` helper that runs `cargo build --bin q2 [--release]
  --message-format=json-render-diagnostics` once and returns the `executable`
  path from the `compiler-artifact` message. This respects `CARGO_TARGET_DIR`
  and the profile without guessing paths.
- [x] `build_agents_docs::run` builds once and passes the binary path to
  `stage_doc_examples` and `render_docs`, which invoke it directly. The
  standalone `cargo xtask stage-doc-examples` builds its own.
- [x] Thread a `--release` flag through `build-agents-docs` and
  `stage-doc-examples`, and have `build-all --release` pass it. The renders then
  reuse step 9's release binary instead of building a dev one.

### Phase 5: end-to-end verification

- [x] `cargo build --bin q2` twice on this checkout (no
  `hub-client/dist-preview-embed`): the second run must not compile anything.
- [x] `cargo run --bin q2 -- --help` must finish in about 1s.
- [ ] `cargo xtask build-all --release`: step 10 must compile no Rust crates
  between example renders. Record the before and after timings of the step in
  this plan.
- [x] Repeat the no-op check inside a worktree (finding 3).
- [x] Building `hub-client/dist-preview-embed`, `agents-docs-dist`, and the
  other embeds after the fact must still trigger exactly one rerun that picks
  them up. Check this for at least the editor embed and `agents-docs-dist`.

## Decisions (2026-10-20)

1. **The shared helper lives in a new build-dependency crate,**
   `crates/quarto-build-support`. It replaces the four copies of
   `watch_recursive`.
2. **`quarto-trace`: fix only the path resolution now.** Moving the git hash
   into the leaf binaries is filed as bd-0pwv0mhd.
3. **CI runs the no-op rebuild check,** in `test-suite.yml`, and `cargo xtask
   verify` runs it too, added in the same commit. CI builds with
   `--cargo-profile ci`, so the check takes a profile argument.

## Implementation notes and end-to-end results (2026-10-20)

- `cargo xtask check-noop-rebuild [--profile P] [--rustflags F]`
  (`crates/xtask/src/check_noop_rebuild.rs`). Before the fix it failed on this
  tree, naming `quarto-preview` and `q2`. After the fix it passes, with the
  second build finishing in 0.47s. `verify` runs it after the claude-notes
  render. CI runs it in `test-suite.yml` (Linux, after the claude-notes
  render) as `--profile ci --rustflags "-D warnings"`. The flags are an
  argument rather than env so that the xtask built by "Run custom lints" is
  reused.
- `crates/quarto-build-support` provides `watch_path`, `watch_entry` (the
  non-recursive form, for `quarto-wasm-embed`\'s `assets/` directory) and
  `write_if_changed`. All six build scripts use it, and the four copies of
  `watch_recursive` are gone.
- `quarto-trace` resolves `HEAD` and `index` with `git rev-parse --git-path`.
  Worktree repro: before the fix, the second `cargo build -p quarto-trace`
  recompiled with `MissingFile …/.git/HEAD`. After the fix it is fresh, and
  the watch points at `.git/worktrees/<name>/HEAD`.
- `util::build_q2(root, release)` builds once and reads the executable path
  from cargo's JSON. `stage-doc-examples` and `build-agents-docs` take
  `--release`, and `build-all --release` passes it.

Observed:

```
$ time cargo run --bin q2 -- --help        # 1.07s total (was ~16s)
$ cargo xtask build-agents-docs            # 43.7s wall, 0 q2 compiles
                                           # (24 examples + 381-page docs render)
$ cargo xtask build-agents-docs --release  # 49.9s wall: 47s one-time release
                                           # rebuild for this change, then renders
$ cargo build --bin q2  (after staging)    # Compiling quarto, 2.3s (re-embed)
$ cargo build --bin q2                     # no-op, 0.53s
$ mkdir hub-client/dist-preview-embed; cargo build --bin q2
                                           # one rerun (quarto-preview + quarto)
$ cargo build --bin q2                     # no-op, 0.51s
$ rmdir …; cargo build --bin q2            # one rerun; next build is a no-op
$ q2 docs llms --embed-info                # source: real, pages: 381
```

I inspected the staged output in `docs/examples/*` and the `--embed-info`
report by hand.
