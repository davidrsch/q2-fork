---
title: 'Implement q2 add: install an extension from GitHub, URL, or local path (bd-rfz4e3bw)'
date: 2026-10-22
description: 'q2 add is a stub; implement Quarto 1''s extension install on top of the quarto-source-fetch machinery that q2 use brand already ships.'
status: implemented  # 2026-10-22, on braid/bd-rfz4e3bw-q2-add-extension
braid:
  strand: bd-rfz4e3bw
---

**Branch:** `braid/bd-rfz4e3bw-q2-add-extension` (topic branch in the main checkout, based on `main` @ `47337c56b`)
**Design settled with the user on 2026-10-22; implemented the same day. See § Implementation notes.**

## Triage verdict

**Ready to implement (design settled).** Most of what `q2 add` needs already exists: target
parsing, fetching, safe extraction, extension reading, and organized `_extensions/<org>/<name>/`
discovery. The new work is an install planner, a replace-directory install step, Q1's
confirmation table, and a `.quarto-source.yml` provenance record with file hashes that gates
overwrites behind `--force`. The decisions are recorded below.

## Issue context

Feature, P2, filed 2026-10-22 by Claude Fable 5.1 while assessing `posit-dev/product-doc-theme`
on q2. `q2 add <anything>` exits 1 with `Command not yet implemented: add`
(`crates/quarto/src/commands/add.rs`, CLI arm `Commands::Add { extension }` at
`crates/quarto/src/main.rs:441`, dispatched at `:1722`). Acceptance: the four source kinds
install into `_extensions/<owner>/<name>/`, a Q1-style table is printed, `--no-prompt` skips
confirmation, and a second add reports No Change. The end-to-end check is that
`q2 add posit-dev/product-doc-theme` followed by `project.type: posit-docs` renders. Required
tests: local dir, local archive, and a recorded archive fetch.

## Dependency graph

- **discovered-from**: bd-0i6fii0k (in_progress, P1): the product-doc-theme assessment. That is
  finding F4. Its plan `claude-notes/plans/2026-10-21-product-doc-theme-q2.md` lives on branch
  `braid/bd-0i6fii0k-product-doc-theme-q2` (commit `9f3c57e44`) and is **not on `main`**. The
  assessment worked around the missing `q2 add` by copying `_extensions/` by hand. Nothing else
  in that assessment depends on this strand.
- **blocks / related**: none. bd-0i6fii0k is in turn related to bd-ad7i1pc6 (custom project
  types, which makes `type: posit-docs` resolve once the files are in place).
- No incoming pressure: this is a parity and usability gap, not a blocker for other strands.

## What the code looks like today

Still reproducible at HEAD: `add.rs` is the 8-line `NotImplemented` stub described in the strand.

Reusable pieces (verified):

- `crates/quarto-source-fetch`: `resolve_target` handles local path, `http(s)` URL, and
  `org/repo[/subdir][@ref]`. It already fixes Q1 defects: it probes `main` then `master`, tries
  tags then heads for `@ref`, and derives the archive root instead of predicting it.
  `fetch_into` plus `derive_archive_root` produce a source directory. Its crate doc says it was
  split out "because `q2 add` will need identical machinery".
  **Gap:** `RemoteTarget` does not record the GitHub org. Q1 uses `owner = host.organization`
  for the install path, so `RemoteTarget` needs an `owner: Option<String>` field.
  **Gap:** Q1's `findExtensionDir` looks for `_extensions/` at the root of the archive or one
  level down (`install.ts:283-300`), and also accepts a path that *is* `_extensions`
  (`install.ts:609-621`). `derive_archive_root` only unwraps a single top-level directory.
  That covers GitHub archives, but the "path that is `_extensions`" case needs its own handling.
- Network tests use a scripted `FakeFetch` and a real localhost server
  (`crates/quarto-source-fetch/tests/integration/network.rs`). "Recorded fetch" in the
  acceptance criteria should mean the same pattern. Nothing is recorded from github.com.
- `crates/quarto-core/src/extension/read.rs`: `read_extension_with_org` returns `Extension`
  with `title` and `version: Option<String>`. Q1 compares versions with semver. The workspace
  has no `semver` dependency.
- `discover.rs::scan_extension_entry` already handles both `_extensions/<name>/` and
  `_extensions/<org>/<name>/`. `find_extension("posit-docs")` matches by name across orgs, so a
  GitHub install into `_extensions/posit-dev/posit-docs/` should still resolve
  `type: posit-docs`. This needs an end-to-end test.
