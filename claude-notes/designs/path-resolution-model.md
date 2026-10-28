# Path resolution contract (config-authored paths)

**Status:** Normative contract. Promoted 2026-08-19 from the 2026-06-10
index note, after the #455/#524 divergence assessment
(`claude-notes/research/2026-08-19-path-resolution-class-assessment.md`).
Peer of `transform-pipeline-phases.md` and `document-profile-contract.md`:
code that consumes config-authored paths MUST conform; deviations are bugs
even when no test pins them yet.

**The one-sentence rule for authors:** *a consumer of a config-originated
path never chooses its own base directory.*

## The two rules (normative)

Quarto 2 interprets a path written in source (`.qmd` front matter,
`_quarto.yml`, `_metadata.yml`, extension config) by two rules:

1. **No leading `/` → relative to the directory of the file that *declared*
   the path.** Not "relative to the project root" in general — that is the
   special case for `_quarto.yml`, which lives at the project root.
   Concretely:
   - a path in `_quarto.yml` resolves against the project root,
   - a path in `docs/foo/_metadata.yml` resolves against `docs/foo/`,
   - a path in `docs/foo/bar.qmd`\'s front matter resolves against `docs/foo/`.
   Getting this right requires **provenance**: the resolver must know which
   file declared the value, not which document is consuming it. Provenance is
   captured either as `SourceInfo` retained through the merge, or by
   marking + rebasing the value at merge time (see mechanisms below).

