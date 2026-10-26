---
title: '`q2 docs serve`: browse the embedded documentation site offline'
date: 2026-10-26
description: 'Assessment and plan for a human-facing counterpart to `q2 docs llms`: a local web server that serves exactly the Quarto 2 documentation the running binary was built from.'
status: draft  # Design decisions recorded 2026-10-26 (see Decisions); awaiting the go-ahead to implement.
braid:
  strand: bd-x248xpyh
  priority: P3
---

## Overview

`q2 docs llms` (bd-hwop1zii, shipped v0.27.0) serves the docs site's llms.txt
mirror from the binary. Its strand left the `q2 docs` namespace open for a
human-facing counterpart and filed bd-x248xpyh for it. This plan is that
counterpart: `q2 docs serve` starts a local HTTP server showing the full docs
site (navigation, styling, search, embedded examples), and the site is exactly
the one built from the binary's own commit.

Two designs were proposed:

- **A. Pre-render at build time and embed the `_site`.** Expected to be fastest.
- **B. Embed sources and assets, and render at runtime (the equivalent of `q2
  preview --static`).** Expected to be leanest in binary size.

**Recommendation: A**, and make the archive the *single* docs embed, so `q2
docs llms` reads from it too. The measurements below show that B's size
advantage is small. With unification, A costs about **+0.2 MB** over today's
binary, and B would cost more than that.

## Decisions (2026-10-26)

1. **Command:** `q2 docs serve` and `q2 docs open` are aliases of each other
   (one clap subcommand, `visible_alias`). Neither may need network access
   for any important content. See "Offline guarantee" below.
2. **Unify now:** `q2 docs llms` moves onto the site archive in this same
   change, so there are no phases. Latency budget: ≤ 50 ms of added time per
   `docs llms` call is acceptable and > 150 ms is not. Measured at **~1 ms**
   added with llms-first archive ordering (see "Decode latency").
3. **Renaming** `agents-docs-dist/` / `build-agents-docs` is deferred to
   bd-7tkifpnh. This change keeps the current names.

## Current situation (what exists today)

- **Staging.** `cargo xtask build-agents-docs`
  (`crates/xtask/src/build_agents_docs.rs`) stages the example projects,
  renders `docs/` in place with `q2`, then copies **only the
  llms-ledger-listed files** (`docs/.quarto/llms-manifest.json`: `llms.txt`,
  `llms-full.txt` and the per-page `.md` files) into gitignored
  `agents-docs-dist/`. It also writes `embed-info.json` (commit + dirty flag).
  So the full HTML `_site` is **already rendered** whenever the llms embed is
  staged, both on dev machines and in the release workflow's `web-payloads`
  job. Design A adds no render step. The xtask simply throws `_site` away
  today.
- **Embedding.** `crates/quarto/build.rs` points `include_dir!` at
  `agents-docs-dist/` if it holds `llms.txt`, and at a placeholder otherwise.
  The embed is **uncompressed** (`include_dir`), about 2.7 MB today.
- **CLI.** `crates/quarto/src/commands/docs_llms.rs` holds the lookup logic
  over an `include_dir::Dir` (testable with synthetic trees), including href
  normalization and near-miss suggestions. `main.rs` defines `DocsCommands`,
  whose doc comment already reserves `docs serve`.
- **Release gate.** The release workflow fails any leg whose `q2 docs llms
  --embed-info` reports a placeholder or a foreign commit.
- **Compressed-embed precedent.** `crates/quarto-preview/build.rs` archives the
  SPA dirs as deterministic identity-only tar.zst (bd-rem4bpee).
  `quarto-preview/src/lib.rs` decodes the archive lazily into a
  `HashMap<String, Box<[u8]>>` behind a `OnceLock`
  (`decode_embedded_archive`). `quarto-wasm-embed` does the same for one
  file. `quarto_build_support` has `write_if_changed` and `watch_dir`.
- **Static server precedent.**
  `quarto_preview::static_mode::{build_router, serve}` (for `q2 preview
  --static`) serves a directory **on disk**. It hard-wires the live-reload
  client injection (`file_response` → `inject_client`), the SSE endpoint, the
  page-view reporting used by lazy execution, and the pdf.js viewer. Its pdf.js
  tree is already abstracted as a lookup closure (`PdfjsTree`). That shows how
  the server could serve from memory, but the site root itself is a
  `PathBuf`.
