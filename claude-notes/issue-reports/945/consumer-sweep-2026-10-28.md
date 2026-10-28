# Consumer sweep: config-authored path strings (2026-10-28)

Fresh snapshot at `main` @ `49cb226de` (0.37.0) of every site that
joins, probes or reads a path string authored in YAML, produced for
bd-nqq41zgx by a read-only code survey (no renders), then spot-checked
by hand (`shortcode_resolve.rs`, `format_defaults.rs`,
`quarto-sass/src/config.rs` + `compile_theme_css.rs`, `pandoc_write.rs`
typst partials, `project/mod.rs` `adjust_paths_recursive`; all confirmed).
It supersedes the 2026-08-19 inventory in
`claude-notes/designs/path-resolution-model.md` for the rows it corrects;
the contract table is updated in Phase 1 of the plan.

Paths are relative to `crates/`. "FS" = filesystem space, "URL" = emitted
into HTML/XML.

## How merge-time marking stands today

- `mark_format_path_values` (`quarto-core/src/project/format_paths.rs:224`)
  runs on the project, `_metadata.yml` and front-matter layers
  (`stage/stages/metadata_merge.rs:46-111`); extension layers get only
  `adjust_paths_to_document_dir`.
- `FORMAT_PATH_KEYS` (`format_paths.rs:115-159`): `css`, `theme`,
  `include-in-header`, `include-before-body`, `include-after-body`,
  `epub-cover-image`, `epub-metadata`, `epub-embed-font`, `reference-doc`,
  `template`, `bibliography`, `csl`, `components`.
- Not in the registry: `filters`, `template-partials`, `shortcodes`,
  `highlight-style` / `syntax-highlighting`, `language`, `font-paths`,
  `brand`, `logo`, `image`, `title-block-banner`, `listing.template`.
- Leading `/`: anchored at the project root only for marked keys, and for
  an `ExistenceSilent` key only when the file exists; otherwise the raw
  `/x` reaches the consumer and is treated as OS-absolute.

## 1. Still violating (rows already in the contract table)

