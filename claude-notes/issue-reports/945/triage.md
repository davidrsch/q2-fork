# Issue #945 — project-level `filters:` paths resolve against each document's directory

- **GitHub**: https://github.com/quarto-dev/q2/issues/945
- **Reporter**: @cscheid (Carlos Scheidegger), 2026-10-27, found while reproducing @atheriel\'s Connect-docs report
- **Triage date**: 2026-10-28
- **Branch**: `issue-945`, a topic branch in the main checkout (no worktree, by request), based on `main` @ `49cb226de` (0.37.0)
- **Braid strand**: bd-nqq41zgx
- **Scope**: the `filters` key as reported, **plus** the class survey the
  user asked for in the same request: every place Quarto 2 turns a
  YAML-authored relative path into a filesystem path, and a plan to converge
  them. The plan lives in
  `claude-notes/plans/2026-10-28-config-path-late-binding.md`; this document
  is the bug record.

## Summary

Reproduced verbatim at HEAD in every spelling the issue lists and one it
does not (leading `/`). A relative entry in a project-level `filters:` list
is joined onto the consuming document's directory, so every page outside the
project root fails with `Failed to read filter '<subdir>/<filter>'`. The
leading-`/` spelling the path-resolution contract guarantees as
project-root-relative fails for *every* page, because `filter_resolve.rs`
treats it as OS-absolute. Front-matter declarations work, since there the
document directory *is* the declaring directory.

This is a known, deferred instance of the config-path resolution class:
`filters` is a row in the VIOLATIONS table of
`claude-notes/designs/path-resolution-model.md`, scoped out of bd-oejuizi9
(theme/include fix, 2026-08-19) into bd-rdcvjy2s and bd-hjv5o. Real bug;
the local fix is small; the class fix is the plan.

## Reproduction

Fixtures under `claude-notes/issue-reports/945/` (each is a two-page
website: `index.qmd` at the root, `sub/page.qmd`, `hello.lua` at the root
printing `hello from filter`). Run `q2 render` inside each. Output below is
from `target/debug/q2` rebuilt at `49cb226de` on 2026-10-28; the
`Search index is missing …` info lines are unrelated and elided.

| Fixture | Declaration | Observed | Expected |
|---|---|---|---|
| `repro/` | `filters: [hello.lua]` in `_quarto.yml` | root page runs the filter; `sub/page.qmd`: `Failed to read filter '…/repro/sub/hello.lua'`; `Rendered 1 of 2 files … 1 error` | both pages run it |
| `exp-format-filters/` | `format.html.filters: [./hello.lua]` | same, path `…/sub/./hello.lua` | both pages run it |
| `exp-leading-slash/` | `filters: [/hello.lua]` | both pages: `Failed to read filter '/hello.lua'`; `Rendered 0 of 2 files … 2 errors` | both pages run it (`/` = project root, contract rule 2) |
| `exp-frontmatter/` | `filters: [../hello.lua]` in `sub/page.qmd` front matter | `Rendered 2 of 2 files`, filter runs on `sub/page.qmd` | works (regression guard) |

Quarto 1 on the `repro/` layout runs the filter on both pages: it rewrites
project-config strings that exist relative to the project dir into
input-relative form before merging (`toInputRelativePaths`,
`external-sources/quarto-cli/src/project/project-shared.ts:138`, applied to
`_quarto.yml` at `render-contexts.ts:748` and to each `_metadata.yml` at
`project-shared.ts:345`), then probes CWD (= input dir) and falls through to
extension lookup. Q1 does *not* anchor leading `/` at the project root for
this key; that is a Q2 contract decision.

## Localization

All in `crates/quarto-core/src/filter_resolve.rs` (line numbers at HEAD):

- `resolve_filters` (`:137`): takes `document_dir: &Path` and nothing about
  provenance. Its two "is this a file or an extension name?" probes (`:180`,
  `:207`) call `runtime.path_exists(&document_dir.join(s))`.
- `resolve_filter_path` (`:322`): `if path.is_absolute() { path } else { document_dir.join(path) }`.
  `Path::is_absolute` makes `/hello.lua` OS-absolute (and on Windows would
  make it *relative*, since it has no drive prefix; `quarto_util::is_rooted`
  is the right predicate).
- Callers: `stage/stages/user_filters.rs:134-142` and
  `stage/stages/pandoc_write.rs:929`, both passing
  `ctx.document.input.parent()`. Both have `doc.source_context` and
  `ctx.project.dir` in scope, so the provenance a fix needs is available
  without new plumbing.