- **Browser launch.** `commands::preview::spawn_browser_open_when_ready(host,
  port, url, browser)` already exists for this purpose.

## Measurements (2026-10-26, macOS arm64, docs at `main`)

| Thing | Raw | tar + zstd-19 |
|---|---:|---:|
| Full rendered `docs/_site` (1379 files) | 59 MB | **2.88 MB** |
| …without `examples/` | 42 MB | 2.11 MB |
| …without the llms artifacts (`*.md`, `llms*.txt`) | — | 2.67 MB |
| Staged example output (`docs/examples/`) | 17 MB | 0.81 MB |
| Docs *sources* (`git ls-files docs/`, 393 files) | 2.0 MB | 0.38 MB |
| Current llms embed (`agents-docs-dist/`, uncompressed) | 2.7 MB | — |
| Installed release `q2` binary | 170 MB | — |

- The 59 → 2.9 MB ratio (~20×) is real, not a measurement error. Every page
  repeats the same navbar and a ~400-entry sidebar (`errors/index.html` alone
  is 150 KB), and zstd-19 removes almost all of that repetition.
- The `.md` companions + `llms-full.txt` add only **0.21 MB** to the
  compressed site archive, against 2.7 MB embedded raw today.
- Decompressing the full archive with the `zstd` CLI takes ~16 ms wall.
  Parsing the tar into a map in-process adds a little to that, but stays at
  tens of milliseconds.
- In-process decode cost is in "Decode latency" below.
- A native `q2 render` of a scratch copy of `docs/` (no `.quarto` cache) took
  **0.81 s wall, 700 MB peak RSS** for 387 files. No execution engine was
  needed: every `{python}` cell in the docs sits inside a markdown example
  fence.

### Decode latency (the unify-now question)

Bench: a scratch release binary with the workspace's pinned `zstd 0.13.3` +
`tar 0.4.46`, holding the real `_site` archive via `include_bytes!` (as the
embed would). Timings come from `hyperfine -N`, 50 runs, whole-process wall
time, on an M-series Mac.

| Process | Mean |
|---|---:|
| Empty bench binary (process floor) | 1.2 ms |
| Bench: decode **whole** archive into a `HashMap` (1379 files, 59 MB) | 13.6–14.2 ms |
| Bench: decode only the **llms prefix** (archive ordered llms-first), stop at first non-llms entry | **2.1 ms** |
| Today: `q2 docs llms guides/projects/preview` (uncompressed `include_dir`) | 3.7 ms |
| Today: `q2 docs llms --full` | 3.9 ms |

So a unified `docs llms` costs about **+12.5 ms** with a naive full decode, and
about **+1 ms** with llms-first ordering. Both are well inside the 50 ms
budget. On a machine 3–4× slower (old laptop, CI x86 runner), a full decode
would land around 40–55 ms, which is at the edge of the budget. The llms-first
ordering would still be under ~5 ms. **Decision: use llms-first ordering.**
The build script writes `llms-manifest.json`, `embed-info.json`, `llms.txt`,
`llms-full.txt`, and every ledger-listed `.md` first (sorted), then the rest
of the site (sorted). `docs llms` stream-decodes and stops at the first entry
outside the ledger set. The ordering costs +44 KB of compression (2.78 →
2.82 MB). `docs serve` decodes everything (~14 ms, once per server start).

An integration test should guard the ordering invariant, since a silent
regression would only cost performance. Example check: the archive's first
entry is the manifest, and every ledger path comes before any non-ledger
path.

### Network dependencies of the rendered site

A scan of every `.html`, `.css`, and `.js` in `docs/_site` for absolute
`http(s)://` URLs in `<script src>`, `<link href>`, `<img src>`, `<iframe
src>`, and CSS `@import`/`url()` found:

- **No CDN scripts or stylesheets.** Bootstrap, the quarto JS, Pagefind
  search, reveal.js and mermaid (in the examples) all live under `site_libs/`
  or `*_files/`.
