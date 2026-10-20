---
title: 'Release asset: hub-client WASM package for Connect satellite builds (bd-m7oy2asc)'
date: 2026-10-20
description: 'connect-satellites'' build-editor.sh needs the gitignored crates/wasm-quarto-hub-client/pkg from a q2 checkout at its pinned tag; package it once in the release pipeline''s web-payloads job and publish quarto-hub-client-wasm-<version>.tar.gz (checksummed + minisigned) as a release asset.'
status: phases 1-2 landed; dry-run dispatch + real-release confirmation pending
braid:
  strand: bd-m7oy2asc
---

## Overview

`external-sources/connect-satellites/quartohub/build-editor.sh` builds the
editor shell that the Connect satellite serves at `<mount>/editor`. It works
from a q2 checkout at the pinned release tag (`Q2_VERSION` in
`quartohub/gateway.py`, currently `0.34.0`), and it needs exactly one thing a
fresh clone cannot provide: the compiled wasm-bindgen package at
`crates/wasm-quarto-hub-client/pkg/`. That directory is gitignored and building
it (`npm run build:wasm` in `hub-client/`, which drives
`hub-client/scripts/build-wasm.js`) requires nightly Rust with `rust-src`, the
pinned `wasm-bindgen-cli`, binaryen (`wasm-opt`), and LLVM — today the script
copies the pkg from a developer machine via `Q2_PREBUILT_TREE`.

This plan adds a new, separate release asset to the release pipeline so the
satellite can download the exact pkg bytes for its pinned tag:

```text
quarto-hub-client-wasm-<version>.tar.gz         (+ .sha256, + .minisig)
```

The release pipeline already builds these bytes once per release: the
`web-payloads` job runs `npm run build:wasm` and uploads
`crates/wasm-quarto-hub-client/pkg` as part of its intermediate artifact. We
package that directory into a release-grade tarball in the same job, then
checksummed, minisigned, and publish it alongside the existing assets. The
changes are confined to `.github/workflows/release-pipeline.yml` plus
documentation; no Rust or TypeScript code changes.

## What build-editor.sh consumes (the contract we are serving)

From reading the script and the editor-shell vite config
(`external-sources/connect-satellites/quartohub/editor-shell/vite.config.ts`):

1. **A q2 checkout at tag `v$Q2_VERSION`** — the script clones this itself
   (`git clone --depth 1 --branch`), so no artifact is needed for it. It uses:
   - `hub-client/src/**` — the unmodified Editor component tree, imported as
     TypeScript source via the `source` resolve condition;
   - `ts-packages/**` sources (notably `wasm-js-bridge`, `preview-renderer`,
     `preview-runtime`), same mechanism;
   - the root npm workspace (its `package.json` globs include `q2-demos/*`,
     which is where the script stages the shell as `connect-editor`) plus
     `package-lock.json` for one hoisted `npm install` with exactly one React;
   - `resources/attribution/viewer.css`, read by a vite plugin;
   - the committed symlink `hub-client/wasm-quarto-hub-client` →
     `../crates/wasm-quarto-hub-client/pkg`.
2. **The built wasm package** at `crates/wasm-quarto-hub-client/pkg/` — the
   only gitignored input. Four files (sizes from a local build):

   | file | size | role |
   |---|---|---|
   | `wasm_quarto_hub_client.js` | ~92 KB | wasm-bindgen `--target web` glue |
   | `wasm_quarto_hub_client_bg.wasm` | ~36 MB | the render engine, `wasm-opt -Oz` |
   | `wasm_quarto_hub_client.d.ts` | ~50 KB | types |
   | `wasm_quarto_hub_client_bg.wasm.d.ts` | ~11 KB | types |

   The script checks `wasm_quarto_hub_client.js` exists and that the tree it
   came from sits at the pinned tag. The shell build resolves the package
   through the vite alias `wasm-quarto-hub-client` →
   `hub-client/wasm-quarto-hub-client/wasm_quarto_hub_client.js` (the symlink),
   so extraction must reproduce the repo-relative path
   `crates/wasm-quarto-hub-client/pkg/`.

The editor and hub share an Automerge schema and sync protocol and are pinned
to the same q2 release, so the pkg must come from the release commit itself —
which is exactly what the `web-payloads` job builds.

## Design decisions

**Artifact name.** `quarto-hub-client-wasm-<version>.tar.gz`, following the
name-first convention of `quarto-hub-mcp-<version>.tar.gz`. Deliberately *not*
the `q2-<version>-<platform>` shape: that namespace is the install.sh binary
contract (encoded in `crates/quarto/tests/integration/bootstrap_sh.rs`), and
keeping it meaning "installable q2 binaries" avoids surprising anyone
glob-downloading `q2-*` assets. The sign/publish globs in the `release` job
are extended explicitly, as they already were once for `quarto-hub-mcp-*`.