- The merged `filters` items keep their own `source_info` (the concat merge
  of arrays keeps each item's `ConfigValue`), and the merge stage registers
  `_quarto.yml` and every `_metadata.yml` in `doc.source_context`
  (`metadata_merge.rs:214-266`), so each item's declaring file is
  recoverable today; nothing reads it.
- The model to copy, either way: `project/format_paths.rs` (merge-time
  marking against the declaring layer, `candidate_path` for leading `/`) or
  `transforms/navigation_href.rs:779` `resolve_metadata_path` (late binding
  from `SourceInfo`). The plan recommends the second, generalized.

`template-partials` (`apply_template.rs:415`, `pandoc_write.rs:578`) and
`shortcodes` (`shortcode_resolve.rs:1193`) have the identical shape and are
the issue's nearest siblings; the full sibling list is in
`consumer-sweep-2026-10-28.md` beside this file.

## Open questions — resolved during triage

- *Does the `format.html.filters` spelling behave differently?* No
  (`exp-format-filters/`); `resolve_format_config` flattens it into the
  same `filters` array before resolution.
- *Does the leading-`/` workaround from the GH #455 discussion apply here?*
  No (`exp-leading-slash/`): it fails for all pages, worse than the bare
  spelling. bd-rdcvjy2s already records this for `filters`.
- *Is provenance really lost by the time `resolve_filters` runs?* Only the
  layer identity is; per-item `SourceInfo` survives and the declaring files
  are registered in the document's `SourceContext`. The contract's note that
  `_quarto.yml` values "degrade to raw" under mechanism 1 predates
  bd-x113wg9v and is stale.
- *What does Quarto 1 do, precisely?* Existence-driven rewrite at the
  declaring layer (above). Equivalent to `FORMAT_PATH_KEYS`\'s
  `ExistenceSilent` policy applied to every string.
- *Is HEAD green?* Rust: `cargo xtask verify --skip-hub-build` ran all
  17,435 nextest tests green, lint/clippy green; hub-client unit (1,741) and
  integration (229) green. The hub-client `test:wasm` step failed (26
  smoke-all, 105 parity, and 4 golden comparisons) with the signature of a
  stale WASM artifact (`--skip-hub-build` does not rebuild it; the `pkg/`
  artifact was dated 2026-10-21). After `npm run build:wasm` the same
  `npm run test:wasm` passed: 44 files, 462 tests. HEAD is green.

## Outcome / recommended next step

Filed bd-nqq41zgx. Phase 0 of the plan fixes this issue by piloting one
provenance-based resolver on `filters` (failing-first integration tests on
these four fixtures); a one-row `FORMAT_PATH_KEYS` fix is the documented
fallback. Awaiting plan review before execution.

## Verification commands used

```bash
gh issue view 945 --repo quarto-dev/q2 --json title,body,author,createdAt,labels,comments
cargo xtask verify --skip-hub-build          # pre-flight, main @ 49cb226de
git switch -c issue-945
cargo build --bin q2
cd claude-notes/issue-reports/945/repro && ../../../../target/debug/q2 render   # and the three exp-* dirs
braid show bd-oejuizi9 bd-hjv5o bd-rdcvjy2s bd-oqoozmtr bd-0x1znhgb bd-40luf359
braid create "…" -t bug -p 1 --deps related:bd-oejuizi9,…   # → bd-nqq41zgx
```

## Cross-references

- Strands: bd-nqq41zgx (this), bd-oejuizi9 (theme/include, in_progress),
  bd-hjv5o (generalization audit), bd-rdcvjy2s (leading `/` in filesystem
  keys; names `filters`), bd-0x1znhgb (carry the base, not the span),
  bd-40luf359 (`config-path-base` lint), bd-o1meelim, bd-epk8cmdl,
  bd-yghnqcfg (sibling listing cases).
- `claude-notes/designs/path-resolution-model.md` (contract; inventory to be
  corrected from the sweep).
- `claude-notes/research/2026-08-19-path-resolution-class-assessment.md`.
- `claude-notes/plans/2026-08-19-bd-oejuizi9-declaration-site-path-keys.md`
  (the last fix of this shape; its "out of scope" list names `filters`).
- `claude-notes/plans/2026-05-20-bd-qor9a-metadata-path-resolution.md`
  (mechanism 1).
- `claude-notes/plans/2026-10-28-config-path-late-binding.md` (the plan).
- `CLAUDE.md` path-resolution bullet; `crates/quarto-core/AGENTS.md`.