- **Google Fonts `@import`** (Source Sans Pro) in the 5 site theme CSS files
  and in 24 example reveal.js theme files. Offline, the browser falls back to
  the system sans-serif stack. This is cosmetic and doesn't affect content.
- **`search-insights` from jsDelivr** inside the vendored autocomplete
  library. It only loads when Algolia insights is configured, which the docs
  site doesn't do, so it's inert.
- Math: no docs page currently renders math (0 pages with `.math` spans), so
  there is no MathJax/KaTeX CDN load today. That could change if a page
  starts using math, and the guard below catches it.

## Options

### A. Embed the pre-rendered site (recommended)

Stage the whole `docs/_site` into the dist dir, archive it at build time as a
deterministic tar.zst (copying the quarto-preview build.rs pattern), decode it
lazily into a path → bytes map, and serve that map from memory.

- **Size:** +2.9 MB, or about **+0.2 MB net** if the llms embed moves onto the
  same archive (see "Unify the embeds" below).
- **Startup:** ~tens of ms to decode, then instant. No render, so no warnings,
  no 700 MB spike, and no tool-environment sensitivity.
- **Fidelity:** byte-identical to the site CI rendered for this commit,
  search index included. The existing release gate can cover it by
  construction.
- **Build cost:** none new. The render already happens and only the staging
  filter changes.
- **Risk:** low. Every piece has a precedent in the tree.

### A′. Same embed, extract to a cache dir, serve from disk

Unpack the archive once into `dirs::cache_dir()/q2/docs/<commit>/` and point
the existing `static_mode` server at it (with reload disabled).

- This reuses `static_mode`'s path handling, directory redirects, and MIME
  types without any refactor.
- But it writes 59 MB to the user's disk and needs cache GC across versions.
  The mcp-launcher cache shows that GC has real Windows sharp edges
  (bd-5tdzaf3t: renaming a dir while holding a handle into it). It also needs
  a version key and a stale-cache story, and it still has to turn off the
  reload/page-view features.
- Not worth it next to serving from memory, which is also small.

### B. Embed sources, render at runtime

Embed `docs/` sources (0.38 MB) plus the staged example output (0.81 MB,
because the example projects live outside `docs/` and are rendered by a
separate xtask), extract them to a temp dir, run `render_once`, and serve with
`static_mode`.

- **Size:** about +1.2 MB **on top of** the llms embed, which must stay
  regardless. `q2 docs llms` cannot render the site on every invocation. So
  B's real total is ~3.9 MB uncompressed-llms, or ~1.4 MB if the llms embed is
  also compressed, against A-unified's ~2.9 MB total. B saves about 1.5 MB,
  roughly 0.9% of the binary.
- **Startup:** ~1 s plus extraction on a fast machine, more on slow laptops,
  with a 700 MB memory spike. Adding a render cache brings back A′'s cache-dir
  problems.
- **Fidelity:** the output depends on the user's machine. Their env vars, the
  `QUARTO_*` settings, a future docs page that needs an engine or Typst, and
  the 33 render warnings the docs currently emit all leak in. A render bug in
  a given release also breaks that release's docs. A's site is checked once,
  in CI.
- **Upside:** the docs would act as a large smoke test of the shipped binary
  on every machine. That's interesting, but it doesn't belong in a docs
  viewer.

### B2. Embed sources, render lazily per page through the default `q2 preview` (hub + SPA)

Load the docs sources into the preview hub and let the SPA render pages on
demand, so there's no upfront render.

- This brings the whole hub/automerge/WASM stack into a read-only docs
  viewer, adds the preview chrome, and makes the first view of each page wait
  on a render. Search (Pagefind, a whole-site index) doesn't fit a
  per-page-render model.
- Rejected. It's the most machinery for the least gain.

### C. No embed: open the versioned public website

`q2 docs open` launches the browser at a versioned URL on the published docs
site.

- This costs nothing in binary size, but it needs network access, and it
  needs per-version hosting that doesn't exist today. The docs site would
  have to keep one copy per release, or the command would show the wrong
  version's docs, which defeats the "docs for the version you run" property
  `q2 docs llms` was built around.
- Worth keeping as an optional *fallback* when the embed is a placeholder
  (dev builds). It isn't a replacement for the embed.