**Tarball layout.** Repo-root-relative paths
(`crates/wasm-quarto-hub-client/pkg/...`), so the consumption contract is one
command:

```bash
tar -xzf quarto-hub-client-wasm-<version>.tar.gz -C /path/to/q2-checkout
```

Tar the real directory, never the `hub-client/wasm-quarto-hub-client` symlink
(the same rule the `web-payloads` artifact staging already documents). The
symlink itself is committed, so clones already have it. Estimated asset size
~12–15 MB after gzip; confirm in the dry run.

**Build once, in `web-payloads`.** The wasm is target-independent (identical
bytes for every release target) and the job already has the heavy toolchain
(nightly + rust-src + wasm-bindgen + binaryen). Packaging there mirrors how
the `hub-mcp-bundle` job consumes the same pkg. Signing stays centralized in
the `release` job — the one-job-touches-the-key property is explicitly worth
keeping (comment at the sign step).

**Checksums + signature.** Same treatment as every other asset: a `.sha256`
sidecar folded into `checksums.sha256` by the existing combine step, and a
`.minisig` whose trusted comment is the asset filename. install.sh is
unaffected: it selects its platform archive by exact name and verifies
checksums with `--ignore-missing`.

**Nightly channel.** Gets the asset too (filename carries the nightly
version), for free, since it shares the pipeline. Satellites pin to stable
versions via `Q2_VERSION`, so this is a convenience, not a requirement.

**No `cargo xtask verify` counterpart.** The release pipeline is deliberately
outside the verify mirror (it publishes rather than gates). No new secrets or
permissions.

**Out of scope (follow-up in connect-satellites, not this repo).** Rewiring
`build-editor.sh` to download and verify the asset for `v$Q2_VERSION` instead
of requiring `Q2_PREBUILT_TREE` (keeping the latter as a dev override), and
bumping the pinned `Q2_VERSION` to the first release that carries the asset.
That repo's `0.34.0` pin predates this work.

### Alternatives considered

- **npm publish (`@quarto/wasm-quarto-hub-client`).** The clean long-term
  channel, but the satellite builds from a git checkout and needs no npm
  packaging; a release asset matches the existing MCP-bundle stopgap pattern
  and adds no publishing infrastructure. Revisit alongside bd-3tak0lyy.
- **Ship the whole hub-client `dist/` or a pre-built editor.** The satellite
  builds its own shell (it substitutes its `CustomHeader` for the upstream
  top bar at vite time), and the shell sources live in connect-satellites.
  Nothing pre-built on our side can serve that.
- **Package in the `release` job from the `web-payloads` intermediate
  artifact.** Avoids touching `web-payloads`, but downloads hundreds of MB of
  SPA/docs payloads to tar up one directory. Packaging where the bytes are
  already local is cheaper and keeps the verify-at-source gate natural.

## Checklist

### Phase 1 — workflow changes (`.github/workflows/release-pipeline.yml`)

- [x] `web-payloads` job, after the `Build WASM (hub-client)` step: add a
  packaging step that (a) asserts the four pkg files exist and are non-empty
  (fail closed), (b) optionally syntax-checks the JS glue
  (`node --check --input-type=module < .../wasm_quarto_hub_client.js`),
  (c) creates `quarto-hub-client-wasm-${{ inputs.version }}.tar.gz` with
  repo-relative paths, (d) writes + verifies its `.sha256`, (e) prints
  `tar -tzf` for the log.
- [x] `web-payloads` job: upload the tarball + `.sha256` as a new artifact
  named `hub-client-wasm-pkg` (`if-no-files-found: error`, 7-day retention).
- [x] `release` job: download the `hub-client-wasm-pkg` artifact into
  `artifacts/` (its own download step, like `hub-mcp-bundle`).
- [x] `release` job: fail-closed content check before signing — `tar -tzf`
  must list all four `crates/wasm-quarto-hub-client/pkg/` files.
- [x] `release` job: extend the minisign loop glob with
  `artifacts/quarto-hub-client-wasm-*.tar.gz` (an unmatched glob stays literal
  and fails minisign, which is the fail-closed behavior we want).
- [x] `release` job: extend the `gh release create` asset list with the
  tarball, its `.sha256`, and its `.minisig`.
- [x] `release` job: add a short "Hub client WASM package" section to the
  shared part of the notes template: what it is, the one-line extraction
  contract, and that most users do not need it. (Mention in both channels;
  the nightly filename carries the nightly version.)

### Phase 2 — documentation

- [x] Update the asset list in `claude-notes/instructions/release-runbook.md`
  ("What a release produces") with the new asset and its consumer.