- `crates/quarto/src/commands/common/{plan,writer,prompter}.rs`: the shared `FilePlan` writer
  **never overwrites** (an existing file is `SkippedExisting`). Q1 `add` replaces an installed
  extension wholesale: it copies to `._extensions.staging`, removes the old directory, and
  renames the new one in (`install.ts:556-606`). An update therefore cannot go through
  `execute_plan` as it is.
- `q2 use brand` (`use_cmd/brand.rs`) is the model for the command structure: resolve, then
  fetch, then plan, then execute. It has a human output path and a `--json` path, an
  `InquirePrompter` that is tested through a fake `Prompter`, and a **fail-closed** trust
  gate: the default answer is no, and with no terminal the command refuses unless `--trust` is
  passed.

Corrections to the strand description, from reading `install.ts`:

- `[Update]`/`[No Change]` comes from a **version comparison**, not a diff. Equal versions
  report No Change. There is also a `[Revert]` action for a downgrade. When neither side has a
  version, Q1 always reports `[Update]`. The table also shows `from -> to` and a type column
  (`(format)`, `(shortcodes)`, `(filter)`, …).
- Even on `No Change`, `completeInstallation` still re-copies every staged extension.
- Q1's trust prompt defaults to **yes**, and `--no-prompt` installs remote code without asking.
- Install root: Q1 uses cwd unless cwd is inside a project subdirectory and the user answers
  yes to "Install extension into project?". With `--no-prompt` it uses **cwd**, not the project
  root as the strand states.
- After installing, Q1 offers to open the `learnMoreUrl` in a browser.

## Design decisions (settled with the user, 2026-10-22)

Each item records where q2 deliberately deviates from Quarto 1. The strand asks for those
deviations to be stated.

1. **Trust gate: fail closed, like `q2 use brand`.** A remote source (URL or GitHub) needs
   either a "yes" at a prompt that defaults to **no**, or `--trust`. With `--no-prompt`,
   `--json`, CI, or no terminal, a remote source without `--trust` is refused before anything
   is fetched. Local sources need no trust step. *Deviation:* Q1 defaults to yes and
   `--no-prompt` installs remote code silently.
2. **Install root: project root, else cwd, overridable with `--dir <path>`.** No "install into
   project?" question. *Deviation:* Q1 installs into cwd unless the user answers that question
   with yes.
3. **Provenance: a sibling file `_extensions/[<org>/]<name>/.quarto-source.yml`.** The author's
   `_extension.yml` is never touched. The file opens with a generated-file comment, e.g.
   `` # Generated by `q2 add`. Do not edit: q2 uses it to detect local changes before an update. ``
   Contents (draft):
   ```yaml
   # Generated by `q2 add` ...
   version: 1               # schema version of this file
   source: posit-dev/product-doc-theme   # what the user typed
   kind: github             # github | url | local-archive | local-dir
   ref: null                # the @ref, when given
   files:                   # path relative to the extension dir -> sha256
     _extension.yml: 9f2c…
     assets/images/logo.svg: 41ab…
   ```
   *Deviation:* Q1 records nothing.
4. **Version comparison: a `semver` workspace dependency, matching Q1 exactly.** The actions are
   Install / Update / Revert / No Change. Both unversioned gives Update. Versioned to
   unversioned (and the reverse) gives Update, with Q1's from/to labels. Q1 parses with npm's
   lenient `semver.coerce` (`extension.ts:813`), so `1.2` becomes `1.2.0`, `v1` becomes
   `1.0.0`, and an unparseable string counts as unversioned. Rust's `semver` crate is strict, so
   we need a small `coerce` helper with tests that pin those cases. The table columns follow
   Q1: name (title, falling back to `org/name`), `[action]`, from, `->`, to, and types.
5. **Flags: `--no-prompt`, `--trust`, `--dir`, `--dry-run`, `--json`, `--force`.** The "open
   docs in browser" prompt is dropped (print the learn-more URL instead). `--embed` is out of
   scope. `--dry-run` combined with `--force`/`--trust` is refused, the same rule `q2 use`
   applies.
6. **Overwrite safety: `--force` is required to replace locally modified files.** Before an
   Update or Revert, the installed directory is checked against the hashes in its
   `.quarto-source.yml`. A modified hash, a missing file, or an extra file counts as a local
   edit. When any extension in the batch has local edits, the install is refused without
   `--force` and the refusal names the changed files. Nothing is written.