## Proposed design (option A)

### CLI

```
q2 docs serve [PAGE] [--port N] [--host H] [--no-browser] [--browser CMD]
q2 docs open  [PAGE] ...            # alias; identical behaviour
```

`open` is a clap `visible_alias` of `serve`, so both spellings appear in `q2
docs --help` and parse to the same args. A parse test asserts they're
equivalent, mirroring `docs_llms_and_agents_info_parse_identically`.

- Long-running, like `q2 preview`: prints `Serving Quarto 2 docs (<commit>) at
  http://127.0.0.1:<port>/`, opens the browser unless `--no-browser`, and
  stops on Ctrl-C.
- `PAGE` (optional) deep-links. It is resolved with the **same normalization
  and near-miss suggestions as `q2 docs llms <href>`** (`guides/projects/preview`,
  `guides/projects/preview.md`, `.html` all work), so an unknown page fails
  fast with suggestions before the server starts. This later lets error
  messages say `q2 docs serve errors/markdown/Q-2-47`.
- Port defaults: reuse whatever `q2 preview` uses for picking a free port.
  Binds to loopback only by default.
- Placeholder embed (fresh clone): fail with the same `REBUILD_HINT` as `docs
  llms`. The command never falls back to the public website, since it must
  not depend on the network.
- `--embed-info` stays on `docs llms`, which reports on the single embed (see
  below).

### Offline guarantee

Three parts:

1. **The server never fetches.** It serves only the decoded embed. No proxying,
   no fallback to the public site.
2. **The content needs nothing remote.** `build-agents-docs` gets a staging
   check that scans the staged HTML/CSS/JS for absolute-URL subresources
   (`<script src>`, `<link rel=stylesheet|preload|modulepreload href>`, `<img
   src>`/`srcset`, `<iframe src>`, `<video>`/`<audio>`/`<source src>`, CSS
   `@import` and `url()`) and **fails staging** on any hit outside a small,
   commented allowlist. The allowlist starts as just the Google Fonts
   `@import` and the vendored autocomplete library's inert insights loader.
   Ordinary `<a href>` links to external sites are fine, since they're
   navigation, not content. A docs page that starts pulling MathJax or
   anything else from a CDN then breaks the docs build, not the user's
   offline reading. A unit test drives the scanner with fixtures.
3. **Fonts (decided 2026-10-26: leave it).** Offline, the Source Sans Pro
   `@import` fails and text renders in the system sans-serif font, which is
   acceptable. Online, it makes one request to Google, the same request the
   published site makes. The docs server serves the CSS unmodified, and the
   allowlist names the import explicitly. A theme variant that doesn't need
   Google Fonts may come later, as separate work. It's out of scope here.

### Unify the embeds

Move `q2 docs llms` onto the site archive, so there is one docs embed and one
staleness story:

- **Staging:** `build-agents-docs` copies the whole `_site` (instead of the
  ledger subset) plus the ledger itself (`llms-manifest.json`) and
  `embed-info.json`. The ledger travels into the archive because `docs llms`
  must still tell real `.md` companions from any user-resource `.md` in the
  site. That was the reason the ledger filter existed.
- **Embed:** `crates/quarto/build.rs` archives the dist dir to
  `OUT_DIR/docs-site.tar.zst` (deterministic: llms-first then sorted entry
  order, as in "Decode latency"; zeroed mtimes; level 19;
  `write_if_changed`), and the placeholder path stays. The `include_dir`
  dependency for docs goes away.
- **Lookup:** `DocsEmbed` today takes an `include_dir::Dir`. Change it to take
  the decoded map (`&HashMap<String, Box<[u8]>>` or a small trait), so the
  synthetic-tree unit tests keep working with a constructed map. Two decode
  entry points share one helper: `decode_llms_prefix()` (stops at the first
  non-ledger entry; used by `docs llms`) and `decode_all()` (used by `docs
  serve`).
- **Cost to `docs llms`:** measured at ~+1 ms (see "Decode latency"). As an
  implementation check, re-run the `hyperfine` comparison against the real
  `q2` before and after.
- **Naming:** `agents-docs-dist/` and `build-agents-docs` keep their names
  here; the rename is bd-7tkifpnh.

