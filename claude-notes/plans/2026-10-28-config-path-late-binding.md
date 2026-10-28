---
title: 'bd-nqq41zgx — config-path resolution: converge on provenance-based late binding (issue #945)'
date: 2026-10-28
description: 'Assessment of how Quarto 2 resolves relative paths written in YAML, why project-level `filters:` (GH #945) still fails for subdirectory pages, and a plan to replace the five coexisting mechanisms with one resolver that binds the base directory late, from each value''s SourceInfo.'
status: in-progress  # assessment 2026-10-28; all review decisions taken; Phase 0 executing
braid:
  strand: bd-nqq41zgx
  priority: P1
---

**Braid:** bd-nqq41zgx (related: bd-oejuizi9, bd-hjv5o, bd-rdcvjy2s, bd-0x1znhgb, bd-40luf359).
**Branch:** `issue-945` (topic branch in the main checkout).
**Triage record:** `claude-notes/issue-reports/945/triage.md` (repro fixtures beside it).
**Contract:** `claude-notes/designs/path-resolution-model.md` (normative since 2026-08-19).
**Prior assessment:** `claude-notes/research/2026-08-19-path-resolution-class-assessment.md`.

## 0. TL;DR

- GH #945 is a *known, deferred* instance of the config-path class: `filters`
  sits in the contract's VIOLATIONS table, scoped out of bd-oejuizi9 on
  2026-08-19 into bd-rdcvjy2s / bd-hjv5o. Nothing about it is new except that
  a user hit it.
- The codebase has **five** coexisting provenance mechanisms (§2). Three of
  them are "normalize before merge" (the strategy the original merge design
  anticipated); one is "late-bind from SourceInfo"; one is glob-specific.
- The premise that late binding is now cheap to retrofit is **correct**: since
  bd-x113wg9v / bd-fu16z22k / bd-r64mj1aa the merge stage registers
  `_quarto.yml`, every profile overlay, every `_metadata.yml` and every
  `_extension.yml` in the document's `SourceContext`, so a merged value's
  `SourceInfo` resolves to its declaring file for every YAML layer. The
  contract's caveat that `_quarto.yml` values degrade to raw is **stale**.
- Recommendation (§4): one blessed resolver, `resolve_config_path`, that takes
  a `&ConfigValue` and binds the base late (declaring-file dir from
  `SourceInfo`; project root for leading `/`; "already rebased" for
  `Path`-kind values produced by the merge-time mechanisms). Consumers call
  it instead of choosing a base; the key registry stops being load-bearing for
  *resolution* and shrinks to the existence-driven disambiguation cases.
  Merge-time marking stays as the carrier for values with no usable span.
- Phase 0 fixes #945 by piloting that resolver on `filters` (the contract's
  worst remaining violator and the one with an existence probe already in
  place). A one-row `FORMAT_PATH_KEYS` fix is the fallback if the pilot slips
  a release (§5, Phase 0 alternative).

## 1. The reported bug (#945)

Verified 2026-10-28 on `main` @ 49cb226de (0.37.0), fixtures under
`claude-notes/issue-reports/945/`:

| Fixture | Declaration | Root page | `sub/page.qmd` |
|---|---|---|---|
| `repro/` | `filters: [hello.lua]` in `_quarto.yml` | runs | **fails**: `Failed to read filter '<proj>/sub/hello.lua'` |
| `exp-format-filters/` | `format.html.filters: [./hello.lua]` | runs | **fails**: `'<proj>/sub/./hello.lua'` |
| `exp-leading-slash/` | `filters: [/hello.lua]` (contract rule 2 spelling) | **fails**: `'/hello.lua'` (OS-absolute) | **fails** |
| `exp-frontmatter/` | `filters: [../hello.lua]` in `sub/page.qmd` front matter | n/a | runs (regression guard: document-relative is right *for front matter*) |

Root cause, all in `crates/quarto-core/src/filter_resolve.rs`:

- `resolve_filter_path` (`:322`) joins every non-absolute path onto
  `document_dir`, and `Path::is_absolute` makes `/hello.lua` OS-absolute.
- The two existence probes that decide "file or extension name?" (`:180`,
  `:207`) also probe `document_dir.join(s)` only.
- `resolve_filters` is called from `UserFiltersStage` (`user_filters.rs:140`)
  and `PandocWriteStage` (`pandoc_write.rs:929`) with
  `ctx.document.input.parent()`. Both call sites hold `doc.source_context`
  and `ctx.project.dir`, so the provenance needed for the fix is in scope.
