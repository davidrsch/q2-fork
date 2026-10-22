---
title: 'pandoc.read text gets a home in the document''s SourceContext: correct locations (GH #884), forwarded reader diagnostics (bd-fwswbxnl), and a commonmark reader (GH #885)'
date: 2026-10-22
description: 'Nodes pandoc.read returns carry offsets into the string, which later passes read as offsets into the host document. Give each pandoc.read call its own hash-FileId file, register it in the document''s contexts when the host drains it, and build the commonmark arm on the same mechanism.'
status: draft  # awaiting go-ahead
braid:
  strands:
    - bd-pandoc-read-host-locations-k9v9bviz   # GH #884 (Phases 0-2)
    - bd-fwswbxnl                              # reader diagnostics dropped (Phase 3)
    - bd-pandoc-read-commonmark-7vuj788t       # GH #885 (Phase 4)
github:
  issues: [884, 885]
---

**Branch (proposed):** integration branch `feature/pandoc-read-source-locations` off `main`. Each strand gets a topic branch via `cargo xtask switch-task <id> --from feature/pandoc-read-source-locations` and is merged `--no-ff`.
**Depends on:** `2026-10-22-commonmark-reader-single-fileid.md` (GH #883) for Phase 4 only.

## Why one plan for #884 and #885 (and bd-fwswbxnl)

All three are the same question: *where does text passed to `pandoc.read` live in the document's `SourceContext`?* bd-fwswbxnl deferred it, #884 is the user-visible consequence, and #885 needs the answer for a second reader. Once the mechanism exists, #885's location work is wiring, and so is bd-fwswbxnl. Planning them separately would mean designing the mechanism three times. They still ship as **separate PRs** (one per strand) on an integration line, so the bug fix (#884) doesn't wait on the feature (#885).

#883 stays in its own plan: no design question, tiny, and only Phase 4 here needs it.

## Assessment

### The bug (#884), confirmed by reading the code

1. `pandoc_read`\'s qmd arm (`crates/pampa/src/lua/readwrite.rs:260`) calls `readers::qmd::read(…, "<pandoc.read>", …, None)`. The fresh context's FileId(0) is the string, and every node comes back as `Original { FileId(0), offset-into-string }`.
2. The nodes cross into Lua and come back out with that `SourceInfo` intact.
3. In the host document, FileId(0) is the document. Any later pass that maps a node span (e.g. Q-2-49 in `transforms/reference_link_diagnostics.rs`) lands at the string's offsets in the document, usually inside the front matter.

**Repro at `main` @ `d3ed54fe4` (debug `q2`, 2026-10-22):**

- lmd: the issue's `loc.qmd`, `--json-errors` → Q-2-49 at `start_line: 2, start_column: 12`. The text render underlines `nostic location from p` in the title.
- Lua filter: a `Pandoc(doc)` filter appending `pandoc.read('Allowed kinds: ["skills", "commands"] only.', 'markdown').blocks` to a different document → also line 2, column 12, because byte 15 again falls in that document's `title:` line. So the filter path is affected exactly like lmd, not just in theory.

Scratch files: `/private/tmp/claude-501/-Users-cscheid-rooms-room-3-q2/ece4cad0-62ae-4ed2-b7a5-643f30c05d12/scratchpad/r884/` (Phase 0 recreates them as fixtures).

### Who is affected (narrower than the issue suggests)

- **Lua filters** (`lua/filter.rs` → `UserFiltersStage`): affected.
- **lmd cells** (`LmdExecuteStage`): affected. `relocate_to_cell` protects only the desugaring pass inside the stage.
- **Lua shortcodes: not affected.** `stamp_shortcode_anchors` (`quarto-core/src/transforms/shortcode_resolve.rs:912`) replaces every returned block's and inline's `source_info` with `Generated { by: shortcode, from: [Invocation(token)] }`, so `pandoc.read` output from a shortcode handler already resolves to the shortcode call. (Nested `attr_source` / target spans are not re-stamped. They're a minor residual and get handled by the same mechanism for free if the shortcode engine also drains; see Phase 2.)

### Facts the design rests on

- **Hash-based FileIds are an established pattern in the document context.** `MetadataMergeStage` registers `_quarto.yml` and friends under `quarto_yaml::file_id_for_filename(path)` via `add_file_with_id`, into **both** `doc.ast_context.source_context` and `doc.source_context` "in the same order" (`metadata_merge.rs:222`). A hash id needs no knowledge of the host context's current length, which matters because quarto-source-map 0.3.0 has no `len()` and is an external crate (`posit-dev/quarto-source-map`).
- **An id the context doesn't know resolves to nothing, not to something wrong.** `map_offset` → `ctx.get_file(id)?` → `None`, and the renderer then has no snippet. `--json-errors`\' `json_source_file` drops to its fallback when the file doesn't map, or when its path doesn't exist on disk (`quarto/src/commands/render.rs:2062`).
- **The qmd reader can parse "into" a parent.** With `parent_source_info: Some(p)`, Ok-path spans become `Substring(p, …)` and Err-path diagnostics are rerooted (`reroot_diagnostics_into_parent`). **But coverage has stragglers:** `create_line_break_inline` (`treesitter_utils/text_helpers.rs:385`) stamps `SoftBreak`/`LineBreak` with a hard-coded `FileId(0)`, ignoring the parent. There are ~20 `FileId(0)` literals across `pandoc/treesitter_utils/*` and `pandoc/location.rs`. Some are tests or legitimate, but each needs checking. Any straggler would keep #884 alive for that node type, so Phase 0 includes a property test that catches all of them.
- **lmd chunks are named `@<doc path>` with line padding** (`lmd_execute.rs:239`), so `filter_source_info(lua)` inside a cell yields `By::filter("<doc path>", <document line>)`. That's the call-site frame the issue asks for, at no extra cost. In a filter it yields the `.lua` path and line.
- **Host drain points already exist** for Lua-side state: `extract_lua_diagnostics` in `apply_lua_filter_with`, and `LmdRuntime::take_diagnostics`. `UserFiltersStage` adopts `output.context` into `doc.ast_context` only (`user_filters.rs:286`), so it must also register into `doc.source_context`. `doc.source_context` is what final diagnostics render against (e.g. `pipeline.rs:1230`, `render_html.rs:161`).

## Design

**Each `pandoc.read` call mints its own virtual file.** At call time the binding:

1. Builds a label for the file from the call site: `{chunk}[pandoc.read, line {line}]` (e.g. `loc.qmd[pandoc.read, line 14]`), using `filter_source_info(lua)`\'s `By::filter(path, line)`, with the path shown relative where possible. The form follows the existing notebook-cell label (`notebook.ipynb[cell 2, markdown]`). The label is the only provenance for now; a structured origin is deferred to a design session (see the appendix).
2. Mints `FileId = hash(label, content, per-Lua-state counter)`. Same recipe family as `file_id_for_filename`. The counter keeps two reads from the same line, such as a loop, apart.
3. Parses so that every span is rooted at that FileId:
   - qmd arm: `parent_source_info = Some(SourceInfo::original(fid, 0, len))`, where `len` covers the newline-terminated text the reader actually parses.
   - commonmark arm (Phase 4): `commonmark::convert(text, fid)` (the entry point from the #883 plan), so no parent is needed.
4. Records `(fid, label, content)` in a Lua app-data registry, `PandocReadSources`.

**Hosts that can, drain the registry and register each file** with `add_file_with_id` into every context they hold, skipping ids already present (re-entry or collision, as `metadata_merge` does). Once registered, a diagnostic about a node from the string shows **the string's own text** with an exact underline, headed by a label naming the call site.

**Hosts that don't drain get an honest absence:** the FileId is unknown to the document context, so a downstream diagnostic carries no location. That's the issue's second-preference outcome, reached without a separate stripping walk, and it's never a confident wrong line. It covers the pampa CLI, the conformance harness, and any future host.

Why not the alternatives:

- *Substring of a `Generated` anchor* (the "strip to `By::filter`" option): quarto-source-map documents that a Substring chain always bottoms out in an `Original`. Violating that invariant risks breaking consumers (`resolve_byte_range`, the attribution sidecar, the incremental writer).
- *Re-stamp everything to `Generated(By::filter)` with an AST walk:* this loses the in-string precision that the registry gives for free, and it needs a new full-AST walker in pampa. (`shortcode_resolve` has one but it's private to quarto-core.)
- *Sequential `add_file` ids:* the Lua side would need the host context's length at call time. There's no accessor, and shortcodes only hold `&SourceContext`.
- *Clone the host context into the Lua state:* clones whole document contents per filter, and still needs lockstep handling for the two contexts.

## Plan

### Phase 0 — failing tests first

1. **e2e, lmd** (`crates/quarto/tests/integration/pandoc_read_locations_e2e.rs`, registered in `main.rs`): the issue's `loc.qmd`, `--json-errors`. Assert the Q-2-49 warning is **not** at `loc.qmd` line 2. After Phase 2, also assert that the human-readable output shows the `["skills", "commands"]` snippet under a `…loc.qmd[pandoc.read, line 14]` label, and that `--json-errors` reports no `source_file` for it (the label isn't a path on disk, so `json_source_file` returns `None`).
2. **e2e, Lua filter:** same content passed through a filter file that returns `pandoc.read(...)` blocks. Same assertions, with the `.lua` path and line in the label.
3. **Property test** (pampa, `lua/readwrite.rs` tests or `tests/integration/`): `pandoc.read` a fixture that exercises soft/hard breaks, emphasis, links with attrs and titles, images, spans, code, math, quotes, a pipe table, a list-table div, a definition list, and footnotes. Walk every `SourceInfo` in the result (node spans, `attr_source`, target spans) and assert none resolves to `FileId(0)` and all resolve to the minted FileId. This fails today on everything, and after Phase 1 it fails on each straggler. That's the point.
4. **Unit:** the registry records one entry per call, labels carry the chunk and line, and two reads from one line get distinct ids.

### Phase 1 — mint per-call FileIds in `pandoc.read` (pampa only)

- `lua/readwrite.rs`: label, hash FileId, registry (`PandocReadSources` in Lua app data, created lazily), parse the qmd arm with the parent, record.
- **Fix the parent-ignoring stragglers** the property test finds, starting with `create_line_break_inline`. Each fix threads the context's base (as `location.rs:299/342/366` already do) instead of `FileId(0)`.
- Result: #884's wrong location becomes *no* location everywhere. This is an intermediate state, not a separate merge: Phases 0–2 ship as one PR (decided 2026-10-22).

### Phase 2 — hosts register the files (fixes #884 properly)

- `lua/filter.rs`: drain into `FilterOutput` (new field, e.g. `read_sources: Vec<VirtualSource>`), threaded through `apply_lua_filters_with` and `unified_filter.rs`\'s `FilterOutput`. Also register into `output.context.source_context` so pampa-CLI filter runs get locations too.
- `UserFiltersStage`: register into `doc.source_context` (lockstep with the adopted `ast_context`). Use one helper, `register_virtual_sources(doc, &sources)`, rather than inlining a third copy of the both-contexts loop.
- `LmdRuntime::take_read_sources()` next to `take_diagnostics()`; `LmdExecuteStage` registers via the same helper.
- Shortcode engine: drain and register too, for the nested attr/target spans `stamp_shortcode_anchors` leaves alone. **A drive-by, per review (2026-10-22): do it if it stays simple, otherwise drop it.** The intended route follows an existing pattern. `AstTransformsStage` already copies `render_ctx.artifacts` / `render_ctx.resource_copies` back to the stage context after the transforms run (`ast_transforms.rs`, just after the `render_ctx` setup at `:251`). So: add a `read_sources` field to `RenderContext`, have the shortcode transform (`shortcode_resolve.rs:1220`) push `lua_engine.take_read_sources()` into it when it finishes, then have `AstTransformsStage` copy it back and call `register_virtual_sources(doc, …)` once its `&doc.ast_context.source_context` borrow ends. **Bail-out:** if this needs more than those touch points (e.g. the borrow can't end before registration, or the engine is created in more than one place), leave shortcodes on the Phase 1 "no location" behavior for nested spans, note that in the PR, and file a strand.
- `pampa main.rs`: nothing extra, since it renders against `output.context`.
- Verify the e2e assertions on the snippet and label.

### Phase 3 — forward the reader's own diagnostics (bd-fwswbxnl)

- Ok path: push `_warnings` (already rooted at the minted FileId through the parent) into the Lua diagnostics channel that `quarto._diagnostics` drains, so they reach the render like any filter warning.
- Err path: same, then raise. The Lua error message stays as it is (titles joined), so `pcall` users see no change. The structured diagnostics carry the locations.
- json arm: forward the desugar collector's diagnostics (e.g. Q-2-35) the same way; the comment there says they're currently dropped for lack of a channel.
- Test: Q-2-54 (definition list) from `pandoc.read` in an lmd cell appears with the string's snippet.

### Phase 4 — `pandoc.read(text, "commonmark")` (GH #885)

Requires the #883 plan merged.

- `options.rs`: add `"commonmark"` to `SUPPORTED_READER_FORMATS`. Since `register_format_lists` iterates it, `pandoc.readers.commonmark` comes along automatically. Make sure the pampa CLI's own `--from` handling still routes `commonmark` to its existing arm (`main.rs:323`).
- `lua/readwrite.rs`: a `"commonmark"` arm that mints the FileId (Phase 1 helper), calls `commonmark::convert(text, fid)`, then runs `desugar_document` (parity with the json arm, so a `.list-table` div still becomes a table), forwards collector diagnostics (Phase 3), and records the source.
- Decisions to document in the Lua filter guide (`docs/guides/authoring/lua-filters.qmd`):
  - *Implicit figures:* they come from `desugar_document`, same as the json arm. Recommend keeping them, for consistency with every other route into the AST.
  - *Heading ids:* comrak assigns none. Recommend not adding them in the reader. Document-level ids come from the pipeline's heading-id transform, and Pandoc's own `commonmark` reader without `+auto_identifiers` adds none either. **Verify that transform runs after `UserFilters`/`LmdExecute` before stating this in the docs.**
  - *Extensions* (`commonmark+…`): **ignored**, as the qmd arm does (decided 2026-10-22). Raising an error is reasonable only if it's done uniformly across all reader formats, which is out of scope here. Document in the Lua filter guide that `pandoc.read` ignores extensions for every format.
  - The precision limits the issue lists (`Code` span offsets, link/image URLs without provenance, the entity-then-`&`/`\` tiling blind spot). Put these in a code comment in the arm, not in user docs.
- Tests: the issue's motivating strings (`{env:VAR}`, `$HOME`, `"<name>"`, `["a", "b"]`) round-trip unchanged through `commonmark` where `markdown` throws or alters them. Phase 0's property test is parameterized over both readers. An e2e diagnostic about a commonmark-read node shows the string snippet.
- **WASM size check:** comrak is an unconditional pampa dependency but is currently reachable only from the pampa binary, so it's likely dead-stripped from the hub-client wasm. The new arm pulls it in. Measure `wasm-quarto-hub-client` bundle size before and after and report it in the PR.

### Verification (every phase)

- `cargo nextest run -p pampa -p quarto-core -p quarto`
- `cargo xtask verify --skip-hub-build`; Phase 4 also runs the hub build for the size check.
- No `cfg(target_arch = "wasm32")` blocks in `filter.rs`/`shortcode.rs` are expected to change. If one does, update `crates/pampa/tests/wasm_lua.rs` per `.claude/rules/wasm.md`.

## Risks / things to check during implementation

- **Consumers that assume a node's FileId is the document.** The attribution sidecar (`resolve_byte_range`) and the incremental qmd writer's verbatim-copy decision. Today these see bogus FileId(0) offsets for `pandoc.read` nodes, so the change should be strictly safer, but grep for `FileId(0)` / `root_file_id() ==` comparisons in `quarto-core` and hub-client's TS AST consumers and confirm.
- **`SourceContext` growth:** one file per `pandoc.read` call. A data-driven page making ~1,100 calls (the #885 sweep) adds ~1,100 small files plus their `FileInformation` line indexes. Probably fine, but measure. If needed, register lazily only for files that some surviving node or diagnostic references. That adds complexity, so measure before deciding.
- **Preview / hub-client serialization** of `SourceContext` (`file_id_map` is serde-skipped when empty, otherwise serialized). Check the preview path doesn't choke on extra sparse ids. `_quarto.yml` already exercises this, so it's probably fine.
- **Hash collisions** with a dense id: astronomically unlikely, and guarded by the `get_file(id).is_none()` check, as in `metadata_merge`.

## Open questions for review

1. ~~**Label vs. structured origin.**~~ **Resolved 2026-10-22:** do the local fix: the call site goes in the path label only, with no quarto-source-map change. A structured origin raises a broader design question (a per-case `FileOrigin` variant vs. a general shape), which is deferred to a standalone session: `bd-virtual-file-origin-design-2h808wi3`. Prep notes are in the appendix.
2. ~~**Phase 1 as a standalone merge.**~~ **Resolved 2026-10-22:** one PR for Phases 0–2.
3. ~~**Shortcode drain.**~~ **Resolved 2026-10-22:** do it as a drive-by if it stays simple, otherwise drop it (see the Phase 2 bullet for the route and the bail-out criterion).
4. ~~**Commonmark extensions.**~~ **Resolved 2026-10-22:** ignore them, as the qmd arm does. Any error should be uniform across all formats; not in this plan.

## Appendix: structured provenance for virtual files (prep for a design session)

**Status:** not part of this plan's work. Preparation notes for `bd-virtual-file-origin-design-2h808wi3`. This plan ships with a path label only.

### The question

`pandoc.read` files are *virtual*: their content isn't a file on disk, and it isn't a byte-identical slice of one either. The text came from a Lua string, which may have escapes or may have been computed at runtime. quarto-source-map has exactly one structured way to say where virtual content came from, `FileMetadata::origin: Option<FileOrigin>`, and its only variant is `NotebookCell`. The obvious move is a second variant, sketched below as `ScriptString`.

**The smell:** quarto-source-map is a general crate. It underlies quarto-yaml's annotated values and the doctemplate parser as well as pampa, so a variant per producer (`NotebookCell`, `ScriptString`, …) bakes Quarto-specific producers into it and grows without bound. `NotebookCell` already set that precedent. The session should decide whether to accept per-case variants, or to replace the enum with a general shape before there are two of them.

### Option A: the per-case variant (what a minimal change looks like)

```rust
#[non_exhaustive]                      // new: makes future variants additive
pub enum FileOrigin {
    NotebookCell { /* unchanged */ },
    /// Text a script passed to a string-to-AST API (e.g. Lua `pandoc.read`).
    ScriptString {
        script_path: String,   // filter .lua path, or the document for lmd (chunks are `@<doc>`)
        line: usize,           // 1-based call line in script_path
        via: String,           // "pandoc.read"
    },
}
```

- `Display`: `loc.qmd[pandoc.read, line 14]`. This is already the label this plan uses, so text output would not change when an origin lands.
- JSON: `origin: {"kind":"script_string", …}`, through the existing serde tag. The `schemars` derive follows.

### Option B: seeds for a general shape

These are candidates to evaluate, not decisions.

- **What distinguishes an origin from a `Substring`:** a `Substring` already models virtual content that *is* a byte-identical slice of a host file. An origin is needed only when content was *derived* from a host location without being byte-identical: a notebook cell's source is JSON-escaped in the `.ipynb`, and a Lua string literal has quotes and escapes, or doesn't exist as a literal at all. A general shape is therefore something like "derived from host location H by producer P".
- **Reuse the existing provenance vocabulary.** quarto-source-map already has `Generated { by: By, from: [Anchor] }`, with `By::filter(path, line)` (exactly what `filter_source_info` returns for a `pandoc.read` call) and role-labelled anchors such as `Invocation`. An `origin: Option<Generated>`, or a struct pairing a `By` with an optional host `SourceInfo` anchor, would express both notebook cells and script strings without any new enum variant, and producers outside Quarto could use it the same way.
- **A host anchor as a `SourceInfo`** would let the renderer add a secondary "called from here" frame pointing into the host file, instead of encoding the call line in a label.
- **Survey first:** list every place that registers a virtual file today (notebook cells, engine intermediates in `engine_execution.rs`, config files in `metadata_merge.rs`, `pandoc.read`), and check whether quarto-yaml or doctemplate create any. Then see which of them would want an origin.

### Questions any option has to answer

- **JSON coordinates.** For notebooks, `source_file` is the notebook but `start_line`/`start_column` are relative to the cell, so consumers must read `origin` to interpret them. For script strings the options are: (a) `source_file: null` with only the in-string coordinates (this plan's behavior); (b) `script_path` with the coordinates rewritten to the call line; (c) the notebook convention. Nothing in hub-client or ts-packages reads `origin` today.
- **Hyperlink line.** `quarto-error-reporting`\'s `hyperlink_target` (`diagnostic.rs:1021`) returns only a path. The hyperlink line comes from the mapped location, which for virtual content is a line *inside* the virtual file. Pointing at the host's line needs a line override in that function's return value. This is a private change, but it's in the published crate.

### Release cost (same for A and B)

`FileOrigin` is not `#[non_exhaustive]`, and published crates match on it or expose `SourceInfo` in their APIs, so either option is semver-breaking and needs ordered publishes:

1. `quarto-source-map` 0.3.0 → 0.4.0: the change, plus `#[non_exhaustive]` (or the general shape) so the next addition is additive.
2. `quarto-error-reporting` 0.4.0 → 0.5.0: `hyperlink_target` handles the new case.
3. `quarto-yaml` 0.5.0 → 0.6.0: dependency bump only, required because q2 can't link two source-map versions.
4. q2: bump all three workspace versions, add the arm in `quarto/src/commands/render.rs::json_source_file` (`:2073`), and set the origin when `pandoc.read` files are registered. Add a test next to `ipynb_parse_error_json_carries_cell_origin` (`crates/quarto/tests/integration/json_errors.rs`).