| Site | Key(s) | Space | Base used | Leading `/` | Status | Notes |
|---|---|---|---|---|---|---|
| `quarto-core/src/stage/stages/apply_template.rs:185,206-207` | `template` (HTML) | FS | document dir | OS-absolute when unmarked | partly fixed | `template` is in `FORMAT_PATH_KEYS`, so an existing file arrives doc-relative and the join is right; a missing file is joined to the doc dir and the error names the wrong location. |
| `apply_template.rs:186-190,415-423` (`build_partial_resolver`) | `template-partials` (HTML) | FS | document dir | OS-absolute | violation | Not in the registry; correct only for extension values (force-marked). |
| `quarto-core/src/filter_resolve.rs:179-180,206-207,322-338` | `filters` (string and `{path:}` forms) | FS | document dir | `is_absolute` → OS-absolute | **violation (GH #945)** | Called from `user_filters.rs:134-142` and `pandoc_write.rs:929`. |
| `quarto-core/src/transforms/title_banner.rs:116-122,141-146,200-207` | `title-block-banner` (image form) | FS probe + URL copy | document input dir | OS-absolute | violation | Copy destination is `page_dir().join(raw)`. |
| `quarto-core/src/project_resources.rs:705-722,827-836` | engine-/filter-reported resources | FS | document dir | kept as-is | out of scope | Runtime output, not YAML; the test at `:1704` pins the behaviour on purpose. |

## 2. New violations (not in the contract table)

| Site | Key(s) | Space | Base used | Leading `/` | Notes |
|---|---|---|---|---|---|
| `quarto-core/src/stage/stages/pandoc_write.rs:578-591` (`typst_template_files`) | `template-partials` (Typst leg) | FS | document dir | OS-absolute | Twin of the HTML partials bug; the comment at `:574-576` says plain scalars resolve against `doc_dir`. |
| `quarto-core/src/transforms/shortcode_resolve.rs:1193-1203` (`extract_shortcode_paths`) | `shortcodes` | FS | document dir | OS-absolute | Project- or directory-level `shortcodes:` resolves against the consuming document. |
| `quarto-core/src/pandoc_filters/format_defaults.rs:259-272` | `highlight-style` (`.theme`/`.xml` file) | FS (pandoc arg) | document dir | OS-absolute | Forwarded as `PandocArg::Path(doc_dir.join(v))`. |
| `quarto-core/src/pandoc_filters/typst_highlight.rs:126-152` (`load_theme_json`) | `syntax-highlighting` / `highlight-style` | FS | document dir | OS-absolute | Typst leg; reads the file. |
| `quarto-core/src/stage/stages/language_resolve.rs:193-205` (`load_language_file`) | `language: <file>.yml` | FS | doc dir, then project root | OS-absolute | A two-candidate guess is "a consumer choosing its own base". |
| `quarto-core/src/stage/stages/typst_compile.rs:230-233,413-424`; `pandoc_write.rs:387-421` | `font-paths` | FS | document input dir | handled per site (`/` → project root) | `_quarto.yml` value resolves against the document dir; per-site `/` handling breaks author rule 3. |
| `quarto-core/src/transforms/listing_render.rs:510-536` (`load_custom_template`); parsed at `project/listing/config.rs:604` | `listing.template` | FS | host page dir | `is_absolute` → OS-absolute (bd-o1meelim) | Wrong base from `_metadata.yml`; reads with `std::fs::read_to_string`, bypassing the `SystemRuntime`/VFS. |
| `quarto-core/src/revealjs/footer_logo.rs:168-176` (`logo_slot_html`) | `logo` (revealjs) | URL | none: raw string emitted | verbatim | No `page_url_for`, no copy. |
| `quarto-sass/src/config.rs:699-711` (`load_split_brand`), called from `compile_theme_css.rs:613`, `pandoc_write.rs:282`, `typst_compile.rs:215`, `transforms/hephaestus.rs:348` | `brand:` (path form) | FS | **project dir**, applied to merged document metadata | `is_absolute` → OS-absolute | The contract calls this "project-root by construction"; that holds only for `project/mod.rs:2438` (reads `_quarto.yml`). These four read the merged metadata, so a `brand:` in front matter or `_metadata.yml` resolves against the project root. |
| `quarto-core/src/project/book/render_item.rs:285-300,360`; `project/book/static_analyzer.rs:103` | `book.chapters` / `appendices` hrefs | FS | project dir (correct) | not handled (`/intro.qmd` → OS root) | Leading `/` only. |
| `quarto-core/src/project/listing/item.rs:285-291,431-465`; `project/listing/helpers.rs:70-73,96-102`; `transforms/listing_generate.rs:503-507` | front-matter `image`, `listing-item.image` | URL + copy | document's source dir (`rebase_image`) | `is_external_src` treats `/` as external: emitted verbatim, never copied (bd-epk8cmdl) | `is_external_src` is a hand-written prefix list; the contract names `is_external_url` as the sole classifier. An `image:` in `_metadata.yml` gets the document's dir. |
| `quarto-core/src/project/mod.rs:290-294` (`adjust_paths_recursive`) | every explicit `!path` value | FS | declaring dir → document dir | `is_rooted` → skipped, i.e. OS-absolute | Inside mechanism 2 itself: a prefix list (`http://`/`https://`) instead of `is_external_url`, and `!path /x` is never anchored at the project root. |

## 3. Conforming

| Site | Key(s) | Space | Base used | Leading `/` | Mechanism |
|---|---|---|---|---|---|
| `format_paths.rs:115-159,192-197,224-322`; `metadata_merge.rs:73-104` | the registry keys | FS | layer base | project root | (3) |
| `stage/stages/include_resolve.rs:559-570` | `include-*` | FS | doc dir on an `Always`-marked value | via marking | (3) |
| `transforms/format_css.rs:113-121` | `css` | FS + URL | doc dir, `Path`-kind only | via marking | (3) |
| `quarto-sass/src/themes.rs:463-469`; `compile_theme_css.rs:462-469` | `theme` (custom SCSS) | FS | doc dir on a marked value | via marking; OS-absolute if unmarked | (3) |
| `pandoc_filters/format_defaults.rs:216-256` | `reference-doc`, `template` (pandoc leg) | FS | `Path`-kind only; else hard error Q-5-30 | via marking | (3) |
| `pandoc_write.rs:1700-1708` | `template` (Typst leg) | FS | `Path`-kind only; unmarked silently ignored | via marking | (3) |
| `pandoc_write.rs:153-158` | `epub-*` | FS | doc dir on a marked value | via marking | (3) |
| `pampa/src/citeproc_filter.rs:204-260` | `bibliography`, `csl` | FS | base dir passed in | marked values fine; unmarked `is_rooted` → OS root although the comment at `:205-208` says project root | (3) |
| `pandoc_write.rs:1262-1270` | `bibliography`, `csl` (Typst PDF request) | FS | document dir | handled per site (rule 3 breach, redundant) | (3) |
| `transforms/components_resolve.rs:146`; `components/config.rs:222` | `components[].src` | FS + copy | doc dir on an `Always`-marked value | via marking | (3) |
| `transforms/{navbar,sidebar,footer}_generate.rs` | nav hrefs, navbar logo / logo-href | URL | `SourceInfo` → declaring dir | project root | (1) `navigation_href.rs:779` (contract cites `:583`) |
| `project_resources.rs:245-260,489-510`; `project/discovery.rs:575`; `transforms/sidebar_auto.rs:275` | `resources:`, `project.resources`, `project.render`, sidebar `auto:` | FS (glob) | `BaseDirContext` | project root | (4) |
| `project/mod.rs:880-895`; `extension/paths.rs:65-72,149,166-177`; `extension/read.rs:337,349,425,781` | extension-contributed values | FS | extension dir | `ext_dir.join("/x")` → OS-absolute (minor) | (5) |
| `project/website_post_render.rs:75-96,128-176,200-232,430-436`; `project/website_config.rs:157-174` | `favicon`, navbar logo copy, footer image copy | FS copy | project dir (reads `project.config.metadata` only) | stripped → project root | project root by construction |
| `project/render_scripts.rs:486-492` | `pre-render`, `post-render` | FS (exec) | project dir | not handled | project root by construction |
| `brand_fonts.rs:215-224`; `pandoc_filters/typst_brand.rs:180-205` | `_brand.yml` font files, logo assets | FS | brand file's dir | project root | brand-file dir |
| `transforms/book_cover_image.rs:85-110`; `project/book/single_file_render.rs:486-505` | `book.cover-image` | FS + URL | project dir | per site | project root by construction (rule 3 breach) |
| `stage/stages/include_expansion.rs:772-780` | `{{< include >}}` | markdown | includer's dir | project root | conforming (contract cites `:689`) |
| `project/listing/feed/binding.rs:250-277` (`website_image` branch) | `website.image` | FS probe | project dir | — | project root by construction |

## 4. Unclear (needs a decision)

| Site | Key(s) | Notes |
|---|---|---|
| `project/listing/feed/binding.rs:258-270` | `listing.feed.image` | Declared in the host page's front matter but joined to the project dir; `/x` → OS root; probes with `imagesize` directly instead of the runtime. |
| `project/listing/feed/binding.rs:363-375` | listing item image (feed) | Downstream of the item-image row in §2. |
| `project/listing/feed/binding.rs:231-235`; `project/listing/config.rs:979` | `listing.feed.xml-stylesheet` | Emitted verbatim into the feed XML; no resolution, no copy. |
| `pandoc_filters/typst_brand.rs:276-330` (`build_logo_param`) | document `logo.path` (Typst) | Passed to Typst unresolved. |
| `quarto-brand/src/types.rs:873-881` (`with_path_relative_to`) | `_brand.yml` `logo.*` | `is_rooted` passes through; leading `/` never anchored explicitly. |
| `project/mod.rs:2401-2405,2084-2091` | `project.output-dir` | `dir.join("/x")` → OS-absolute; whether rule 2 should apply to an output location is a decision. |
| `extension/paths.rs:71`; `project/mod.rs:801` | `format-resources` | Marked, but no consumer copies it; a user-level `format-resources` is never read. |

Keys with no consumer at all (nothing to audit yet): `open-graph` /
`twitter-card` images, `lightbox`, `notebook-links`, `about.image`,
`metadata-files`, `execute-dir`, `lib-dir`, `revealjs-plugins`, sidebar logo.

## Corrections the contract table needs

- Re-scope the `apply_template.rs` row to `template-partials` (HTML + Typst).
- Refresh stale line numbers (`filter_resolve`, `include_expansion`,
  `navigation_href`, `project_resources`).
- Move `brand:` (merged-metadata readers) and the listing image emitter out
  of Conforming.
- Add Conforming rows for `epub-*`, `components`, `bibliography`, `csl`.
- Add the §2 rows, each with a strand once the plan is approved.