- By the time `resolve_filters` runs, `doc.ast.meta` is the flattened merge:
  the `filters` array items from `_quarto.yml` are `Scalar` strings whose
  `source_info` still points into `_quarto.yml` (concat merge keeps each
  item's own `ConfigValue`), but nothing reads that provenance.

What Quarto 1 does (`external-sources/quarto-cli/src/project/project-shared.ts:138`
`toInputRelativePaths`, called from `render-contexts.ts:748` for `_quarto.yml`
and `project-shared.ts:345` for each `_metadata.yml`): before merging, every
non-absolute string in the project / directory config whose
`join(declaringDir, value)` **exists** is rewritten to an input-relative path.
Then the filter resolver probes CWD (= input dir) and falls through to
extension lookup. So Q1\'s rule is *existence-driven marking at the declaring
layer*, applied to every string regardless of key: exactly the policy Q2\'s
`FORMAT_PATH_KEYS` implements per key (`MarkPolicy::ExistenceSilent`), just
without the key table. Q1 never anchors leading `/` at the project root for
filesystem keys; that is a Q2 contract decision (rule 2).

## 2. The class: what exists today

### 2.1 Where provenance is lost, and where it survives

The merge (`MetadataMergeStage` → `collect_metadata_layers` →
`prepare_layer` → `MergedConfig`) flattens five layer kinds (extension
contribution, project, extension format, `_metadata.yml` chain, document,
runtime) into one `ConfigValue`. Layer identity is gone after the merge, **but
each leaf keeps its own `source_info`**, and `metadata_merge.rs:214-266`
registers every YAML layer file in both `doc.ast_context.source_context` and
`doc.source_context` under `quarto_yaml::file_id_for_filename(path)`. So for
any leaf authored in YAML, `source_info.resolve_byte_range()` →
`SourceContext::get_file(FileId)` → `SourceFile.path` → declaring directory
works today, for all of: `_quarto.yml`, `_quarto-<profile>.yml`,
`_quarto.yml.local`, each `_metadata.yml`, each `_extension.yml`, and the
document itself (its own FileId).

Values with **no** usable span: the runtime layer (`-M`, preview overlay;
`json_to_config_value` sets `SourceInfo::generated`), Lua-set metadata,
programmatic config (`By::is_programmatic_sentinel`), and the
book `pre_render` copies (`book.*` → `website.*`, which keep the original
span, so they are fine). Values whose span is **not** their base: extension
fragments rebased by `rebase_fragment_paths` keep their `_extension.yml` span
while the string is already project-root-relative (bd-74ktvzv7 → bd-0x1znhgb).

### 2.2 The five mechanisms

| # | Mechanism | Where | When | Needs a key registry? | Output stored as | Covers |
|---|---|---|---|---|---|---|
| 1 | `resolve_metadata_path` | `transforms/navigation_href.rs:779` | Generate time (late) | no: caller knows the key | project-root-relative string | nav hrefs/logos |
| 2 | `!path` → `ConfigValueKind::Path` + `adjust_paths_to_document_dir` | `project/mod.rs:268`, `metadata_merge.rs` | merge time | only via explicit tag | **doc-relative** `Path` (absolute if outside project) | explicit `!path`; extension-marked values |
| 3 | `FORMAT_PATH_KEYS` + `mark_format_path_values` | `project/format_paths.rs:115` | merge time, per layer | **yes** (12 rows, 5 `KeyForms`, 3 `MarkPolicy`s) | doc-relative `Path` | css, theme, include-\*, epub-\*, reference-doc, template(?), bibliography, csl, components |
| 4 | `BaseDirContext` | `glob/provenance.rs:47` | consumer (late) | no | n/a (globs) | listing `contents`, `resources:` |
| 5 | Extension read-time tables (`PATH_VALUED_KEYS`, `FORMAT_ASSET_PATTERNS`, `FRAGMENT_PATH_PATTERNS`, website patterns) | `extension/read.rs:254`, `extension/paths.rs:65`, `project/mod.rs:795` | extension read | **yes** (three tables) | `Path` rebased to ext dir, or plain project-root string | extension-contributed values |

Plus the unconverted consumers (bare `doc_dir.join` on a config string):
`filter_resolve.rs` (**#945**), `apply_template.rs:199,415`
(`template`, `template-partials`), `transforms/title_banner.rs:200`,
`project_resources.rs:721,835` (engine/filter-declared resources),
`listing_render.rs` `load_custom_template` leading-`/` (bd-o1meelim). The
fresh consumer sweep (§2.4) is the authoritative list.

Note on `template`: `FORMAT_PATH_KEYS` has a `template` row
(`ExistenceSilent`, added for docx/pptx forwarding), so a project-level
`template:` that exists is already marked doc-relative and
`apply_template.rs`\'s `document_dir.join` then happens to be right; a missing
file or `template-partials` are not. The contract's VIOLATIONS row is half
stale in the other direction.

### 2.3 Costs the current design has already incurred

- **Three table-driven fixes for the same bug** (css 2026-08-14, theme +
  include-\* 2026-08-19, bibliography/csl 2026-09-24), each scoped to the key a
  report named; `filters`/`template-partials` deferred each time. The class
  survives every fix because the registry is opt-in per key.
- **Doc-relative storage changes the value per consuming document.**
  bd-79c4do6g: the SCSS cache key hashed the doc-relative theme path → 97 %
  miss rate, 78 % of `q2 render` time on the Connect docs. `q2 get-config`
  prints a different string per document for the same declaration. Any
  consumer that hashes, dedups or emits the stored form is exposed.
- **Span ≠ base** for rebased extension values (bd-74ktvzv7, patched by a
  filename check in `resolve_metadata_path`; general fix deferred to
  bd-0x1znhgb).
- **Windows `MAX_PATH`** from `..`-chain rebases (bd-s0jupmmv, bd-9z2258af):
  consequences of storing a *relative* rebased form rather than resolving at
  the consumer.
- Silence: a wrong base is only visible when the file happens to be missing;
  a file that exists in both places picks the wrong one with no diagnostic.

### 2.4 Consumer sweep (2026-10-28)

Full table (file:line, key, space, base, leading-`/` handling, mechanism):
`claude-notes/issue-reports/945/consumer-sweep-2026-10-28.md`. Headlines:

- **Still violating** (already in the contract table): `filters` (#945),
  `template-partials` (HTML), `title-block-banner`. `template` is half
  fixed by its registry row (existing file right, missing file still
  doc-joined). Engine-reported resources are runtime output, not YAML:
  out of this class.
- **Twelve sites the contract table does not list**, all the same shape
  (`doc_dir.join(as_plain_text())`, or a per-site leading-`/` rule):
  `template-partials` (Typst leg), `shortcodes`, `highlight-style` /
  `syntax-highlighting` theme files (pandoc and Typst legs), `language:`
  file (doc dir *then* project root guess), `font-paths`, `listing.template`
  (bd-o1meelim), revealjs `logo` (emitted raw), `brand:` read from
  **merged** metadata against the project dir at four call sites (the
  contract calls it conforming), `book.chapters` leading `/`, front-matter
  `image` / `listing-item.image` with a hand-written external-prefix list
  (bd-epk8cmdl), and mechanism 2 itself (`adjust_paths_recursive` uses a
  `http://`/`https://` prefix list and never anchors `!path /x`).
- **Conforming**: the 13 registry keys, navigation (mechanism 1), globs
  (mechanism 4), extension tables (mechanism 5), and the
  `_quarto.yml`-only keys (favicon, pre/post-render, cover image).
- **Unclear**: feed images and stylesheet, Typst `logo.path`,
  `project.output-dir` (does rule 2 apply to an output location?),
  `format-resources` (marked, never consumed).

The count matters for the strategy choice: with strategy A each of the
twelve is a registry row *plus* a `KeyForms` variant where the shape is
new; with strategy B each is a one-call swap at the consumer.

## 3. Two strategies, evaluated

### 3.1 Strategy A: identify path strings, normalize, then merge

The original design intent: know which strings are paths (schema or `!path`),
rewrite each layer's paths against its known base *before* the merge, so the
merged tree is base-free. Mechanisms 2, 3 and 5 are this strategy.

- Works for every layer whose base is known at merge time, including the
  runtime layer and `_quarto.yml` with no SourceContext lookup.
- Consumers stay as written (`doc_dir.join`), which is why each table fix was
  small.
- **Requires the registry**: the merge does not know which strings are paths.
  There is no YAML schema in Q2 yet, so the registry is a hand table that
  must enumerate every key *and every value shape* (`KeyForms`: scalar/array,
  `{light,dark}` maps, smart-include `{file:}`, `components[].src`, filter
  `{path:}` maps, listing records, `about.image`, …). Each new shape is new
  code in the marker, not a row.
- Namespaces that mix names and paths (`theme: cosmo` vs `theme: x.scss`,
  `filters: [myext]` vs `filters: [x.lua]`) force existence-driven heuristics
  at merge time, probing the filesystem once per key per layer per document.
- Must choose a stored form. Doc-relative (today) costs §2.3. Project-root
  pivot would be better but still rewrites what the author wrote.
- `!path` as the explicit escape hatch is fine but nobody writes it; the
  table is what actually runs.

### 3.2 Strategy B: keep the raw string, bind the base late from SourceInfo

Mechanism 1, generalized. The merged tree keeps the authored string; a
consumer that knows it is reading a path asks one resolver for the resolved
form, and the resolver derives the base from the value's own provenance.

- **No registry for resolution.** The consumer always knows the key's
  semantics; the merge never has to. The only residual table role is
  disambiguation for name/path namespaces, and the consumer handles that
  anyway (probe the resolved candidate, else name lookup: what
  `resolve_filters` does today with the wrong base).
- Value stays as authored: stable for caching and `get-config`, round-trips
  through the qmd writer and serialization unchanged, and diagnostics anchor
  on the declaration for free.
- Rule 2 (leading `/`) lives in exactly one function.
- **Requires every consumer to call the resolver**: enforcement is a lint
  (bd-40luf359), not the type system. Mitigated by making the resolver the
  *convenient* path and the raw accessor the awkward one (§4.3).
- Needs `&SourceContext` + project root at the consumer. Stages have both;
  transforms get `RenderContext.source_context: Option<&SourceContext>`
  (bd-qor9a); crates below quarto-core (`quarto-sass`, pampa's citeproc)
  receive a pre-resolved path from quarto-core, as bibliography already does.
- Falls back when there is no span (runtime layer, programmatic values):
  must pick a default base. Today's behaviour is document dir; keep it.
- Needs the span≠base fix for rebased extension values (bd-0x1znhgb) or a
  convention that makes them self-describing (§4.4).
- `FileId` lookup is by hash of the path *spelling*; the merge stage already
  guarantees the spelling matches (bd-x113wg9v), and a miss degrades to the
  fallback, never to a panic.

### 3.3 Verdict

B is the convergence target. The decisive points: the registry A depends on
is the schema we do not have, while the provenance B depends on is already
in every merged leaf and every layer file is already registered; and B does
not rewrite stored values, which removes the §2.3 cost class rather than
managing it. A is not discarded: its merge-time marking is the right carrier
for the two cases B cannot see (explicit `!path`, and extension values whose
base is not their span), and the resolver treats a `Path`-kind value as
"base already applied". The user's "normalize then merge" framing and the
"late binding" framing therefore meet in one accessor rather than competing.

## 4. Design sketch (for iteration)

### 4.1 The resolver

New module `crates/quarto-core/src/config_paths.rs` (name open):

```rust
pub struct PathCtx<'a> {
    pub source_context: Option<&'a SourceContext>,
    pub project_dir: &'a Path,
    pub document_dir: &'a Path,
}

pub enum ResolvedConfigPath {
    /// RFC-3986 scheme or `//host`: not a path (quarto_util::is_external_url).
    External(String),
    /// Filesystem-space answer: native absolute (or VFS-absolute) path.
    File(PathBuf),
}

/// Rule 1 + rule 2 of the contract, in one place.
pub fn resolve_config_path(value: &ConfigValue, ctx: &PathCtx) -> Option<ResolvedConfigPath>;
/// Same, for the URL-space exit: project-root-relative pivot with `/`.
pub fn resolve_config_href(value: &ConfigValue, ctx: &PathCtx) -> Option<String>;
```

Resolution order inside `resolve_config_path`:

1. `ConfigValueKind::Path(s)`: produced by mechanisms 2/3/5, already
   doc-relative (or absolute when outside the project): `document_dir ⊕ s`.
   This is the "normalize then merge" output, honoured as-is.
2. Else take the string (`as_plain_text`); `is_external_url` → `External`.
3. Leading `/` → `project_dir ⊕ rest` (rule 2; `is_rooted`, not
   `is_absolute`, for Windows).
4. Else base = directory of the declaring file: `source_info` →
   `resolve_byte_range` → `source_context.get_file` → `parent()`.
   Programmatic sentinel / `Concat` / unregistered FileId / no context →
   fallback base = `document_dir` (today's behaviour, now explicit and
   logged at trace level).
5. `normalize_lexically` the join; never canonicalize (file may not exist).

`resolve_metadata_path` becomes a wrapper over the same core (or its callers
migrate), and the `_extension.yml` filename special case moves into step 4
pending §4.4.

### 4.2 What the consumer does

```rust
// before (filter_resolve.rs:322)
FilterSpec::Lua(document_dir.join(path))
// after
let resolved = resolve_config_path(item, &ctx)?;   // base chosen by provenance
```

For name/path namespaces (`filters`, `theme`): probe `resolved` for
existence; on miss, fall through to the name lookup exactly as now. The
existence probe moves from "doc dir" to "declaring dir", which is the whole
fix for #945 and GH #455\'s sibling keys.

### 4.3 Making the right thing the easy thing

Add `ConfigValue::as_path(&self, ctx) -> Option<ResolvedConfigPath>` (a
thin call into the resolver) so a consumer reads a path key the same way it
reads a string key today. The lint (bd-40luf359, `config-path-base`) then
flags `.join(` / `Path::new(` / `file_read*(` whose argument derives from
`as_plain_text()`/`as_str()` outside the resolver module and
`metadata_merge.rs`, same shape as `metadata-as-str`.

### 4.4 The span ≠ base cases (bd-0x1znhgb)

Two options; pick one in review:

- **(a)** Values rewritten to project-root form by `rebase_fragment_paths`
  (website patterns, which deliberately stay `Scalar`) are rewritten to the
  **leading-`/` spelling** (`/_extensions/x/logo.svg`). Rule 2 then makes
  them self-describing; no provenance consulted, and the
  `_extension.yml` filename check in `resolve_metadata_path` goes away.
  Consumers that "read the value as project-root-relative and relativize per
  page themselves" (`website_config.rs`, `website_post_render.rs`) must
  accept the leading `/`; `resolve_root_relative_resource_href` already
  does.
- **(b)** Carry the base on the value: `ConfigValueKind::Path { raw, base: PathBase }`
  with `PathBase::{ProjectRoot, DocumentDir, Dir(PathBuf)}`. Correct and
  explicit, but a `quarto-pandoc-types` type change touching serialization,
  the Lua marshaller, WASM and every `Path(_)` match arm.

Recommendation: (a). It uses a rule the contract already has, and keeps the
type stable.

### 4.5 What happens to mechanism 3

Not removed in this work. Once the migrated consumers all go through the
resolver, each `FORMAT_PATH_KEYS` row is only still needed for a key whose
consumer lives *below* quarto-core and receives a string (theme → quarto-sass,
bibliography → pampa). Decide per row later, measured: if the rebase is what
breaks a cache key, that row goes; if a consumer cannot take a `PathCtx`,
quarto-core resolves before handing over (the bibliography pattern) and the
row still goes. Deleting a row is a one-line change once the consumer is
migrated; adding one for #945 today would be a line we expect to delete.

### 4.6 The stored form of a marked value (open: see §6 Q4)

Today every merge-time mechanism (2, 3, 5) stores a marked value as a
`ConfigValueKind::Path` holding a **document-relative** string (absolute
only when the file lies outside the project, bd-9z2258af). That string is a
contract with its consumers, all of which join it onto the consuming
document's directory without looking at the key or the provenance:

| Consumer | Key(s) | Join today |
|---|---|---|
| `stage/stages/include_resolve.rs:559-570` | `include-*` | `doc_dir.join` |
| `transforms/format_css.rs:113-121` | `css` | `doc_dir.join`, then the `<link href>` is derived from the resolved source |
| `quarto-sass/src/themes.rs:463` (`ThemeContext::resolve_path`), `compile_theme_css.rs:462` | `theme` | `document_dir.join` (quarto-sass has no project context) |
| `pandoc_filters/format_defaults.rs:216-256` | `reference-doc`, `template` | `doc_dir.join` |
| `pandoc_write.rs:153,578,1262,1700` | `epub-*`, Typst `template-partials`, Typst `bibliography`/`csl`, Typst `template` | `doc_dir.join` |
| `pampa/src/citeproc_filter.rs:204` | `bibliography`, `csl` | base dir passed in by quarto-core (= document dir) |
| `transforms/components_resolve.rs:146`, `components/config.rs:222` | `components[].src` | `doc_dir.join` |
| `transforms/shortcode_resolve.rs:1193`, `apply_template.rs:415` | extension `shortcodes`, `template-partials` | `doc_dir.join` |
| Lua filters (`pampa/src/lua/config_value.rs:167`) | any marked key | user code sees the stored string as a `Path` special |
| smoke test `smoke-all/metadata/dir-metadata-paths` | `css` | asserts `href="../../shared/styles.css"` |

Switching the stored form to the project-root pivot (written with a leading
`/`, the same self-describing spelling decided for extension values in
§4.4) therefore means: **every row above must stop joining onto the document
dir in the same change**, either by calling the resolver (step 1 then reads
"pivot form" instead of "doc-relative") or, for crates below quarto-core,
by receiving an already-resolved path. Nothing else is coupled: the marking
code changes in one place each (`mark_entry`, `adjust_paths_recursive`), the
cache-key class (bd-79c4do6g) disappears because the stored value no longer
varies per document, `get-config` prints one answer per declaration, and
`..` chains stop existing (bd-s0jupmmv).

Two user-visible consequences to weigh:

- A Lua filter that reads a marked key (`meta["include-in-header"]`) sees
  `/custom-header.html` instead of `../custom-header.html`. Quarto 1 shows
  filters the *input-relative* form (`toInputRelativePaths`), so the
  doc-relative stored form is the Q1-compatible one for Lua. Mitigation:
  `quarto.project.*` / a `quarto.utils.resolve_path` (bd-cic0dfdp is the
  URL-space cousin), or convert at the Lua boundary.
- Golden fixtures that embed the stored form (the smoke test above, the
  hub-client pandoc-request goldens if the request carries metadata) need
  regenerating; that is bookkeeping, not risk.

Proposed sequencing if we switch: not in Phase 0. Phase 1 first routes
every row above through the resolver with the stored form unchanged
(behaviour-preserving, each row a one-call swap). Phase 1b then flips the
stored form in the two marking sites and the resolver's step 1, in one
commit, with the full suite as the guard. That gets the uniform form within
the same strand sequence without coupling it to the `filters` pilot.

## 5. Phased plan

### Phase 0: #945 pilot (this strand, small)

- [x] 0.1 Failing-first integration test, `crates/quarto-core/tests/integration/filter_paths.rs`
      (the `render_project` harness from `format_path_keys.rs`): project
      `filters: [hello.lua]` reaches root + subdir pages; `format.html.filters`
      spelling; leading-`/` spelling; front-matter `../hello.lua` regression
      guard; `_metadata.yml`-declared filter resolves against that
      directory; extension-name entry still resolves (name lookup after the
      probe miss); explicit `!path` entry unchanged.
- [x] 0.2 `config_paths.rs` with `resolve_config_path` (steps 1–5) and unit
      tests per step (Path-kind pass-through; external; leading `/`;
      `_quarto.yml` span; `_metadata.yml` span; document span; programmatic
      sentinel fallback; unregistered FileId fallback; Windows spelling via
      the cross-target clippy check).
- [x] 0.3 `resolve_filters` takes a `PathCtx` (both call sites have the
      inputs); both existence probes and `resolve_filter_path` go through
      the resolver. Unit tests in `filter_resolve.rs` updated: the
      `relative_paths_resolved_against_document_dir` test becomes
      "…against the declaring dir; document dir when no span".
- [x] 0.4 Real-binary E2E on the four fixtures; record output in the triage
      doc. `cargo xtask verify --skip-hub-build`; wasm `cargo check` for
      `wasm-quarto-hub-client` (quarto-core is in its closure).
- [x] 0.5 Contract: move the `filters` row from VIOLATIONS to conforming
      ("mechanism 6: `resolve_config_path`, late"); fix the stale mechanism-1
      caveat; note the `template` half-staleness. Update bd-rdcvjy2s (filters
      done; template/template-partials remain).

**Phase 0 alternative** (if the resolver pilot must not gate the #945 fix): one
`FORMAT_PATH_KEYS` row `("filters", ExistenceSilent, KeyForms::Filters)` with a
`Filters` form (array; string items unless `citeproc`/`quarto`; map items\'
`path:`), mirroring `extension/read.rs:263`. ~40 lines, zero consumer
change, the fourth fix of its kind. Not recommended as the only change, for
§2.3\'s reasons.

### Phase 1: migrate the remaining violators — strand bd-lwvumpfk

Branch `braid/bd-lwvumpfk-config-paths-phase-1`, stacked on `issue-945`
(PR #954). Failing-first tests in
`tests/integration/config_path_consumers.rs` (nine, one per consumer in
the "declared in a parent dir, consumed in a subdir" or leading-`/`
shape; all failed at the branch base for the predicted reason).

- [x] 1.1 `PathCtx::for_document` and `ConfigPath` (carrier for parsed
      types: `Listing.template`, `ComponentSource::File`).
- [x] 1.2 `template-partials` (HTML and Typst legs), missing `template`,
      `shortcodes`, `language: <file>`.
- [x] 1.3 `title-block-banner` image probe/copy; `listing.template`
      (bd-o1meelim; `std::fs` read stays, transforms carry no runtime).
- [x] 1.4 pandoc legs: `epub-*`, Typst `template`/`bibliography`/`csl`,
      `reference-doc`, `highlight-style` / `syntax-highlighting` theme
      files, `font-paths` (per-site leading-`/` rule removed).
- [x] 1.5 `brand:` from merged metadata: quarto-sass takes a
      `BrandPathResolver` closure (`brand_base_dir` for callers with no
      provenance); `config_paths::brand_path_resolver` at every
      quarto-core call site.
- [x] 1.6 `book.chapters` leading `/`; stored project-relative.
- [x] 1.7 Mechanism-3 consumers inside quarto-core routed through the
      resolver, stored form unchanged: `include-*`, `css`, `components`,
      `reference-doc`/`template`, `epub-*`, Typst template/partials.
      Not routed (below quarto-core, Phase 1b changes the hand-over):
      quarto-sass `ThemeContext::resolve_path` (`theme`), pampa citeproc
      base dir (`bibliography`/`csl` on the HTML leg).
- [x] 1.8 Scope-outs filed: revealjs `logo` (bd-7328beha), listing
      `image` prefix list (bd-epk8cmdl) — URL-space emitters.
- [x] 1.9 Contract inventory updated.

**Behaviour changes** (all in the "declared in a parent/other file" case;
front-matter declarations are unchanged):
- `brand:` in a `_metadata.yml` or front matter resolves against that
  file, not the project root. One fixture (`unreferenced_brand.rs`) wrote
  `brand: _brand.yml` in `docs/_metadata.yml` for a root file and now
  spells it `../_brand.yml`. **Open question for review:** Quarto 1's
  effective rule for most keys is "declaring dir if the file exists there,
  else the project root"; the resolver's existence policy only falls back
  for a leading `/`. A project-root fallback for existence-driven keys
  would be Q1-friendlier and less strict; not added here.
- `language: <file>` no longer tries document dir then project root; one
  candidate at the declaring file (the project root for `_quarto.yml`,
  which is what the old fallback was really serving).
- An unmarked (missing) Typst `template:` is reported at its
  declaration-resolved path instead of silently using the vendored one.
- `font-paths`, `shortcodes`, `template-partials`, highlight theme files,
  a banner image, a listing template: a project-/directory-level
  declaration reaches subdirectory pages; leading `/` = project root.

### Phase 1b: switch the stored form of marked values to the project-root pivot (§4.6) — strand bd-okqrjh8v

Branch `braid/bd-okqrjh8v-config-paths-phase-1b`, stacked on Phase 1.

**Spelling decision (deviation from §4.6, recorded for review).** The
pivot is the project-root-relative path *without* a leading `/`
(`config_paths::pivot_form`), absolute only for a file outside the
project. §4.6 and §4.4 spoke of a leading-`/` spelling; that would make
a marked `/x` ambiguous on Unix with the OS-absolute outside-project
form (a built-in extension extracted to a temp dir), which the resolver
cannot tell apart without probing the filesystem. A marked value's kind
already says "resolved", so the slash buys nothing for consumers; for
Lua the docs state the rule instead. The leading-`/` spelling of §4.4
still applies to the *unmarked* extension-rebased website strings
(bd-0x1znhgb, Phase 3), where the string has to be self-describing.

- [x] 1b.1 `mark_entry` and `adjust_paths_to_project_root` (renamed
      from `adjust_paths_to_document_dir`) write the pivot, normalized
      lexically on every platform (review 2026-10-28: the inherited
      "keep `..` off Windows for symlinks, collapse on Windows for
      `MAX_PATH`" fork was vestigial — the resolver normalizes at use
      anyway, and pivot form has no long chains — and gave the pivot a
      per-OS spelling; dropped, with the symlink caveat documented on
      `pivot_form`).
- [x] 1b.2 `!path /x` anchors at the project root; `adjust_paths_recursive`
      classifies URLs with `is_external_url` (the two mechanism-2 defects
      from the sweep).
- [x] 1b.3 A front-matter `!path` is rewritten at the merge (Document
      layer) so every marked value is pivot form; the resolver's step 1
      joins a relative marked value onto the project root.
- [x] 1b.4 Below-quarto-core consumers: `ThemeContext` built on the
      project root (HTML and reveal); pampa citeproc base = project root
      (`UserFiltersStage` and the book single-file merge). Pampa's own
      `resolve_against_base` is unchanged (the CLI still passes the input
      dir).
- [x] 1b.5 Docs: `docs/guides/authoring/lua-filters.qmd`, "How metadata
      looks from Lua": path-shaped keys are project-root-relative, unlike
      Quarto 1's input-relative form; build absolute paths with
      `quarto.project.directory`.
- [x] 1b.6 Test `lua_sees_marked_paths_in_project_root_form` pins the
      Lua-visible form across pages; unit expectations moved from
      document-relative to pivot strings.
- [x] 1b.7 Goldens: smoke-all `dir-metadata-paths` unchanged natively;
      the WASM run of the same fixture caught the one consumer that
      emitted the stored form verbatim — `FormatCssTransform` in
      source-served mode (no resolver / VFS root) now relativizes each
      marked entry to the page. Hub-client pandoc-request goldens
      unchanged (WASM suite green).

Handoff: `cargo xtask verify --skip-hub-build` green with the WASM
artifact rebuilt (17,476 Rust tests; hub-client 1,741 + 229 + 462 wasm).
Two things the first verify caught: the `wasm32` branch of
`UserFiltersStage` still named the removed `doc_dir` (the host build does
not compile that arm — rebuild the WASM before trusting a quarto-core
change), and the css emission above.

Known imperfection: an *unmarked* `bibliography`/`csl` (file missing at
merge time) declared in a subdirectory document's front matter is now
reported relative to the project root rather than the document; the
file is missing either way. A provenance-aware pampa API would fix the
message (bd-0x1znhgb territory).

### Phase 2: enforcement (bd-40luf359)

The `config-path-base` xtask lint with the blessed-module list
(`config_paths.rs`, `metadata_merge.rs`, `format_paths.rs`, the
`project/mod.rs` rebase, `glob/provenance.rs`). First cut restricted to
`stage/` and `transforms/`.

#### Phase 2 handoff (2026-10-28, branch `braid/bd-40luf359-xtask-lint-config-path`, stacked on PR #959)

- [x] 2.1 `crates/xtask/src/lint/config_path_base.rs`, wired into
      `check_file`. Scope: `quarto-core/src/{stage,transforms}/`;
      `metadata_merge.rs` blessed; test code skipped. Taint sources: a
      `let` from a metadata read (`<meta>.get(..)`, `as_plain_text()`,
      `as_str()`), a string-typed parameter of a function that also
      takes a `Path`/`PathBuf` (the ad-hoc helper shape:
      `fn probe(raw: &str, doc_dir: &Path)`), and the parameters of a
      closure over a tainted receiver (`v.as_plain_text().map(|s|
      dir.join(s))`). Closure-parameter taint is scoped to the closure
      (a shared name like `p` elsewhere is not contaminated); `let`
      names are function-wide. Sanitizers: a `let` through a
      `resolve*` call or `pivot_form` / `declaring_dir` / `page_url_for`
      / `base_dir_for` / `file_base_dir`. Sinks: `.join(..)`,
      `Path::new`, `PathBuf::from`, `std::fs::*` and runtime probes
      (`is_file`, `path_exists`, `file_read_string`, …).
- [x] 2.2 Recall check: the pre-Phase-0 versions of the migrated
      consumers (main before #954) copied into scope flag
      `include_resolve`, `language_resolve`, `pandoc_write` (5),
      `components_resolve`, `shortcode_resolve`, `title_banner` —
      every shape the consumer sweep fixed by hand. Misses:
      `apply_template` (the string reaches the join through a struct
      field), `typst_compile`, `listing_render`, `compile_theme_css`,
      `user_filters` (strings arrive via helper fns that take no
      directory, or via struct fields) — the intra-procedural limit,
      documented in the module.
- [x] 2.3 On the current tree the lint found one real violator the
      sweep missed — `transforms/book_cover_image.rs` hand-rolled
      `resolve_declared_path` — now routed through
      `resolve_config_path` (`book.cover-image` keeps the project root
      as its fallback). Four sites are allowed with reasons:
      `include_expansion::resolve_include_target` (2; markdown-space
      include targets are relative to the including file by the
      shortcode's contract, bd-udrn0q47), `prefetch_remote_images`
      (a generated file name from a URL), `components_resolve::
      resolve_relative_import` (an ES-module specifier).
- [ ] 2.4 The stronger form (a distinctively named raw accessor on
      `Path`-kind values) and widening the scope beyond `stage/` and
      `transforms/` (to `project/`, `pandoc_filters/`, `revealjs/`)
      are follow-ups once the mechanism-3 row review (§4.5) shrinks the
      blessed list; the scope constant is `SCOPE_SUBSTRINGS`.

### Phase 3: bd-0x1znhgb (§4.4 option a) and the mechanism-3 row review (§4.5)

Separate strands, after Phase 1 shows which rows still earn their place.

#### Phase 3 handoff (2026-10-28, branch `braid/bd-0x1znhgb-defer-metadata-path-resolution`, stacked on PR #958)

- [x] 3.1 §4.4 option (a): `rebase_fragment_paths` spells a rebased
      `FRAGMENT_SITE_PATH_PATTERNS` value (`website.favicon`,
      `navbar.logo`, `navbar.logo-href`, `sidebar.logo`) as
      `/_extensions/<ext>/…` — self-describing in URL space; every
      consumer (`resolve_metadata_path` → `resolve_to_project_root`,
      `normalize_favicon_path`, `copy_navbar_logo`,
      `resolve_href_for_html`) already strips the `/`. A file outside
      the project keeps its OS-absolute form. Visible in `get-config`.
- [x] 3.2 The `_extension.yml` file-name rule in `resolve_metadata_path`
      is gone. What replaces it is not a per-value recorded base (see
      the note below) but a **virtual file**: the project fragment
      (`contributes.project`, `contributes.metadata.project`) is
      re-keyed at config time to `config_paths::project_fragment_file_id`
      (`rekey_project_fragment`, called from `rebase_fragment_paths`),
      and the merge stage / every config-source binder
      (`config_sources::config_file_ids`, `register_config_source`,
      `bind_config_source`, `project_resources`, `compile_theme_css`)
      registers that id with the manifest's path and bytes. One rule,
      `config_paths::file_base_dir(id, path, project_dir)`, gives the
      project root for a fragment id and the file's directory otherwise;
      `declaring_dir`, `resolve_metadata_path` and
      `glob::provenance::BaseDirContext::base_dir_for` all call it. So a
      fragment `href: index.qmd` (bd-74ktvzv7 finding 2) and a fragment
      `listing.contents` glob anchor at the project root, while the
      manifest's `contributes.metadata` / `contributes.formats` values
      keep resolving against the extension directory.
- [x] 3.3 The Phase-2 half of bd-74ktvzv7 (`Path` values
      doc-relativized then page-relativized) is moot since Phase 1b:
      marked values are the project-root pivot and are never
      document-relativized.
- [ ] 3.4 §4.5 mechanism-3 row review — not started; separate strand
      when a row is measured to break a cache key or block a consumer.

**Why not "carry the base on the value".** The first cut wrapped each
fragment leaf's `SourceInfo` in `Generated { by: config-base {base},
from: [Invocation anchor = the authored span] }`. `resolve_byte_range`
and `root_file_id` follow the anchor, but the published
`quarto-error-reporting` renders a location from `start_offset()` /
`length()`, which are `0` for `Generated`, so three CLI tests lost their
`_extension.yml` snippet (`extension_config_spans`,
`render_scripts_cli`). `ConfigValue` has ~216 struct-literal sites, so a
new field was out too. The virtual-file id keeps every span `Original`
and needs no plumbing: the base is a function of the id. If the
source-map crate later gains a first-class resolution base (or
error-reporting learns to render anchored `Generated` spans), the
per-value form becomes available again; `file_base_dir` is the one
place to switch.

### Not in scope

Include-shortcode nested retargeting (bd-udrn0q47; markdown space), listing
record `path:` emission (bd-yghnqcfg; URL-space emission question), schema-
driven interpretation (the `meta_annotations.rs` "delete me when schemas
land" note stands; with B, schemas are for validation/LSP, not resolution).

### Phase 0 handoff (2026-10-28)

- Branch `issue-945` (remote `bugfix/issue-945`), commits: triage record,
  plan, implementation. Resolver: `crates/quarto-core/src/config_paths.rs`
  (`resolve_config_path`, `resolve_config_file`, `declaring_dir`,
  `PathCtx`). First consumer: `filter_resolve.rs`, both call sites.
- Failing-first evidence: the four bug tests in
  `tests/integration/filter_paths.rs` failed at branch base for the
  reported reason; the four guards passed; all eight pass after.
- Two things the plan did not anticipate, both now in the resolver's
  docs and the contract: (1) a `Path`-kind value with a leading `/` is
  OS-absolute, because `adjust_paths_to_document_dir` stores absolute
  paths for files outside the project (the orange-book built-in
  extension, extracted to a temp dir, broke the first cut); anchoring an
  authored `!path /x` at the project root belongs to the marking site
  (Phase 1b). (2) An authored OS-absolute filter path (`quarto::integration
  lua_host_context_cli` writes one) must keep working: `resolve_config_file`
  adds the existence policy (rule-2 candidate if it exists, else the
  OS-absolute reading if that exists, else the rule-2 candidate for the
  diagnostic), which is the effective behaviour mechanism 3 already has
  for its existence-driven keys. Phase 1 consumers with name/path
  namespaces (`theme`) should use `resolve_config_file`; always-path keys
  use `resolve_config_path`.
- Gates: `cargo xtask verify --skip-hub-build` green (17,463 Rust tests;
  hub-client unit 1,741, integration 229, wasm 462 after
  `npm run build:wasm`); E2E on the four fixtures renders 2 of 2 each.
  Windows cross-check of quarto-core is not possible from macOS (C build
  scripts); the one `#[cfg(windows)]` unit test is unverified.
- Environment notes for the next phase: `cargo xtask verify
  --skip-hub-build` runs `test:wasm` against the *existing* `pkg/`
  artifact, so rebuild it first when quarto-core changed; a stale
  `node_modules` (missing `@types/jsdom`) fails the preview-renderer
  typecheck step and is fixed by `fnm exec --using=24 npm install`.

## 6. Review decisions (2026-10-28, with the user)

1. **Phase 0 shape**: pilot the resolver on `filters`. Decided.
2. **Fallback base when there is no span** (runtime `-M`, Lua): document
   dir, today's behaviour. Decided.
3. **§4.4**: option (a), the leading-`/` spelling for rebased extension
   values. Decided.
4. **Stored form going forward**: switch to the project-root pivot as
   Phase 1b (§4.6). Decided. The Lua-visible change (a filter extension
   that reads a marked key sees `/x` where Quarto 1 showed it the
   input-relative form) is an accepted behaviour change and **must be
   documented** in the user docs when 1b lands (Phase 1b task).
6. **Execution authorized 2026-10-28.** Phase 0 is this strand's PR;
   Phases 1, 1b, 2 and 3 get their own strands and PRs.
5. **WASM / hub-client**: existence probes against an unsynced VFS degrade
   as css does today. Accepted for Phase 0.

## 7. Verification

Per phase: failing-first tests, `cargo clippy -p quarto-core --all-targets -- -D warnings`
(host and `--target x86_64-pc-windows-msvc`), `cargo nextest run -p quarto-core`,
real-binary E2E on the `claude-notes/issue-reports/945/` fixtures, and
`cargo xtask verify --skip-hub-build` before any push (full verify if the
WASM crate's closure changed, which it will).
