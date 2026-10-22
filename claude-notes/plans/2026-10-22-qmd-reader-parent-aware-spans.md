---
title: 'qmd reader: make every span honor parent_source_info'
date: 2026-10-22
description: 'About 40 span constructions in the tree-sitter conversion build Original(current_file_id(), range) and ignore parent_source_info, so a reparse into a parent roots emphasis, attribute and similar spans in the wrong file. Route them through the existing parent-aware helper.'
status: done  # implemented 2026-10-22 on feature/pandoc-read-source-locations
braid:
  strand: bd-qmd-reader-parent-aware-spans-mq5u2001
---

**Branch:** `braid/bd-qmd-reader-parent-aware-spans-mq5u2001-…` off `feature/pandoc-read-source-locations`
**Context:** found while fixing GH #884; see the "As built" section of `2026-10-22-pandoc-read-source-locations.md`.

## Assessment

`readers::qmd::read(…, parent_source_info: Some(p))` is how embedded strings are parsed in place: config values, `!md` metadata, Lua-filter config values. With a parent, `node_source_info_with_context` and `range_to_source_info_with_context` (`pandoc/location.rs`) build `Substring(p, start, end)`. But about 35 sites in `pandoc/treesitter.rs` and `pandoc/treesitter_utils/*` build `SourceInfo::from_range(context.current_file_id(), range)` directly, and five more pass `context.current_file_id()` into helpers. All of these ignore the parent, so they produce `Original(<the reparse's own file id>, offset-into-the-string)`.

A probe through `pandoc.read` (when it still used a parent) showed the effect. The own spans of `Emph`/`Strong`/`Strikeout`/`Superscript`/`Subscript` and attribute spans (`{.x}` on code, `{.d}` on a div) came back as `Original(FileId(0), …)`. In a document render FileId(0) is the document, so such a span names the right file at the wrong offset, or the wrong file when the string came from `_quarto.yml`.

I haven't reproduced an end-to-end user-visible symptom. Few diagnostics key on these spans today, but source mapping (preview click-to-source, attribution) does use them.

## Plan

1. **Test first** (`readers/qmd.rs` tests): read a construct-rich fixture with `parent = Original(FileId(7), 1000, 1000 + len)`. Assert that every `Original` in the JSON writer's source-info pool is in FileId(7), excluding the zero-length `empty_source_info()` placeholder. Also check exact offsets for one `Emph` and one class span via `resolve_byte_range`.
2. **Mechanical:** `SourceInfo::from_range(context.current_file_id(), r)` → `range_to_source_info_with_context(&r, context)` at every site.
3. **Special sites:** the task-marker and `file_id` locals in `treesitter.rs`, and the three `extract_quoted_text(…, context.current_file_id(), …)` calls. Thread `provenance_builder_with_context` or a parent-aware equivalent through them instead of a bare `FileId`.
4. **Placeholder decision:** `empty_source_info()` (zero-length `Original(FileId(0), 0, 0)`, ~17 sites) stays as it is. It marks "no source", not a position in the string, and changing it would touch every consumer that recognizes it. The test excludes exactly that shape.
5. Verify: `cargo nextest run -p pampa`, then `cargo xtask verify --skip-hub-build`.