- [x] Update the header comment of `release-pipeline.yml` (the artifact
  inventory) in the same commit.

### Phase 3 — verification

- [x] Local mechanics simulation: ran the packaging step's exact commands
  against the locally built pkg — all four files present/non-empty, the
  `node --check --input-type=module` gate passes, the tarball has
  repo-relative paths and is 9.3 MB. Extracted it into a fresh
  `git clone --depth 1 --branch v0.34.0`; the four files land at
  `crates/wasm-quarto-hub-client/pkg/` and the committed
  `hub-client/wasm-quarto-hub-client` symlink resolves to the extracted
  glue. The release job's `grep -qx` content check passes on the tarball.
  `actionlint` is clean on the edited workflow; Python YAML parse OK.
- [x] End-to-end with the satellite — see "Verification record" below.
- [ ] Dry-run release on the branch: `workflow_dispatch` with `publish:
  false` (the whole pipeline runs; inspect the `release-set` artifact for
  tarball + `.sha256` + `.minisig`, and `notes.md` for the new section).
  Needs the branch pushed and the dispatch authorized.
- [ ] After the next real release: confirm the asset is attached to the
  GitHub Release and installs the extraction contract end-to-end from the
  published URL.

## Verification record

End-to-end proof that a checkout plus the new asset is sufficient for the
satellite build, run 2026-10-20 in a scratch directory
(`$PA_SCRATCH_DIR/wasm-asset-test.*`):

1. Packaged the locally built pkg with the workflow step's exact commands
   (`tar -czf quarto-hub-client-wasm-0.0.0-test.tar.gz
   crates/wasm-quarto-hub-client/pkg` from the repo root, plus the
   file-presence and node syntax gates). Drift check first: `git diff
   v0.34.0..HEAD --stat` over the wasm crate, `hub-client/src`, and
   `ts-packages` touches only four hub-mcp test files, and both pins carry
   wasm-bindgen 0.2.108 — so the local pkg is equivalent to a v0.34.0
   build for this purpose.
2. Copied `external-sources/connect-satellites/quartohub/` to scratch
   (never built inside `external-sources/`), made a fresh
   `git clone --depth 1 --branch v0.34.0` at its `q2/`, and extracted the
   tarball into it — the exact consumption contract the satellite will use.
3. Ran `Q2_PREBUILT_TREE="$PWD/q2" ./build-editor.sh`. One deviation: the
   script itself died on macOS at its informational
   `find ... -printf` line (GNU find only; see the follow-up below), so the
   remaining steps were run verbatim by hand: stage `editor-shell` →
   `q2-demos/connect-editor`, `npm install` at the root, the one-React
   check, `npm run typecheck && Q2_VERSION=0.34.0 npm run build`, the two
   deploy-wiring greps, and the final copy to `quartohub/editor/`.
4. Observed output (inspected): typecheck clean; `vite build` succeeded in
   19 s; both wiring greps passed; `quartohub/editor/` contains
   `index.html`, `q2-preview.html`, and `assets/` with
   `wasm_quarto_hub_client_bg-*.wasm` (34 MB) and the bundled glue
   `wasm_quarto_hub_client-*.js` (33 kB) — the extracted asset drove the
   entire build.

### Follow-ups (file as strands when this lands)

- [ ] connect-satellites: teach `build-editor.sh` to download + verify the
  asset for `v$Q2_VERSION` (`.sha256` mandatory; consider minisign against
  the pinned pubkey), keeping `Q2_PREBUILT_TREE` as a dev override; bump the
  pinned `Q2_VERSION` to the first release carrying the asset. Separate repo —
  implement there, link `discovered-from: bd-m7oy2asc`.
- [ ] connect-satellites: `build-editor.sh` line ~103 uses GNU-only
  `find -printf` for its informational pkg listing and dies on macOS (BSD
  find) before any build step runs. Harmless on the Linux targets the
  satellite deploys to, but it blocks local macOS runs; replace with a
  POSIX form (`find ... -exec ls -l` or `stat`). Discovered during the
  end-to-end verification above.

## Notes

- The `web-payloads` intermediate artifact already includes
  `crates/wasm-quarto-hub-client/pkg` for the per-target build legs and the
  MCP bundle job; this plan does not change that contract. GHA artifacts
  expire and need API auth, which is why the satellite needs a release asset
  instead of the intermediate artifact.
- Nothing here makes the editor's wasm byte-identical to the wasm embedded in
  the `q2` binary by `quarto-wasm-embed` — but both come from the same
  `web-payloads` build of the same commit, so they are identical in practice.
  Byte-identity is not a requirement (editor and hub talk Automerge, not
  wasm), so no new parity gate is added.
