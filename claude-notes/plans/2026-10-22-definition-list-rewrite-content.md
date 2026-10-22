---
title: 'The definition-list and list-table rewrites must process their content (bd-definition-list-rewrite-content-baxb5j8e, GH #895)'
date: 2026-10-22
description: 'desugar_div rewrites a .definition-list / .list-table div without visiting what it produced, so nothing inside the new DefinitionList or Table is desugared or post-processed. Assessment, why the issue''s one-line fix is unsafe in the reader, and a plan that keeps the top-down order without re-visiting the new node.'
status: done  # merged to main in PR #897 (2026-10-22); GH #895 closed
braid:
  strand: bd-definition-list-rewrite-content-baxb5j8e
---

**Strand:** bd-definition-list-rewrite-content-baxb5j8e (bug, P1). **Issue:** [quarto-dev/q2#895](https://github.com/quarto-dev/q2/issues/895).

**Related strands:**

- bd-9fy9p4zl (open, P1, 2026-09-24): "list-table / definition-list cell content is never postprocessed". This is the reader half of the same bug, found while documenting editorial marks. It already flagged the hazard described in [Why not just `recurse = true`](#why-not-just-recurse-true). This plan fixes it, so it should close together with this strand.
- bd-7gqr148v (#846): introduced `pampa::pandoc::desugar` and the structural desugaring contract this bug violates.
- bd-byr1oonq (new, discovered here): the qmd writer writes `DefinitionList` in a syntax q2's reader cannot parse. That bug is pre-existing and separate; see [Writer round-trip](#writer-round-trip).

**Base:** `main` @ `d3ed54fe4`. **Branch:** `braid/bd-definition-list-rewrite-content-baxb5j8e-pampa-definition-list-list` (in-place topic branch via `cargo xtask switch-task`).

**Follow-up:** bd-byr1oonq (qmd writer round-trip) is the next task, so this fix deliberately makes no writer changes and its tests make no round-trip assertions.

## Assessment

The diagnosis in the issue is correct. `desugar_div` (`crates/pampa/src/pandoc/desugar.rs:73`) returns `FilterResult(vec![…], false)` for both rewrites. `topdown_traverse_blocks` visits a node before its children, and `false` stops it from descending into the replacement. Nothing inside the new `Table` or `DefinitionList` is ever visited, in either of the two places that share `desugar_div`:

1. **The reader's postprocess filter** (`.with_div(…)`, `treesitter_utils/postprocess.rs:1131`). Here, *every* reader handler is skipped inside the result, not only the desugaring set.
2. **`desugar_blocks` / `desugar_document`**, the non-reader path (lmd cell results, `pandoc.read(…, "json")`, `--from json`, document import).

The impact is broader than the issue lists. I reproduced each of these on `main` with `pampa x.qmd -t native`:

| Input inside a definition / list-table cell | Today | Severity |
|---|---|---|
| nested `.definition-list` / `.list-table` div | stays a `Div` | wrong output |
| loose `![alt](img.png)` paragraph | `Para [Image]`, no `Figure` | wrong output |
| ` ```{{python}} ` fence | no Q-2-50 warning | missing diagnostic |
| footnote reference `[^1]` | **Q-3-31 "Unprocessed note reference" error** | error |
| `*emph*{.x}` / `[x]{.y}` inline attrs | **Q-3-32 standalone attribute error** | error |
| `## Header` | no auto id | wrong output |
| `[++ x]` editorial marks (from bd-9fy9p4zl) | Q-3-33 | error |
| a cell whose text is `: b` (parsed as a `CaptionBlock`) | **panic**: "CaptionBlock found in filter - should have been processed during postprocessing" | crash |

The last row is the worst. This input crashes `pampa` today:

```markdown
::: {.list-table}
* - a
* - : b
:::
```

The same `* - : b` in a plain bullet list produces a fallback warning and `Para [Str ": b"]`. The difference is that the list-table's cell content is never handed to the `CaptionBlock`-handling `.with_blocks` handler.

The module docs for `desugar_div` say the non-recursion is deliberate ("Neither rewrite recurses into its result, matching the reader"). That conflicts with the contract in `dev-docs/syntax-notes.md` § Structural desugaring contract ("All three apply at any depth"), and with pandoc's behavior. The current tests miss the conflict because `the_set_applies_at_any_depth` only goes through a blockquote and a plain wrapper div, never through a rewritten div.

## Why not just `recurse = true` {#why-not-just-recurse-true}

The issue suggests returning `FilterResult(…, true)`. I tried this locally (and reverted it). It fixes every row of the table above, in both paths. It also introduces a regression in the reader.

`recurse = true` does more than descend into the replacement. `blocks_apply_and_maybe_recurse!` (`filters.rs:514`) passes the new content to `topdown_traverse_blocks`, which runs the filter **on the new nodes themselves** first, and then on their children. In the reader, that means the new `Table` passes through `.with_table`. That handler is a pipe-table heuristic: if the caption is empty and the last row has a single cell whose `Plain` starts with `:`, it takes that row and makes it the caption. List-tables must not be subject to it. bd-9fy9p4zl pointed this out in September, and the experiment confirms it:

```markdown
::: {.list-table}
* - a
* - \: b
:::
```

- Today: a two-row table whose last cell is `Plain [Str ":", Space, Str "b"]`.
- With `recurse = true`: a one-row table captioned "b". The row is lost.

The same thing happens for any single-column list-table whose last cell starts with `:` (for example `:smile:`). The heuristic also returns `recurse = false` after it fires, so the cells of that table would go back to being unprocessed.

In the `desugar_blocks` path, `recurse = true` is harmless, because `desugar_filter` has only div and paragraph handlers. But because one function serves both paths, the fix has to be correct for the reader as well.

The issue's other two arguments still hold. Termination: the result is not a `Div`, so `desugar_div` cannot fire on it again. No double processing: the traversal is top-down, so the children have not been visited yet. The only missing piece is that the replacement node must not be re-visited.

## The fix: replace the node, then descend into its children

The traversal already has a way to "descend into the children of this value without re-applying the filter to it": returning `Unchanged(value)`. `blocks_apply_and_maybe_recurse!` handles that with `value.filter_structure(…)`, which is `traverse_block_structure`. The codebase already relies on this idiom: `with_cite` in postprocess mutates the node and returns `Unchanged`, and so does pass 2 of `utils/autoid.rs`, whose comment explains exactly why `FilterResult(_, true)` would be wrong there.

A typed `with_div` handler can only return `Unchanged(Div)`, so it cannot change the node's type. The generic `filter.block` handler (type `Block`) can. Its `Unchanged(Block::Table(…))` is traversed as a `Table`: caption, head, bodies and foot. Typed handlers take precedence over the generic one (`handle_block_filter!`, `filters.rs:197`). With no `with_div`, every `Div` goes to the generic handler.

Concretely:

1. **`desugar_div` returns a `Block`** rather than a `FilterReturn`: the `Table` or `DefinitionList` for valid sugar, and otherwise the `Block::Div` unchanged (with Q-2-35 still reported for an invalid list-table). Rename it if that reads better (for example `desugar_div_block`). Rewrite the doc comment: the rewrite happens top-down, *before* the div's content is visited, and the caller descends into the result's children without re-visiting the result.
2. **Add a `with_block` builder** to `Filter` (`filters.rs`). The `block` field is `pub`, but there is no builder for it today; `define_filter_with_methods!` covers the typed fields. This is a few lines.
3. **Reader** (`postprocess.rs:1131`): replace `.with_div(…)` with
   ```rust
   .with_block(|block, _ctx| match block {
       Block::Div(div) => Unchanged(desugar_div(div, &mut error_collector_ref.borrow_mut())),
       other => Unchanged(other),
   })
   ```
   This adds an extra closure call for each block type that has no typed handler (blockquote, ordered list, figure, …). It always returns `Unchanged`, which is what the traversal does when no handler is present. The cost is negligible, but `topdown_traverse_alloc_budget` will confirm it.
4. **`desugar_filter`** (`desugar.rs:153`): the same `with_block` shape. The `with_paragraph` → `Figure` handler keeps `FilterResult(…, true)`. Re-visiting a `Figure` is harmless there, and the reader does the same.
5. **Docs:** remove "Neither rewrite recurses into its result" from `desugar_div`; in the module docs, state that the set applies inside its own output. In `dev-docs/syntax-notes.md`, add "including inside a rewritten list-table / definition-list" to the "any depth" sentence.

Why this is better than the alternatives:

- **New `FilterReturn` variant** (for example `Descend(U)`: "replace, then traverse the children only"). It is more explicit, but it changes the public filter API that quarto-core also uses, and it touches about 7 match sites in `filters.rs`. The `Unchanged` idiom already means exactly this. Fall back to this option if reviewers find `Unchanged(Block::Table(…))` too surprising.
- **Move the div rewrite into a pass after the main postprocess traversal** (bd-9fy9p4zl option (a)). The children would be processed first (the div stays a `Div` during the main pass), so they would be bottom-up relative to the rewrite. That changes behavior: a loose image-only term paragraph would become a `Figure` *before* `is_valid_definition_list_div` checks that the term is `Plain`/`Para`, so the list would silently stay a div. It also costs an extra whole-document traversal in the reader. Keeping the top-down order avoids both problems.
- **`recurse = true` plus a guard in `.with_table`.** A list-table-derived `Table` carries no marker to distinguish it from a pipe table, so the guard would rely on source-span tricks. It would also still re-visit the new node with every other present and future `Table`/`DefinitionList` handler.

## Behavior changes to expect (all toward pandoc)

- Headers inside definitions and cells get auto ids. They take part in dedup in document order, so a later `## Foo` can become `foo-1` where it was `foo` before.
- Citations inside definitions and cells are counted by `citation_counter`, so the `note_num` of later citations shifts.
- New diagnostics appear inside these structures: Q-2-50, Q-2-35 for an invalid nested list-table, the orphan-caption fallback warning, and anything else the reader reports.
- The panic above becomes the same warning plus `Para` that a plain list produces.
- Snapshot tests under `crates/pampa/tests/snapshots/native/definition-list-*` and the list-table ones may change if any of them happen to contain affected content. I expect them not to; any diff gets reviewed rather than blindly accepted.

## Writer round-trip {#writer-round-trip}

The issue asks to check that the qmd writer round-trips nested definition-list / list-table content. It does not, and that has nothing to do with nesting. `write_definitionlist` (`writers/qmd.rs:1063`) emits pandoc's `Term` / `:   Def.` syntax, but q2's reader only accepts the `::: {.definition-list}` div form. Even a flat, one-level list reads back as `Para [Str "Term"], Para [Str ":   Def."]`. A multi-block definition indents its later blocks four spaces, which becomes an indented-code error on re-read. A pipe table nested inside a definition is also written unindented. I filed this separately as **bd-byr1oonq** and propose to keep it out of this fix: this strand is about the reader and desugar, while that one is about the writer (likely fix: emit the div form, the way tables fall back to `.list-table`). Open question for you: should bd-byr1oonq be fixed first or together with this, if hub-client edits inside a definition list reach that writer through the incremental writer?

## Tests

TDD: write these first and watch them fail on `main`.

**`desugar.rs` unit tests** (the `desugar_blocks` path):

- a definition-list div inside a definition becomes a nested `DefinitionList`;
- a list-table div inside a definition becomes a `Table`;
- a definition-list div inside a list-table cell becomes a `DefinitionList`;
- `Para [Image(alt)]` in a definition becomes a `Figure`;
- an invalid list-table nested in a valid one warns Q-2-35 exactly once;
- extend `the_pass_is_idempotent_on_valid_output` with a nested case.

**Reader integration tests** (`crates/pampa/tests/integration/`, in an existing file such as `test_warnings.rs`, or a new module registered in `main.rs` per `.claude/rules/integration-tests.md`):

- the three repros from the issue, checked against the AST;
- Q-2-50 inside a definition;
- `[^1]` and `*x*{.y}` inside a definition and inside a cell: no Q-3-31 / Q-3-32 (the bd-9fy9p4zl repros, including `[++ x]`);
- the regression guard: a single-column list-table whose last cell is `\: b` keeps both rows and has no caption;
- the former panic, `* - : b` in a list-table: no panic, the fallback warning, and `Para`;
- a header inside a definition gets an auto id.

**Native snapshots:** possibly one `definition-list-nested.qmd` covering the issue's repro.

**Verification:** run `cargo nextest run -p pampa`, then the full `cargo xtask verify` (the lmd and `pandoc.read` paths run through quarto-core and the hub-client WASM). Because this plan file is new, also run `q2 render --strict claude-notes`.

## Work items

- [x] Write the failing tests above (unit + integration). Unit tests are in `desugar.rs`; reader tests are in `tests/integration/test_rewritten_div_content.rs`. All failed on `main` except the caption-heuristic guard, which passes before and after by design. Deviation: `*emph*{.x}` produces a standalone `Attr` even at top level (not this bug), so the lowering tests use `$x$ {#eq-a}` instead. Note *definitions* are omitted because the native writer cannot write them.
- [x] Add `Filter::with_block`.
- [x] Change `desugar_div` to return `Block`; switch the reader and `desugar_filter` to `with_block` + `Unchanged`.
- [x] Update the doc comments (`desugar_div`, module docs) and `dev-docs/syntax-notes.md`.
- [x] Review any snapshot diffs (none: `cargo nextest run -p pampa` passes all 5197 tests unchanged); run `cargo xtask verify`. Everything is green except 3 typst smoke-all docs (`url-image-mediabag`, `online-logo`, `crossref-grand-finale`), which fail because the local Homebrew pandoc 3.12.1 lacks `+http` and cannot fetch remote images. That is environmental and unrelated to this change; CI installs the official release.
- [x] Close bd-9fy9p4zl together with this strand (both closed 2026-10-22; PR #897 closed #895).
- [ ] Comment on #895 with the fix and the caption-heuristic caveat behind the deviation from the suggested one-liner.
