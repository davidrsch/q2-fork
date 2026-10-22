---
title: 'placeholder shortcode: rasterize locally instead of fetching svg2png.deno.dev (bd-gebcr1sa)'
date: 2026-10-21
description: 'Port quarto-cli bc2d42e00 so that the placeholder shortcode rasterizes its SVG locally instead of calling the retired svg2png.deno.dev service, adding the missing Lua API pieces and deciding what happens in WASM.'
status: done
braid:
  strand: bd-gebcr1sa
---

**Branch:** `braid/bd-gebcr1sa-placeholder-local-rasterize`, a standalone branch off `main` (after #882 merged), for its own PR. Investigated on `main` @ `5dcc4da54`.

**Pre-flight verify (2026-10-21):** steps 1–4 green (lints, fmt, build, claude-notes strict render). Step 5 failed only on 4 pampa pandoc-oracle tests (`test_json_writer`, `test_html_writer`, `unit_test_corpus_matches_pandoc_{markdown,commonmark}`) because the local pandoc is 3.12.1 and the calibrated range is 3.6–3.11. That is environmental and unrelated to this strand. Nextest cancelled after them, so steps 6–14 did not run.

## Triage verdict

**Ready to design; design settled 2026-10-21 (see Decisions).** The upstream fix is small and fully readable. The port is
gated on four small additions to the pampa Lua API (not two, as the strand
says), plus a decision about what WASM (the hub) does. These are design
questions, not unknowns.

## Issue context

Strand bd-gebcr1sa (task, P3, open, filed 2026-10-21 by Carlos Scheidegger):
`resources/extensions/quarto/placeholder/placeholder.lua` converts its SVG to
PNG for non-SVG outputs by fetching `https://svg2png.deno.dev/<svg>`. Upstream
quarto-cli 1.11.6 (bc2d42e00, quarto-dev/quarto-cli#14724, fixes #14722)
replaced this with local rasterization: it writes `in.svg` and a one-line
`in.typ` wrapper to a temp dir, runs
`typst compile --root <tmp> --format png --ppi 96 --ignore-system-fonts`, and
returns a base64 PNG data URI. If that fails, it calls `fatal()` instead of
silently putting text in the image's place.

**Confirmed at HEAD:** `curl https://svg2png.deno.dev/` returns **404**; the
service is permanently retired (upstream commit message). The q2 `format=png`
path is therefore always broken today, not just flaky.

## Dependency graph

- **discovered-from**: bd-jsfg1494 (closed): "Vendored: quarto-cli copies
  v1.11.3 → v1.11.6". This strand came up in the vendored resync
  (b7126df57, on `feature/bd-xm7l-vendored-updates` / PR #882, **not yet on
  `main`**). That resync didn't touch `placeholder.lua`; the port was split
  out because it needs new Lua APIs. Sibling follow-ups from the same close:
  bd-rbd3jq4n, bd-ix7psky0.
- No `blocks` edges and no children, so nothing else is waiting on this.

## What the code looks like today

### The "q2 local patch" is already less exposed than the strand implies

`diff` of Q1 HEAD vs q2's `placeholder.lua`:

- **Q1** default format: `svg` for Typst output, **`png` for everything else**
  (via `quarto.format.is_typst_output()`).
- **q2** default format: **`svg` for every format** (q2 has no
  `quarto.format` namespace, so the branch was collapsed).

So q2 reaches the network only when the author writes `format=png`
explicitly. A plain `{{{< placeholder 200 >}}}` already works offline for every
format (that's what `smoke-all/extensions/builtin-placeholder-shortcode` and
`smoke-all/typst/fraction-layout.qmd` test). Whether SVG is an acceptable
default for LaTeX/PDF and docx output in q2 is an open question (see Q2 below).

### Lua API gaps (pampa shortcode Lua state, `crates/pampa/src/lua/`)

The upstream Lua uses five things. Here's what q2 has for each:

| Upstream call | q2 status |
|---|---|
| `quarto.paths.typst()` | **Missing.** `quarto_api.rs` has no `paths`. The Typst binary is found in `quarto-core` (`BinaryDependencies::discover` → `runtime.find_binary("typst", "QUARTO_TYPST")`, `render.rs:155`) and is only passed to the *pandoc-hybrid* filters through `quarto-environment.paths.Typst` (`pandoc_filters/params.rs:409`). The shortcode Lua state never sees it. |
| `fatal(msg)` | **Missing** as a global in the pampa Lua state. It exists only in the vendored pandoc filters (`resources/pandoc-filters/filters/common/log.lua:31`), which run inside real pandoc. |
| `pandoc.pipe(cmd, args, input)` | **Missing.** pampa only has `pandoc.system.command` (pandoc 3.7 API, `system.rs:77`), which returns the exit status instead of raising. |
| `pandoc.system.with_temporary_directory('placeholder', fn)` | **Arity bug.** q2 registers only the 3-arg form `(parent_dir, template, callback)` (`system.rs:332`). The upstream 2-arg call would bind `template = fn` and fail with a type error. Pandoc accepts both arities. (`parent_dir` is also silently ignored; there's a TODO.) |
| `io.open` (`wb`/`rb`) | Fine on native (`Lua::new()` full stdlib). WASM uses `io_wasm.rs` (VFS-backed). |

### WASM

`WasmRuntime::exec_command` returns `NotSupported` (`quarto-system-runtime/src/wasm.rs:583`),
and there's no Typst binary in the hub, so the upstream approach cannot work in
WASM as written. Because q2 defaults to `svg` everywhere, the only WASM case
affected is an explicit `format=png`.

### A Rust-native alternative exists in the dependency graph

`usvg` 0.45, `fontdb`, and `typst-assets` (with Typst's embedded fonts) are
already in `Cargo.lock`. They come in through `typst-gather → typst-kit →
typst-library` (native-only, quarto-core). `resvg` / `tiny-skia` are not in the
lockfile yet. A `resvg`-based rasterizer is pure Rust and compiles to wasm32,
so one Lua function (e.g. `quarto.svg.to_png(svg)`) could serve both native
and the hub. The costs are a new dependency, WASM bundle size, a font to embed
for the label text, and a q2-specific divergence in the vendored Lua file.

### Repro

`placeholder-local-rasterize-investigation/repro.qmd.txt` (renamed so the claude-notes render skips it): `{{{< placeholder 200 format=png >}}}`
for HTML. At HEAD the service returns 404; q2's `pandoc.mediabag.fetch` logs the failure and returns `nil` instead of raising, so the Lua crashes on `"Expected image/png but got " .. mt` ("attempt to concatenate a nil value (local 'mt')"). The dead `return pandoc.Str(...)` lines after `error()` are Q1-isms (Q1 redefines `error` to log only). The captured output is in `repro-output-at-5dcc4da54.txt` (this plan's own text triggered it before the shortcode was escaped).

## Decisions (2026-10-21, with the user)

1. **Rasterizer:** Q1-faithful now: shell out to `typst` through the new
   `quarto.paths.typst()` and `pandoc.pipe`. A Rust-native `resvg` rasterizer
   (hub support) is a follow-up strand, filed only if the hub ever needs PNG.
2. **Default format:** PNG only where it's needed, meaning LaTeX (`latex`,
   `beamer`, `pdf`) and `docx`. SVG for everything else, including HTML and
   Typst, which both handle SVG natively. Wherever PNG isn't possible (WASM),
   losing it is acceptable; it beats keeping the external service.
3. **WASM, or no typst binary, with PNG:** fail loudly through `fatal()`. No
   silent SVG fallback.
4. **`fatal(msg)`:** a global that just raises a Lua error. In a shortcode that
   becomes the existing "Shortcode error" *error* diagnostic, so the render
   fails with or without `--strict`. Richer semantics can come later.
5. **Typst path:** reuse quarto-core's lookup (`find_binary("typst",
   "QUARTO_TYPST")`), handed to Lua through `LuaOptions` rather than looked
   up again inside pampa.
6. **Base:** first developed on top of PR #882's branch. Once #882 merged, it moved to its own branch off `main` with its own PR, since it is independent of the vendoring work.

## Phases

Implemented in one commit, with the changelog entry in the commit after it.

- [x] **Phase 0: Tests.**
  - [x] pampa `system.rs`: `pandoc.pipe` returns stdout of stdin and raises
        on a non-zero exit; `with_temporary_directory(template, fn)`.
  - [x] pampa `quarto_api.rs`: `fatal` raises at the caller's line; a
        script's own `fatal` wins; `quarto.paths.typst()` is nil until set.
  - [x] pampa `shortcode.rs`: `quarto.paths.typst()` comes from `LuaOptions`.
  - [x] quarto-core `tests/integration/placeholder_shortcode.rs`, driving
        the real `placeholder.lua`: SVG for html/revealjs/typst/gfm; PNG
        default and loud failure without typst for latex/pdf/beamer/docx;
        `format=svg` override; a broken typst path is a `fatal`; with typst,
        a real `data:image/png` URI; upstream's label contract test. The
        last two skip without typst (CI provisions it).
  - [x] ~~smoke-all fixture~~: dropped. WASM smoke-all renders HTML only
        and would fail a `format=png` fixture by design, and native
        `tests.run.requires` only knows engines, not binaries. The
        integration tests above cover the same path.
  - [x] `wasm_lua.rs`: not needed, since no wasm32 cfg block was touched.
- [x] **Phase 1: pampa Lua API.** `pandoc.pipe`, the
      `with_temporary_directory` arity fix, global `fatal`, `LuaOptions.typst`
      and `quarto.paths.typst()` (filter and shortcode states; lmd goes
      through the filter state).
- [x] **Phase 2: quarto-core plumbing.** `render::find_typst(runtime)` is now
      the one lookup; `BinaryDependencies::discover` and
      `lmd::lua_options(project, runtime)` both use it. The other
      `find_binary("typst", ...)` call sites (`typst_compile.rs`,
      `pandoc_write.rs`) are untouched.
- [x] **Phase 3: `placeholder.lua`.** Ported bc2d42e00. Diff vs upstream:
      the latex/docx PNG default and the nil-typst `fatal`.
- [x] **Phase 4: Bookkeeping.** `resources/vendored.toml` notes;
      `hub-client/changelog.md` entry (the `changelogRender` wasm test
      passes).
- [x] **Phase 5: Verify** (2026-10-21, on top of #882's branch before it merged, after
      `npm install` with no lockfile churn). `cargo xtask verify
      --skip-rust-tests` with the hub build passed every step. In
      `cargo nextest run --workspace --no-fail-fast`, 17065 of 17068 passed.
      Two `quarto-preview` pdfjs_serving tests failed only because the hub
      assets were stale and pass after the hub build. `smoke_all` fails on 3
      typst fixtures that fetch remote images (`url-image-mediabag`,
      `online-logo`, `crossref-grand-finale`): the local Homebrew pandoc
      3.12.1 is built without HTTP support. That is environmental, and none
      of the three uses the placeholder.

## Follow-ups

- bd-1hor86xh: PNG placeholders in WASM. Since the investigation we learned
  that `WasmRuntime` already rasterizes SVG with `<img>`/`<canvas>`
  (`SystemRuntime::rasterize_svg`, used by `RasterizeSvgImagesStage` for
  browser docx/pptx downloads, local `.svg` files only). That is a cheaper
  route than `resvg` if the hub ever needs PNG placeholders.

## Risks / tradeoffs (draft)

- The `pandoc.pipe` and `with_temporary_directory` fixes touch shared pampa
  Lua APIs that user filters use. The arity fix widens what's accepted, so
  it's low risk; `pandoc.pipe` is purely additive.
- `--ignore-system-fonts` relies on Typst's embedded fonts to draw the label.
  That's why upstream has the contract test. Q1 bundles a pinned Typst; q2
  uses whatever `typst` is on PATH or in `QUARTO_TYPST`, so the version can
  vary.
- Global `fatal` in the pampa Lua state could shadow or clash with a user
  filter that defines its own `fatal`. Low risk, but define it so that a user
  definition wins.