### Server

A small dedicated in-memory server, **not** a refactor of `static_mode`:

- `static_mode`'s value is the reload/watch/lazy-execution behaviour, and a
  docs viewer wants none of it. Making its root polymorphic would put a
  memory-vs-disk abstraction into the preview's hot path for one consumer.
- The docs server needs GET/HEAD only: percent-decode with the same `..`/`\`/NUL
  refusal (`decode_segments`; make it `pub(crate)`/`pub` and reuse it), map
  `dir/` to `dir/index.html`, 301 `dir` → `dir/`, use content types from
  `spa_manifest::content_type_for`, and return a plain 404 (the docs site has
  no `404.html`). `Cache-Control: no-cache` is fine. Responses over loopback
  don't need gzip.
- Location: a module in `quarto-preview` (for example `embedded_site.rs`),
  which owns the axum stack and these helpers, taking the decoded map as a
  parameter. The `quarto` crate binds the real archive. Integration tests can
  then drive it with a synthetic map via `tower::ServiceExt::oneshot`, as the
  preview route tests do.

### Things to verify during implementation

- **Pagefind search works over `http://127.0.0.1`** from the in-memory server:
  correct MIME for `.pagefind`, `.pf_fragment`, `.pf_index`, `.pf_meta`, and
  `.wasm`, plus whatever headers Pagefind's fetches expect.
- Example iframes (`/examples/...`) and reveal.js examples load.
- Navbar/sidebar links are root-relative or relative and don't point at a
  production `site-url`. (`docs/_quarto.yml` sets no `site-url` today.)
- `cargo xtask check-noop-rebuild` stays green with the new archive step,
  since the build script must not become always-stale (bd-7xlph6ki).
- Windows: tar entry paths use forward slashes regardless of host. Stage with
  `Path` joins (the current `stage_files` already does).

### Release and docs wiring

- `web-payloads` already uploads the dist dir, so only the path changes if it
  gets renamed.
- Extend the per-leg gate: `q2 docs llms --embed-info` reports `source: real`
  and the commit. Add a smoke check that `q2 docs serve --no-browser` answers
  `/` with 200 (start the server, curl it, kill it) on at least the Linux leg.
- Update `docs/guides/embedded-docs.qmd` (add a "for humans" section),
  AGENTS.md § "Refreshing the embedded docs", and the `DocsCommands` doc
  comment.

## Work items (pending go-ahead)

- [x] Decide: command name (`serve` = `open`), unify now, rename deferred
      (bd-7tkifpnh), Google Fonts import left as-is (system-font fallback
      offline).
- [ ] Staging: copy the full `_site` + ledger + embed-info; update xtask
      unit tests (stale-file clearing, missing-file errors).
- [ ] Staging offline check: subresource scanner + allowlist + fixture tests.
- [ ] `crates/quarto/build.rs`: deterministic llms-first tar.zst of the
      dist, placeholder unchanged; check no-op rebuild; ordering-invariant
      test.
- [ ] Shared decode helper (lift `decode_embedded_archive` to a reusable
      spot, or copy it in the crate's style).
- [ ] `quarto-preview` in-memory site server + route tests (dir redirect,
      index, traversal refusal, MIME, 404, HEAD).
- [ ] `q2 docs serve` CLI: args, page deep-link via `docs_llms`
      normalization, browser launch, placeholder error; parse tests in
      `main.rs`.
- [ ] Port `DocsEmbed` to the decoded map (prefix decode); re-measure
      `docs llms` with hyperfine before and after (budget ≤ 50 ms added);
      drop `include_dir` for docs.
- [ ] `serve`/`open` alias parse-equivalence test.
- [ ] Integration test in `crates/quarto/tests/integration/` that works in
      both embed states (like `docs_llms_cli.rs`).
- [ ] Manual check in a browser **with the network disabled**: navigation,
      search, example iframes, a reveal.js example; confirm the network panel
      shows only allowlisted outbound requests.
- [ ] Docs page, AGENTS.md, release-workflow smoke check.
- [ ] `cargo xtask verify`; Windows cross-clippy for `quarto` and
      `quarto-preview`.
