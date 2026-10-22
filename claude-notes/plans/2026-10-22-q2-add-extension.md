---
title: 'Implement q2 add: install an extension from GitHub, URL, or local path (bd-rfz4e3bw)'
date: 2026-10-22
description: 'q2 add is a stub; implement Quarto 1''s extension install on top of the quarto-source-fetch machinery that q2 use brand already ships.'
status: draft  # Investigation — pending design alignment with user; do not implement before the go-ahead
braid:
  strand: bd-rfz4e3bw
---

**Branch:** `braid/bd-rfz4e3bw-q2-add-extension` (topic branch in the main checkout, based on `main` @ `47337c56b`)
**Do not start implementation until the user gives the go-ahead.**

## Triage verdict

**Ready to design.** Most of what `q2 add` needs already exists: target parsing, fetching, safe
extraction, extension reading, and organized `_extensions/<org>/<name>/` discovery. The work is
an install planner, a new overwrite-capable writer path, and the Q1-style confirmation table.
Several Q1 behaviours conflict with conventions that `q2 use brand` set (trust, overwrite,
install root). These need a decision before Phase 0 (questions below).

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

## Proposed phases (draft)

- **Phase 0: Tests first.** CLI integration tests in `crates/quarto/tests/integration/add.rs`,
  registered in `main.rs`: local dir, local dir that is itself `_extensions`, local `.zip`
  and `.tar.gz`, a remote fetch through a fake or localhost fetcher, org placement, a second
  add reporting No Change, an update replacing stale files, an invalid source (no
  `_extensions`, or an unreadable `_extension.yml`) refused with nothing written, and an
  end-to-end check that `type: <custom>` renders after `add`.
- **Phase 1: source-fetch changes.** Add `owner` to `RemoteTarget` and add `_extensions`
  location logic (a shared helper in `quarto-source-fetch`, or in the add command).
- **Phase 2: staging and validation.** Read the staged extensions with
  `read_extension_with_org`, compare them with the installed ones, and build the
  action/version/type rows. Decide on version comparison (semver dependency or a hand-written
  comparison).
- **Phase 3: install writer.** Stage each extension inside the install root, remove the old
  directory, and rename the new one in. This will be a new primitive in `common/writer.rs` or
  a dedicated `add` writer, with `--dry-run` if wanted.
- **Phase 4: CLI surface.** Add flags to `Commands::Add` (`--no-prompt`, plus whatever the
  answers below add), human output, `--json`, and prompting through `Prompter`.
- **Phase 5: docs and parity check.** Update the Quarto 2 docs page for `add`, then run the
  product-doc-theme fixture (strand § Fixture) through `q2 add posit-dev/product-doc-theme`
  and render.

## Open design questions for the user

1. **Trust gate.** Q1 asks "Do you trust the authors…" with default *yes*, and `--no-prompt`
   installs remote code silently. `q2 use brand` defaults to *no* and fails closed without a
   terminal unless `--trust` is passed. Extensions run Lua at render time, so the stakes are
   higher than for a brand. Should `q2 add` follow `q2 use brand` (default no, `--trust`
   needed when it cannot prompt, `--no-prompt` alone refuses a remote source), or follow Q1?
2. **Install root.** Q1 installs into cwd, and only offers the project root when the user
   answers an interactive question. The strand asks for the project root. Proposal: install
   into the project root when inside a project, otherwise into cwd, with no question. Is that
   deviation from Q1 OK? Should there be a `--dir` override?
3. **Provenance (from the strand).** Q1 records nothing. Should `q2 add` record
   `source`/`ref` so a future `q2 update` can work? If yes, should it go in a sibling file
   (e.g. `_extensions/<org>/<name>/.quarto-source.yml`) to keep the author's
   `_extension.yml` untouched, or into `_extension.yml` itself?
4. **Version comparison.** Should we take a `semver` workspace dependency and match Q1's
   Install/Update/Revert/No Change exactly, or use a lighter comparison? For two unversioned
   copies, should we keep Q1's "always Update", or compare file contents and report
   No Change when they are identical (this would satisfy "a second add reports No Change"
   for unversioned extensions, such as the product-doc-theme example if it has no version)?
5. **Flags beyond Q1.** Should `q2 add` get `--dry-run` and `--json` for parity with
   `q2 use brand` (MCP/agent use), or only Q1's `--no-prompt` for now? `--embed` stays out of
   scope as the strand proposes. Is it OK to drop the "open docs in browser" prompt?
6. **Overwrite safety.** An update deletes the installed extension directory wholesale (Q1
   does the same). Should we refuse, or require `--force`, when the installed copy has local
   edits? This is only detectable if we record provenance or a content hash (see question 3).
   Or keep Q1's behaviour and rely on the confirmation table?

## Risks / tradeoffs (draft)

- The overwrite path is new territory for the shared writer, whose doc comment promises it never
  overwrites. A separate primitive keeps that promise intact for `create`/`use`.
- Removing a directory on update needs the care described in the user's rm-rf memory:
  canonicalize, assert the path is inside `<root>/_extensions/`, and never build it from
  unchecked components. The org and name come from the source (an untrusted archive), so they
  must be validated as single safe path segments.
- Windows: renaming over a non-empty directory fails, so do remove-then-rename and handle
  read-only files.
- The parent assessment plan is not on `main`. If that branch never merges, the reference in
  this plan points to a branch, not a file.
