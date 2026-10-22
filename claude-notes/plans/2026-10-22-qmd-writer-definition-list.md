---
title: 'The qmd writer must write DefinitionList in the syntax the qmd reader parses (bd-byr1oonq)'
date: 2026-10-22
description: 'write_definitionlist emits pandoc term / colon syntax, which q2 does not parse, so no definition list survives a qmd write; in hub-client, editing inside one silently turns it into paragraphs. Assessment, a spike that confirms the fix, the edge cases it leaves, and a plan.'
status: in-review  # D1–D3 approved 2026-10-22; PR stacked on #897
braid:
  strand: bd-byr1oonq
---

**Strand:** bd-byr1oonq (bug; discovered from bd-definition-list-rewrite-content-baxb5j8e). That strand is GH #895, PR #897, and its plan is `claude-notes/plans/2026-10-22-definition-list-rewrite-content.md`.

**Branch:** `braid/bd-byr1oonq-qmd-writer-definitionlist-written`, off `c8f14143a` (the #895 fix, not yet merged), set up via `cargo xtask switch-task`.

**Related strands:** bd-25ezfgs8 and bd-i4xrcdqx (both open). In the qmd reader, a whitespace-only blank line makes a list tight, where pandoc says loose. This interacts with the fix; see [D2](#d2).

## Assessment

`write_definitionlist` (`crates/pampa/src/writers/qmd.rs:1063`) writes pandoc's native syntax:

```
Term
:   Def.
```

q2's reader does not parse that syntax. It only accepts the `::: {.definition-list}` div form. So **no** definition list survives a qmd write, flat or nested:

- **A one-level list** reads back as `Para [Str "Term"], Para [Str ":   Def."]`.
- **A multi-block definition** indents its later blocks by four spaces, which is an "Indented code blocks are not supported" error on re-read.
- **Nested structures** (#895) inherit both problems.

This is already known. `incremental_writer_tests.rs` has two definition-list sugar round-trip tests marked `#[ignore = "…lossy: writer produces Pandoc-native syntax, reader expects div syntax"]`.

### User-facing impact: hub-client edits

The incremental writer always fully rewrites a `DefinitionList` block; it never splices into it (see the coupling table at the top of `postprocess.rs`). So **any edit inside a definition list in hub-client rewrites the whole list into the syntax above**, and the next read turns it into paragraphs. I confirmed this through the incremental-write path (`assert_roundtrip` in the incremental writer tests). Editing `def` to `def changed` produces:

```
Before.

term
:   def changed

other
:   more

After.
```

The existing hub-client test (`applyNodeEdit.wasm.test.ts`, "DefinitionList round-trip") does not catch this, because it only checks that the edited text is present. Given that this is silent data corruption in the editor, I propose raising bd-byr1oonq from P2 to **P1**.

### User-facing impact: document import

Found during verify. docx import (`finish_import`) writes the imported AST with this writer. A docx using pandoc's "Definition" / "Definition Term" styles therefore imported as qmd that does not re-read: the `writer-bugs-docx` recording was exempted for exactly this reason (`KNOWN_UNREREADABLE` in `import_read.rs`, the "I18 correction" in the document-import epic plan). With the fix, that recording re-reads as a `DefinitionList`. The exemption is removed, its `expected.qmd` is regenerated (`Q2_UPDATE_IMPORT_EXPECTED=1`; the diff is only the definition list), and both epic plans carry a resolution note.

## The fix: write the sugar the reader desugars

The reader's sugar is a plain AST shape:

```
Div(.definition-list) [ BulletList [ [Plain term, BulletList definitions], … ] ]
```

`transform_definition_list_div` turns that shape into a `DefinitionList`. The writer can do the inverse: rebuild the shape and pass it to the existing `write_div` / `write_bulletlist`. Indentation, nesting, tight versus loose output, and list-table cells then all come from code that already handles them. `write_list_table` is the precedent; it writes a `Table` back as its `.list-table` div.

### Spike result

I tried exactly this, about 30 lines; the patch is kept outside the repo. Each case below was put through read → native, compared with read → qmd → read → native:

| Case | Result |
|---|---|
| flat; two entries; several definitions; multi-block definition; loose terms; loose definitions | OK |
| inline formatting in term and definition; code block, header, ordered list or blockquote as a definition | OK |
| **nested** definition-list in a definition; list-table in a definition; definition-list in a list-table cell (the #895 shapes) | OK |
| implicit figure in a loose definition; definition list inside a blockquote; two adjacent lists; a list right after a bullet list | OK |
| empty definition (`- ` with no content) | OK |
| definition whose first `Para` is followed by a bullet list or code block; a definition list inside a bullet item | **`Para` → `Plain`**; see [D2](#d2) |

With the spike, the two ignored tests pass, and so does the edit-inside-a-definition-list probe through the incremental writer. All 5197 existing pampa tests still pass. No existing snapshot covers definition-list qmd output.

## Decisions

All three were approved on 2026-10-22. The guiding criterion, in Carlos's words: tight versus loose differences in complex definitions are *not* a gross discrepancy, because in practice they only mean a larger rewrite of the node by the incremental writer (block editor → AST → source). What is not acceptable is a definition list that fails to reach a reasonable fixpoint that is recognizably a definition list.

### Sanity check against pandoc's own syntax

How do pandoc's `Term` / `: definition` reader and q2's div reader treat blank lines in definitions? (pandoc 3.12.1, `-f markdown`.)

| Input | pandoc | q2 |
|---|---|---|
| tight single definition | `Plain` | `Plain` |
| blank line between term and definition | `Para` | `Plain` |
| two paragraphs in one definition | `Plain`, `Plain` | `Para`, `Para` |
| paragraph + code block / bullet list | `Plain` + … | `Para` + … |
| two definitions, tight or blank-separated | `Plain`, `Plain` | `Plain`, `Plain` |
| two terms, blank-separated | identical | identical |

The structure is the same in every case: terms, number of definitions, and blocks. Only `Plain` / `Para` differ, in both directions. Pandoc derives looseness from the blank line between term and `:`; q2 derives it from CommonMark item looseness inside each definition. That is not a gross discrepancy, so q2 does not try to replicate pandoc here.

### D1: definitions use the `-` marker {#d1}

As written, the spike reuses `write_bulletlist`, which chooses markers by sibling alternation, so it writes `* Term` / `  * def`. The docs and the #895 repro write `* Term` / `  - def`. Both parse the same. **Recommendation:** write `-` for the definitions, so a rewritten list looks the way authors are told to write one. This costs a small custom loop instead of a fully generic `write_div` call: write the fence, the `*` term items, and each definitions list with `ctx.bullet_marker = '-'`, still through `write_bulletlist`.

### D2: leave blank lines inside list items unindented {#d2}

The three failures in the spike table are **not** specific to definition lists. A plain bullet list fails the same way on `c8f14143a`:

```markdown
* Some text.

  ```python
  1 + 1
  ```
```

This reads as `Para` + `CodeBlock`. The writer's `BulletListContext` indents *every* continuation line, including blank ones, so it emits `"  "` on the blank line. The reader then treats that whitespace-only line differently from a truly empty one and makes the item tight, which turns the `Para` into `Plain`. Pandoc reads both spellings as `Para`, so the reader is the side that's wrong; that bug is bd-25ezfgs8 / bd-i4xrcdqx.

Options:

- **(a)** Leave it. The definition-list fix still works, and these cases wait for the reader fix.
- **(b)** Also stop writing indentation on blank lines in `BulletListContext`. CommonMark does not need blank lines inside a list item to be indented, and `write_cell_block_indented` (list-tables) already writes them empty. I tried this as well: it makes every case in the table round-trip, plain bullet lists included. It changes exactly one existing snapshot, `qmd/editorial-divs.snap`, where three `"  "` lines become empty.

**Recommendation: (b)**, as its own commit in this branch. It removes trailing whitespace from our output, which is better regardless. It doesn't close bd-25ezfgs8, because hand-written input with whitespace-only lines is still misread. I'd leave a comment on that strand.

### D3: AST shapes the div syntax cannot express {#d3}

These shapes arrive only through JSON or Lua, never from markdown:

- **Empty term** `([], defs)`. The spike writes `* ` with nothing after it, and the re-read is a **parse error**, the worst outcome. Writing the `* []` placeholder (the bullet-list writer's convention for an empty item) re-reads as term `[Span ("",[],[]) []]`. That is visually identical, but not equal. **Recommendation:** write `* []`, and make `transform_definition_list_div` strip a term that is exactly one empty, attribute-free `Span`. This mirrors the reader's existing `* []` rule for bullet items. Because `desugar_div` is shared, the stripping also applies to constructed ASTs, which is harmless.
- **Term with no definitions** `(term, [])`. There is no div spelling for it. The item has one block, so the div is left as is and re-reads as `Div(.definition-list)[BulletList[[Plain term]]]`. That loses no content and parses fine. Pandoc's markdown writer cannot express this shape either. **Recommendation:** accept it, document it in `write_definitionlist`, and add a test that pins the fallback, so it at least never becomes a parse error.

## Tests

TDD: write these first and watch them fail.

1. **Un-ignore** `sugar_roundtrip_definition_list_basic` / `_multiple_defs` and delete the "LOSSY" note above them (`incremental_writer_tests.rs:1209`).
2. **Incremental edit inside a definition list:** an `assert_roundtrip` that changes one definition (the probe above), plus one that edits a nested list.
3. **qmd → json → qmd corpus** (`tests/roundtrip_tests/qmd-json-qmd/`, glob-driven): add `definition_list_*.qmd` fixtures for the passing rows of the spike table, including the three #895 nesting shapes. With D2 (b), the three `Para` → `Plain` cases go in too.
4. **Canonical output snapshot:** add `tests/snapshots/qmd/definition-list-*.qmd` (one flat, one nested) to pin the written form, including D1's markers.
5. **Edge shapes (D3):** a writer unit test that builds the empty-term and no-definitions ASTs and checks that the output re-reads as, respectively, the same `DefinitionList` and the documented `Div` fallback, without a parse error.
6. **hub-client:** strengthen `applyNodeEdit.wasm.test.ts` § "DefinitionList round-trip" to assert that the edited output still contains `::: {.definition-list}` and no `\n:   ` line.

**Verification:** run `cargo nextest run -p pampa`, then the full `cargo xtask verify` (the hub-client WASM tests cover item 6), then `q2 render --strict claude-notes`.

## Work items

- [x] Tests 1–6 (failing first). The AST-shape tests went into `qmd_writer_pandoc_shapes.rs` (pandoc-shaped JSON → qmd → re-read), next to the qmd-shaped corpus fixtures. Deviation: a definition list whose definitions are single `Para`s separated by blank lines reads back with `Plain`s. q2's list reader does not loosen a list for blank lines between items, nested or not, and neither does pandoc's definition-list reader (row 5 above). Per the criterion above, that case is pinned as settling (`loose_single_paragraph_definitions_settle_into_tight_ones`: the AST is stable from the first re-read, the text from the second write) rather than as an exact round trip.
- [x] `write_definitionlist`: rebuild the sugar AST and write it as a fenced div, with `-` definition markers (D1). Document the D3 fallback in its doc comment.
- [x] D3: write `* []` for an empty term; strip it back in `transform_definition_list_div`.
- [x] D2 (b), as a separate commit (`ecae1e1c1`; `OrderedListContext` got the same change): leave blank lines inside list items unindented in `BulletListContext`; accept the `editorial-divs` snapshot change after reviewing it. Comment on bd-25ezfgs8.
- [x] Update the coupling table in `postprocess.rs` (definition-list write column: "Div(.definition-list)"; uses indentation boundaries: "Yes (BulletList inside div)"), and the `INCREMENTAL WRITER COUPLING` comments that mention term/definition syntax.
- [x] `cargo xtask verify`: green. The first full run also caught the docx-import fixture (see [document import](#user-facing-impact-document-import)); one later run hit the known timing flake bd-7kp7kjdq (`head_peek_timeout_closes`, 4.998 s against a 5 s floor), which passed 3/3 when re-run on its own.
- [ ] Close bd-byr1oonq; open a PR stacked on #897 (retarget it to `main` once #897 merges).
