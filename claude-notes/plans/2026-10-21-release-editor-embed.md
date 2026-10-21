# Release binaries: ship the real `--ui editor` embed (bd-yx63o8wo)

## Overview

GitHub issue [#869](https://github.com/quarto-dev/q2/issues/869): nightly
(and release) binaries show the placeholder page for `q2 preview --ui
editor`. Root cause: the `web-payloads` job in `release-pipeline.yml`
never builds or stages `hub-client/dist-preview-embed`, so
`crates/quarto-preview/build.rs` embeds the placeholder — silently, which
is the designed behavior for local dev builds — and the per-target
verify gate asserts only the *viewer* asset manifest, so nothing fails.

Investigation concluded this was not a conscious decision: the editor
embed landed 2026-08-06 (bd-jt1etjbn) and predates the nightly/reusable
pipeline (2026-09-19, bd-p4ljdp2e); the pipeline payload list was never
updated when the fourth embed appeared.

## Checklist

- [x] Investigate root cause (GH #869)
- [x] Create strand bd-yx63o8wo, branch, this plan
- [x] Build the editor embed in `web-payloads` and stage
      `hub-client/dist-preview-embed`
- [x] Assert a real `editor:` manifest hash in the per-target verify gate
- [x] Update pipeline header + job comments (FOUR payloads)
- [x] Update `claude-notes/instructions/release-runbook.md`
- [x] Verify: YAML parses (actionlint clean), gate grep logic,
      `q2 render --strict claude-notes` (1532/1532)
- [ ] Dry-run the pipeline from this branch (needs push permission)

## Details

### Why web-payloads

The embed is a hub-client Vite build — target-independent, like the
q2-preview-spa and trace-viewer dists already staged there. Building it
per-leg would need the wasm32 toolchain on every runner for no benefit.

The step runs `npm run build:preview-embed` (working-directory
`hub-client`) — the same invocation `cargo xtask
build-hub-client-embed` wraps, matching how the job already mirrors
`build-q2-preview-spa` / `build-trace-viewer` as bare npm calls (no
xtask compile on that runner). The script re-runs `build:wasm`, which
is a cargo no-op on the warm target dir from the earlier step, plus a
bindgen/wasm-opt re-pass — cheap enough to not warrant a divergent
skip flag.

Placement: after "Build q2-preview-spa". Order among the dists does
not matter for dedupe — `quarto-preview/build.rs` dedupes editor files
against the viewer dist at *Rust build* time on the build legs, not
during these vite builds.

`if-no-files-found: error` on the artifact upload makes a silently
missing dist a hard failure.

### Why the gate holds cross-platform

The editor manifest is generated per-platform by `build.rs` over the
post-dedupe file set. `spa-manifest` generation is deterministic by
design (byte-order path sort, content hashes), so identical staged
dists yield identical hashes on every platform — the existing
`asset-manifest-check` job (exactly 2 unique lines) still holds with a
real editor embed: one viewer line, one editor line. Until now it
trivially passed with `editor: none` everywhere; the new assertion in
the hash-recording step fails a leg whose editor line is not a real
64-hex hash.

### Testing a workflow change

No local harness exists for the pipeline itself. The grep gate logic is
checked mechanically against simulated `none`/real-hash output; the
npm invocation is already exercised in CI by `q2-preview-spa-e2e.yml`
(via the xtask). The full end-to-end proof is the pipeline dry-run the
runbook documents (`gh workflow run release.yml --ref <branch> -f
tag=vX.Y.Z -f publish=false`) — left as the post-push step.