### Decisions made while planning (flag any you disagree with)

- **An installed extension with no `.quarto-source.yml`** (installed by Q1, copied by hand, or
  vendored in-tree) has no baseline, so edits cannot be detected. Requiring `--force` there
  would break every existing Q1 project. Proposal: proceed, add a note to the table output
  ("no install record; local changes cannot be detected"), and write a fresh
  `.quarto-source.yml` after the install.
- **No Change writes nothing.** Q1 re-copies everything even when versions are equal. We
  write nothing, so a second `q2 add` is a true no-op: it does not touch mtimes, and it leaves
  any local edits in place. `--force` with No Change reinstalls, which gives a way to reset a
  locally edited copy.
- **Local-source provenance** records the path as the user typed it, plus `kind`. It is
  informational only. A future `q2 update` decides what to do with a local source.
- **Path safety.** `<org>` and `<name>` come from untrusted input (the GitHub shorthand and the
  archive's directory names). Each must be a single safe path segment: no separators, no `.` or
  `..`, not empty. The directory removed on update is computed as
  `<root>/_extensions/[<org>/]<name>`, canonicalized, and asserted to be under the canonical
  `<root>/_extensions/` before any removal.

## Phases

### Phase 0: Tests

`crates/quarto/tests/integration/add.rs`, registered in `tests/integration/main.rs`, spawning
the real binary like `use_brand.rs` does:

- [x] local dir with `_extensions/<name>/` installs into `<project>/_extensions/<name>/`, and
      `.quarto-source.yml` lists every file with a correct sha256
- [x] local dir that *is* an `_extensions/` folder is accepted
- [x] local `.zip` and `.tar.gz` (wrapped in one top-level directory, and unwrapped)
- [x] cwd in a project subdirectory installs at the project root; outside a project it installs
      in cwd; `--dir` overrides both
- [x] a second identical add reports `[No Change]` and writes nothing (check mtimes)
- [x] a higher version reports `[Update] 1.0.0 -> 1.1.0` and replaces the directory, so stale
      files from the old version are gone
- [x] a lower version reports `[Revert]`
- [x] a locally edited installed copy refuses without `--force` and names the file; with
      `--force` it is replaced; an extra file counts as an edit
- [x] an installed copy with no `.quarto-source.yml` updates with the "cannot detect" note
- [x] `--dry-run` prints the table and writes nothing; `--dry-run --force` is refused
- [x] `--json` emits one result object on stdout and never prompts
- [x] a remote source without `--trust` under `--no-prompt` is refused before any request
- [x] invalid sources leave nothing written: no `_extensions/`, no readable extension, or a
      hostile directory name
- [x] end to end: install a fixture custom project type into a website, set
      `project.type: <it>`, and `q2 render` succeeds

Unit tests: the `coerce` cases, the action matrix (every Q1 branch), the type-column strings,
segment validation, and edit detection (modified / missing / extra).

Remote: tests go through the `SourceFetch` seam (scripted fake), plus one localhost-server test
for `q2 add <http-url>`, following `quarto-source-fetch/tests/integration/network.rs`. No
real github.com traffic.

### Phase 1: quarto-source-fetch

- [x] Add `owner: Option<String>` to `RemoteTarget`, set from the org for GitHub shorthand and
      `None` for URLs. Update the existing constructors and tests.
- [x] Add an `_extensions` locator (Q1 `install.ts:283-300, 609-621`): the root `_extensions/`,
      one level down, or the path itself when it is named `_extensions`.

### Phase 2: Staging, comparison, records (`crates/quarto/src/commands/add/`)

- [x] Add `semver` to `[workspace.dependencies]`, plus the `coerce` helper.
- [x] Read staged extensions with `read_extension_with_org`, and installed ones from the target
      `_extensions/`.
- [x] Action rows plus the type column.
- [x] `.quarto-source.yml` read/write (serde struct; the generated-file comment is written as a
      literal header) and the sha256 tree walk via `sha2`.
- [x] Edit detection against the record.

### Phase 3: Install writer

- [x] A dedicated replace-directory primitive, kept separate from `common/writer.rs` so the
      shared writer keeps its "never overwrites" promise. For each extension: copy into
      `<root>/_extensions/.q2-staging-<rand>/`, write `.quarto-source.yml` into the staged copy,
      remove the old directory (after the path-safety asserts), then rename the staged copy
      into place. Clean up staging on every exit path.
- [x] Windows: remove before rename (rename over a non-empty directory fails there), and clear
      read-only attributes before removal.

### Phase 4: CLI

- [x] Replace `Commands::Add { extension }` with the full flag set and dispatch to
      `commands::add::execute(AddArgs)`.
- [x] Order: resolve target, then trust gate, then fetch, then stage and validate, then compute
      rows and edit detection, then print the table, then confirm (default **yes**, as in Q1;
      skipped under `--no-prompt`/`--json`), then install.
- [x] Human output (Q1-shaped table) and `--json` (versioned result object with per-extension
      action/from/to/path), sharing the `fail`/`emit_warning` shape from `use_cmd`.

### Phase 5: Docs and parity

- [x] User docs page for `q2 add`, stating the deviations from Q1 (trust, root, provenance,
      `--force`).
- [x] Run the product-doc-theme fixture with `q2 add posit-dev/product-doc-theme --trust`
      (real network, manual), then render with `type: posit-docs`. Record the result in the
      strand.
- [x] `cargo xtask verify --skip-hub-build`: 17244 Rust tests pass. Only the vitest suite of `quarto-hub-mcp`
      failed: 1–4 random tests hit the 5 s timeout, a different set each run, and all 575 pass
      with `--testTimeout=30000`. Its WASM bundle predates this branch, so the failures are
      unrelated.

## Implementation notes (2026-10-22)

Where the implementation differs from the phases above, or goes beyond them:

- **Tests were written alongside the code, not before it.** To make up for that, the three key
  gates were checked by mutation: disabling the edit check, the trust gate, or the replace
  (merging instead) each turns its integration test red.
- **No `installed:` timestamp in the record.** It churned on every reinstall for projects that
  commit `_extensions/`, and nothing reads it. This was already flagged as an easy cut under
  Risks.
- **Owner from a GitHub archive URL**, matching Q1's `kGithubArchiveUrlSource`:
  `https://github.com/<org>/<repo>/archive/...` installs under `<org>/`. Other URLs have no
  owner.
- **Type column** lists `project` and `engine(s)` after Q1's format/shortcode/filter. These
  are things q2 extensions contribute, and they matter when deciding whether to trust an
  extension.
- **`quarto_core::extension::read_extensions_dir`** is new and public: Q1's
  `readExtensions(dir)`, which wraps the existing private scanner.
- **Two real-GitHub bugs in `quarto-source-fetch`**, found by the Phase 5 run. They also
  affected `q2 use brand`:
  1. *pax global header.* Every `git archive` tarball starts with a `pax_global_header` entry,
     and the extractor refused it as a "metadata record". So **no** GitHub source could be
     fetched. It is now skipped and charged to the entry and byte budgets. Regression test:
     `a_pax_global_header_is_skipped_not_refused`.
  2. *Symlinks.* product-doc-theme ships `example/_extensions -> ../_extensions`, and the
     extractor refused any archive containing a link. **User decision (2026-10-22):** links are
     never created. They are skipped (their names are still validated and counted) and reported
     in `ExtractSummary::skipped_links` / `FetchedSource::skipped_links`. `q2 add` refuses
     only when a skipped link is inside an extension it would install. `q2 use brand` warns
     when one is inside the brand directory. Escape safety is unchanged: with no link on disk,
     a later entry "through" it is a plain path inside the destination.
- **Phase 5 parity run, done against GitHub.** `q2 add posit-dev/product-doc-theme --trust`
  installs `_extensions/posit-dev/posit-docs` (8.1.4, `(project)`). The example site renders
  with `type: posit-docs` (5 of 6 pages; `grid.qmd` is the known extension-side F5 parse
  error). A second add reports No Change.
- **Docs:** an "Installing extensions" section in `docs/guides/authoring/extensions.qmd`;
  Q-16-1, Q-16-3, and Q-5-17 now suggest `q2 add` instead of `quarto add`.

## Risks / tradeoffs

- **Directory removal** is the dangerous step. It is covered by the path-safety asserts above
  and by a test that a hostile `name` (`..`, `a/b`) is refused before staging.
- **Future `q2 update` / `q2 remove`** (both still stubs) should read `.quarto-source.yml`.
  Keep its schema versioned and documented in the docs page.
- **Committed `_extensions/`**: users who commit extensions will also commit
  `.quarto-source.yml`. That is intended (the provenance travels with the project), but its
  `installed:` timestamp churns on reinstall. Dropping the timestamp is an easy cut if that
  proves noisy.
- The parent assessment plan is not on `main`.
