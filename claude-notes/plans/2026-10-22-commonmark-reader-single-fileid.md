---
title: 'commonmark reader: register the input once and stamp nodes with that FileId (GH #883)'
date: 2026-10-22
description: 'readers::commonmark::read registers its input twice and stamps every node with the content-less first entry; register once, use the returned FileId, and expose a FileId-taking entry point for embedding callers (pandoc.read, GH #885).'
status: draft  # awaiting go-ahead
braid:
  strand: bd-commonmark-reader-fileid-xtvc23oz
github:
  issue: 883
---

**Branch (proposed):** `braid/bd-commonmark-reader-fileid-xtvc23oz` off `main`
**Related plan:** `2026-10-22-pandoc-read-source-locations.md` (GH #884 / #885). This plan is a prerequisite of that plan's Phase 4 and independent of everything else there, so it ships alone and first.

## Why a separate plan

Of the three issues, #883 is the only one with no design question: the qmd reader already shows the correct pattern, the fix is a few lines, and one test needs flipping. It doesn't depend on the `pandoc.read` source-registry work, but that work depends on it. Bundling it would hold a trivially reviewable fix behind a design discussion, so it gets its own PR.

## Assessment

Confirmed at `main` @ `d3ed54fe4`. `crates/pampa/src/readers/commonmark.rs::read`:

1. `ASTContext::with_filename(filename)` → `add_file(path, None)` → **FileId(0)**, `content: None`, `file_info` only if the path exists on disk.
2. `add_file(normalized, Some(input))` → **FileId(1)**, with content.
3. `let file_id = FileId(0);` → every node is stamped with the content-less entry.

`SourceContext::add_file` (quarto-source-map 0.3.0) doesn't dedupe, so both entries persist.

**Who's affected today:** the only caller is the pampa CLI (`--from commonmark`, `main.rs:325`), which passes a real path, so `map_offset` works because FileId(0) re-reads the file from disk. In-memory input has no working path: nothing in the AST maps to a line or column. The planned `pandoc.read(…, "commonmark")` would always be in-memory.

**Pinned, not caught:** `tests/integration/test_diagnostic_path_normalization.rs::commonmark_reader_source_context_uses_forward_slashes` asserts the path at **FileId(1)** and explains why in a comment. It must flip.

**Second wrinkle, found while reading for #885:** `SourceLocationContext::new(input, file_id)` is the only place the FileId enters comrak-to-pandoc. It's used by `sourcepos_to_source_info` *and* by the `Str`/`Space` lockstep walker (`text.rs`, via `ProvenanceBuilder::in_file(file_id, …)`). So once the reader passes the right id, every span is right, and a caller that already owns a `FileId` (for example, one registered in a host document's context) can get correct spans just by passing that id. That's the entry point #885 needs, and it's cheaper than the `parent_source_info` branch the #885 issue proposes (see the other plan, Phase 4).

## Plan

### Phase 0 — failing tests first

- `readers/commonmark.rs` unit test: `read("hello *world*\n", "<mem>")` → the context holds **exactly one** file, at FileId(0), with content `Some("hello *world*\n")`. Every node's `root_file_id()` is FileId(0), and `map_offset(0, &ctx)` on the `Emph` resolves to byte offset 6 of that file (the `*` of `*world*`). This fails today because there are two files and nothing maps for `<mem>`.
- Flip `commonmark_reader_source_context_uses_forward_slashes` to FileId(0), and also assert that `get_file(FileId(1))` is `None`. Update the comment, since the reason it gave is gone.

### Phase 1 — fix

Split the reader into two functions:

```rust
/// Convert CommonMark `input` to Pandoc, stamping every span with `file_id`.
/// The caller owns registering `input` under `file_id` in its SourceContext.
pub fn convert(input: &str, file_id: FileId) -> Pandoc { /* comrak parse + convert_document_with_source */ }

/// Read CommonMark into a fresh ASTContext whose only file is `input`.
pub fn read(input: &str, filename: &str) -> (Pandoc, ASTContext) {
    let mut context = ASTContext::with_filename(filename.to_string());
    let normalized = context.filenames[0].clone();
    context.source_context = SourceContext::new();          // as qmd.rs does
    let file_id = context.source_context.add_file(normalized, Some(input.to_string()));
    (convert(input, file_id), context)
}
```

Use the `FileId` that `add_file` *returns*; don't hard-code it (cf. bd-itj2mjkr in `engine_execution.rs`). Keep `context.filenames` as it is: `with_filename` already put the normalized name at index 0.

Name check: if `convert` collides with something or reads badly next to `comrak_to_pandoc::convert_document*`, use `read_into(input, file_id)` instead. I'll match whatever reads best in `readers/mod.rs`.

### Phase 2 — verify

- `cargo nextest run -p pampa -p comrak-to-pandoc`
- `cargo run --bin pampa -- --from commonmark` on a scratch file with a construct that warns (if there is one; commonmark rarely produces diagnostics), and confirm text output is unchanged.
- `cargo xtask verify --skip-hub-build`

## Out of scope

- The extra copy of the input (`SourceLocationContext` keeps its own `String`; see its doc comment). This plan doesn't make it worse, and it isn't worth touching here.
- Any `pandoc.read` change (other plan).

## Open questions for review

1. `convert` vs `read_into` naming. No strong preference; defaulting to `convert`.