2. **Leading `/` → project-root-relative, uniformly** (decision 4 of
   bd-root-relative-paths-design-fc5pvkcv, extended to filesystem space by
   the #455 discussion). It is never an OS-absolute filesystem path. What
   "resolve" then means depends on the space (next section).

Carve-outs rule 2 must not swallow: anything with an RFC-3986 scheme
(`https:`, `data:`, `mailto:`, `positron:`, `javascript:`, …) or the
protocol-relative `//host/x` form — all classified external before any
path handling. **`quarto_util::is_external_url` is the sole classifier;
never write a prefix list** (`starts_with("http://") || …`). Every such
list drifts to a different subset and the schemes it omits get
path-normalized — `//` collapsed, `../` prepended
(bd-scheme-href-path-normalized-w5zya82r replaced seven of them);
WASM VFS `/project/...` paths (a filesystem-space *internal* convention,
never authored config); genuinely OS-absolute *input-file* paths where a key
documents that it accepts them (checked with `quarto_util::is_rooted`, not
`Path::is_absolute`, for Windows correctness).

## Two spaces, one pivot, two exits

Every config-authored path belongs to exactly one of two spaces, and a
resolved path has a different terminal form in each:

- **Filesystem space** — the renderer reads the file at build time
  (`include-in-header`, `theme`, `template`, `filters`, `bibliography`,
  `csl`, …). Terminal operation: a `SystemRuntime` read against
  *declaring-dir ⊕ relative path*, or *project root ⊕ leading-`/` path*.
- **URL space** — the string survives into emitted HTML (image `src`, link
  `href`, logo, css `<link>`, …). Terminal operation: a **page-relative**
  href computed per consuming page (`ResourceResolverContext::page_url_for`),
  usually paired with a copy-to-`_site` action. No emitted HTML ever
  contains a `/`-absolute link, so a built `_site/` is relocatable under any
  deploy subpath.

The stable pivot between declaration and either exit is the
**project-root-relative canonical form with forward slashes** (or,
equivalently under mechanism 2 below, the doc-relative form produced by a
provenance-aware rebase). Resolution to a native absolute path or a final
href is always a lazy, terminal operation — never stored state, because
these values round-trip through serialization, hashing, and the qmd writer.

**The seam warning (the #524/#455 lesson):** any decree, fix, or audit
scoped to one space MUST record the other space's corresponding work as a
braid strand linked to bd-oejuizi9 before it ships. PR #524 fixed URL space
and carved out filesystem space without filing the complement; issue #455
sat in the carve-out for another release cycle.

## Author rules (MUST)

1. **Never choose a base directory at a consumption site.** No
   `doc_dir.join(config_string)`, no `Path::new(config_string)`, no CWD
   reads. Consume a value already marked/rebased at merge time
   (`ConfigValueKind::Path`), or call a blessed resolver (below) with the
   value's provenance.
2. **New path-shaped config keys** must be added to the marking registry
   (currently the tables listed under mechanisms) *and* to the inventory
   table in this document, in the same commit that introduces the key.
3. **Leading `/` handling lives in the resolvers only.** Do not add per-site
   leading-slash special cases; if a blessed resolver lacks the behavior you
   need, extend the resolver.
4. **Scope-outs are strands.** If your fix deliberately leaves a
   nonconforming site untouched, file the strand and link it before merging.

## Blessed mechanisms (current state; convergence target below)

Three provenance-correct mechanisms coexist today. New code should prefer
(3)\'s shape; the convergence work will fold them together.

1. **`resolve_metadata_path`**
   (`crates/quarto-core/src/transforms/navigation_href.rs:779`) —
   `SourceInfo` → declaring file's dir → project-root-relative string, at
   Generate time. Used by navigation surfaces (sidebar/navbar/footer
   generate transforms). The former caveat that `_quarto.yml`\'s FileId is
   not in the per-document `SourceContext` is **stale** since
   bd-x113wg9v / bd-fu16z22k / bd-r64mj1aa: `MetadataMergeStage`
   registers `_quarto.yml`, the profile overlays, every `_metadata.yml`
   and every `_extension.yml` (`metadata_merge.rs:214-266`). Remaining
   caveat: a value rebased by `rebase_fragment_paths` keeps its manifest
   span while its string is already project-root-relative (bd-0x1znhgb;
   the fix decided 2026-10-28 is the leading-`/` spelling, option (a) in
   `claude-notes/plans/2026-10-28-config-path-late-binding.md` §4.4).
2. **`ConfigValueKind::Path` + `adjust_paths_to_document_dir`**
   (`crates/quarto-core/src/project/mod.rs:253-296`) — merge-time rebase of
   `Path`-kind values from declaring dir to consuming-doc dir. Fed by
   explicit `!path` tags and by the extension marking tables
   (`FRAGMENT_PATH_PATTERNS`, `FORMAT_ASSET_PATTERNS`, `PATH_VALUED_KEYS`).
   Blind spot: user-authored plain strings are never marked.
3. **Per-layer `layer_base` marking** (`crates/quarto-core/src/project/format_paths.rs`,
   `FORMAT_PATH_KEYS` — originally css-only in `format_css.rs`, generalized
   2026-08-19 for bd-oejuizi9) — the merge passes the declaring base per
   layer (project dir / `_metadata.yml` parent / doc dir), marks the value
   `Path` rebased to doc-relative form, anchors leading `/` at the project
   root. The most general mechanism: it handles `_quarto.yml` without
   SourceInfo lookup and runs before any consumer. Covers `css`, `theme`,
   and the three `include-*` slots. Note the registry carries a per-key
   **marking policy** the original contract text did not anticipate: some
   keys\' strings are only *sometimes* paths (`theme` shares its namespace
   with builtin theme names → existence-driven, silent), while others are
   always paths (`include-*` → unconditional, so even a missing file's
   later diagnostic reports the declaration-resolved location).
4. **`BaseDirContext`** (`crates/quarto-core/src/glob/provenance.rs:47`) —
   the provenance engine for glob-valued keys (`listing.contents`,
   front-matter `resources:`). Globs stay on this machinery; they are
   pattern sets, not single paths.

5. **`resolve_config_path`** (`crates/quarto-core/src/config_paths.rs`,
   bd-nqq41zgx / GH #945, 2026-10-28) — **the convergence target below,
   in tree.** Takes the `ConfigValue` (not the extracted string) plus a
   `PathCtx { source_context, project_dir, document_dir }` and binds the
   base late: a `Path`-kind value is already document-relative, or
   OS-absolute for a file outside the project (mechanisms 2/3 output,
   honoured as-is); a URL is classified by `is_external_url`; a leading
   `/` anchors at the project root; an OS-absolute path (Windows
   drive/UNC) is used as written; otherwise the base is the declaring
   file's directory from the value's `SourceInfo`, with the document
   directory as the fallback when there is no usable span (runtime `-M`,
   Lua, programmatic values — plan decision 2). No key registry is
   consulted: the consumer knows it is reading a path. The companion
   `resolve_config_file` adds the existence policy for keys whose strings
   are only sometimes paths (`filters`, `theme`): the rule-2 candidate
   when it exists, else the legacy OS-absolute reading of an authored
   leading `/` when *that* exists (Quarto 1's escape hatch for a filter
   outside the project; the same effective behaviour mechanism 3 has for
   its existence-driven keys), else the rule-2 candidate for the
   diagnostic. First consumer: `filter_resolve.rs`. Every remaining
   violator below migrates to it (plan Phase 1); mechanism 3's stored
   form switches to the project-root pivot afterwards (Phase 1b).

**Convergence target** (assessment §6.2, tracked under bd-oejuizi9 +
bd-hjv5o): generalize mechanism 3 into a single path-shaped-key registry
unifying the four scattered tables plus the annotation table
(`crates/pampa/src/pandoc/meta_annotations.rs`, whose `Interpretation::Path`
is currently unused), so values arrive at consumers already marked and
declaration-dir-resolved, and consumers\' existing `doc_dir.join` reads
become correct as written. Enforcement: the `config-path-base` xtask lint
(see strand reference in the inventory's gap list).

## Inventory of consumption sites (living — keep current)

Snapshot verified 2026-08-19 (full evidence in the assessment doc §4).
Update this table when adding keys or migrating sites.

### Conforming

| Site | Keys | Mechanism |
|---|---|---|
| `transforms/{navbar,sidebar,footer}_generate.rs` | nav hrefs, logos | (1) `resolve_metadata_path` |
| `glob/provenance.rs` + `project/listing/glob_resolve.rs`, `project_resources.rs` | `listing.contents`, front-matter `resources:` | (4) `BaseDirContext` |
| `project/format_paths.rs` (`FORMAT_PATH_KEYS`) + `metadata_merge.rs` call sites | `css`, `theme`, `include-in-header`/`-before-body`/`-after-body`, `reference-doc`, `template` | (3) layer_base marking; consumers (`include_resolve.rs`, `ThemeContext.resolve_path`, `FormatCssTransform`, `pandoc_filters::format_defaults::build_forwarded_args`) read the marked doc-relative values unchanged (fixed 2026-08-19, bd-oejuizi9 / GH #455; `reference-doc`/`template` added 2026-09-20, P7 Task 4 — `ExistenceSilent`, escalated to a hard `Q-5-30` error at the pandoc-invocation consumption site rather than a merge-time warning, since a missing reference doc/template cannot be handed to pandoc at all) |
| `project/mod.rs` fragment rebase; `extension/{paths,read}.rs` | extension-contributed theme/css/include-\*/template/filters | (2) force-marked `Path` |
| `website_config.rs`, `website_post_render.rs` | `favicon`, navbar logo / footer image copy | project-root by construction (`_quarto.yml`-only keys) |
| `discovery.rs`, `project_resources.rs`, `sidebar_auto.rs`, `quarto-sass/src/config.rs` | `project.render`, `project.resources`, sidebar `auto:`, `brand:` | project-root by construction |
| `quarto-core/src/brand_fonts.rs` (`resolve_source`) | `_brand.yml` `typography.fonts[].files[].path` (`source: file`) | brand-file dir base; leading `/` = project root; **emitted URL is artifact-relative** (`fonts/<basename>` beside the theme CSS), never a rebased source path (bd-ve916wr8) |
| URL-space emitters (`link_rewrite`, `navbar/footer_render`, `website_favicon`, `transforms/format_css`, `example_embed`, listing `item.rs`) | emitted hrefs/srcs | `page_url_for` family (rule-2 exit) |
| `transforms/repo_actions_render.rs` + `quarto-navigation::repo_actions` | `repo-url`, `repo-branch`, `repo-subdir`, `issue-url` | absolute external URLs built from the pivot form (`page_relative_source`); **neither rule-2 exit** — no `page_url_for`, no filesystem read. `repo-subdir` is a *repository*-namespace path, outside this contract entirely. |
| `include_expansion.rs:772` (`resolve_include_target`) | `{{< include >}}` (markdown space) | project-root leading-`/` + includer-dir |
| `filter_resolve.rs` (`resolve_filters`, both call sites `user_filters.rs`, `pandoc_write.rs`) | `filters` (string and `{path:}` forms) | (5) `resolve_config_path`; existence probe at the declaration-resolved path, then extension-name lookup (fixed 2026-10-28, bd-nqq41zgx / GH #945; tests `tests/integration/filter_paths.rs`) |

### VIOLATIONS (each tracked by a strand — do not fix one without checking its siblings)

| Site | Keys | Defect | Strand |
|---|---|---|---|
| `stage/stages/apply_template.rs:185-207,415-423` | `template` (missing-file case only; an existing file is marked by `FORMAT_PATH_KEYS`), `template-partials` | consuming-doc-dir join; leading `/` OS-absolute | bd-rdcvjy2s; Phase 1 of bd-nqq41zgx |
| `stage/stages/pandoc_write.rs:578-591` | `template-partials` (Typst leg) | same | Phase 1 of bd-nqq41zgx |
| ~~`filter_resolve.rs:255-273`~~ | `filters` | ~~same~~ **FIXED 2026-10-28** (bd-nqq41zgx): mechanism 5 | ~~bd-rdcvjy2s~~ (filters half) |
| `transforms/title_banner.rs:116-207` | `title-block-banner` image probe | doc-dir probe, raw string emitted | bd-hjv5o scope; Phase 1 of bd-nqq41zgx |
| `project_resources.rs:705-722,827-836` | engine/filter-declared resources | doc-dir join — runtime output, not YAML; out of this class unless a key is added | bd-hjv5o scope |
| `transforms/shortcode_resolve.rs:1193-1203` | `shortcodes` | doc-dir join | Phase 1 of bd-nqq41zgx |
| `pandoc_filters/format_defaults.rs:259-272`, `pandoc_filters/typst_highlight.rs:126-152` | `highlight-style` / `syntax-highlighting` theme files | doc-dir join | Phase 1 of bd-nqq41zgx |
| `stage/stages/language_resolve.rs:193-205` | `language: <file>.yml` | doc dir, then project root (a consumer guessing its base) | Phase 1 of bd-nqq41zgx |
| `stage/stages/typst_compile.rs:230-233,413-424`, `pandoc_write.rs:387-421` | `font-paths` | doc input dir; per-site leading-`/` rule (author rule 3) | Phase 1 of bd-nqq41zgx |
| `transforms/listing_render.rs:510-536` | `listing.template` | host page dir; leading `/` OS-absolute; reads via `std::fs` not the runtime | bd-o1meelim; Phase 1 of bd-nqq41zgx |
| `revealjs/footer_logo.rs:168-176` | revealjs `logo` | raw string emitted, no `page_url_for`, no copy | Phase 1 of bd-nqq41zgx |
| `quarto-sass/src/config.rs:699-711` via `compile_theme_css.rs:613`, `pandoc_write.rs:282`, `typst_compile.rs:215`, `transforms/hephaestus.rs:348` | `brand:` (path form) read from **merged** metadata | project dir regardless of declarer (front matter / `_metadata.yml` wrong) | Phase 1 of bd-nqq41zgx |
| `project/book/render_item.rs:285-300,360`, `project/book/static_analyzer.rs:103` | `book.chapters` / `appendices` | project dir (right); leading `/` → OS root | Phase 1 of bd-nqq41zgx |
| `project/listing/item.rs:285-291,431-465`, `project/listing/helpers.rs:70-73,96-102`, `transforms/listing_generate.rs:503-507` | front-matter `image`, `listing-item.image` | hand-written external-prefix list; `/` treated as external | bd-epk8cmdl; Phase 1 of bd-nqq41zgx |
| `project/mod.rs:290-294` (`adjust_paths_recursive`, mechanism 2 itself) | every explicit `!path` | `http://`/`https://` prefix list instead of `is_external_url`; `!path /x` never anchored at the project root (the resolver's step 1 now compensates at consumption) | Phase 1b of bd-nqq41zgx |
| ~~`pampa/src/citeproc_filter.rs:133,151`~~ | `csl`, `bibliography` | ~~**process-CWD** read (no base at all)~~ **FIXED 2026-09-24** (book-projects P2): declaration-site base dir threaded into `apply_citeproc_filter` from both call paths (`UserFiltersStage` = document dir; book single-file merge = first file-chapter's dir); both keys added to `FORMAT_PATH_KEYS` (`ExistenceSilent` — URLs stay scalar) | ~~bd-oqoozmtr~~ (closed) |

Related open strands: bd-nqq41zgx (the convergence plan,
`claude-notes/plans/2026-10-28-config-path-late-binding.md`; full
2026-10-28 sweep in `claude-notes/issue-reports/945/consumer-sweep-2026-10-28.md`,
which also lists the "unclear" sites — feed images, Typst `logo.path`,
`project.output-dir`, `format-resources`), bd-hjv5o (the generalization
audit this table operationalizes), bd-r1y48cx0 (`css:` copy — possibly
resolved by `37758160`; verify end-to-end before closing).

## How to audit (the sweep that works)

Do **not** enumerate implementations of the convention (grepping for
leading-`/` handlers is structurally blind to sites that do a bare
`Path::join` and handle nothing — exactly how #524's survey missed #455).
Enumerate **consumers** and check each against this table:

- `rg 'runtime\.file_read|fs::read' crates/quarto-core crates/pampa crates/quarto-sass`
  — every hit that reads a config-originated path must trace to a blessed
  mechanism.
- `rg '\.join\(' crates/quarto-core/src/stage crates/quarto-core/src/transforms`
  — joins whose right-hand side derives from `as_plain_text()`/`as_str()`
  on a config value are suspect unless the value is `Path`-kind (mechanism 2
  output) or the site is a blessed resolver.
- Cross-check the key lists in the marking tables against this inventory.

If a bug report mentions a path resolving against the wrong directory,
assume it is an instance of this class: verify the *sibling* keys in the
same table row before scoping the fix to the reported key.

## History / references

- `claude-notes/research/2026-08-19-path-resolution-class-assessment.md` —
  why #524 missed #455; full evidence for the inventory; remediation plan.
- `claude-notes/plans/2026-05-20-bd-qor9a-metadata-path-resolution.md` —
  mechanism 1 and the deferred audit (bd-hjv5o).
- `claude-notes/plans/2026-08-13-site-root-relative-paths.md` — PR #524,
  the URL-space decree and its carve-outs.
- `claude-notes/plans/2026-02-17-dir-metadata-path-resolution.md` —
  mechanism 2 origin.
- `claude-notes/designs/body-link-resolution-contract.md` — URL-space
  resolution rules for body links.
- `claude-notes/designs/provenance-contract.md` — SourceInfo provenance.
