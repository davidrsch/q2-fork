---
description: Survey the repo's vendored third-party assets (everything registered in `resources/vendored.toml` — quarto-cli copies, Typst packages and extension subtrees, Bootstrap, reveal.js, mermaid, fetched WASM manifests, CDN URLs, vendored Rust crates) for upstream updates, reconcile previously-filed `Vendored:` strands, file one strand per upstream that moved, and write a survey plan doc. Survey-only — it never applies an update. Use when the user says "check vendored deps", "survey vendored packages", "are our vendored assets out of date", "do the vendored upgrade survey", asks whether quarto-cli / Typst / reveal.js copies need refreshing, or asks whether open `Vendored:` strands are still real. Sibling of `upgrade-cargo-deps`; runs on demand only.
---

# upgrade-vendored-deps Skill

This skill turns "is anything we vendor behind upstream?" into one
review-friendly artifact. It **does not change any vendored bytes**. It
produces:

1. A plan doc at `claude-notes/plans/YYYY-MM-DD-vendored-survey.md`.
2. One braid strand per upstream that moved (per *group*, see below), each
   carrying enough context for an agent to plan the refresh.
3. A reconciled `Vendored:` strand queue (step 4) — the only step that
   shrinks the backlog.
4. Updated `last_reviewed` dates in `resources/vendored.toml`.

Deciding *how* to refresh an asset is deliberately left to a later session
working one strand: vendored refreshes need judgement (local patches, parity
snapshots, coupled pins) that a survey should not guess at.

## When to use

- "check vendored deps", "vendored upgrade survey", "are our vendored
  packages up to date?", "did quarto-cli change anything we copied?"
- A quarto-cli, Typst, reveal.js or pandoc release just shipped.
- The most recent `claude-notes/plans/*-vendored-survey.md` is more than a
  month or so old and the user asks for a fresh one.
- "Are the open `Vendored:` strands still real?" — run **step 4 only**.

**Do not** use for Cargo crates (`upgrade-cargo-deps`), npm workspace
dependencies other than the pins the registry records, or for actually
performing a refresh (work the filed strand instead).

## Background: the registry

`resources/vendored.toml` is the source of truth for *what* is vendored,
*where*, *at which version*, and *what decides that version*; its header
documents every field. Two facts drive this skill:

- **`policy`** — only `track` assets are compared with upstream. `follow`
  assets move with the asset they follow (the survey lists them as
  *followers* of their leader). `frozen`, `native` and `external` assets are
  never queried.
- **`notes`** — free-form context written *for you*: local patches, coupled
  pins, and what "newer" does *not* mean for that asset (e.g. Typst packages
  follow quarto-cli, not Typst Universe; List.js is Quarto 1's patched build,
  not npm's). Read an asset's notes before saying anything about it.

The `vendored-registry` lint (part of `cargo xtask lint`) keeps the registry
consistent with the tree, so the survey can trust it.

## Steps

### 1. Pre-flight

```bash
git status --short          # must be clean, or coordinate first
cargo xtask lint            # vendored-registry must pass
```

If the lint fails on `vendored-registry`, **stop and report**: the survey
reads the registry, so a stale registry means a wrong survey. Fixing the
registry (e.g. a version that was bumped without updating the registry) is
a reasonable first commit on the survey branch, if the user agrees.

Create the branch (the user prefers a topic branch in the main checkout when
working solo; use `cargo xtask create-worktree` only if another session is
active in this checkout):

```bash
git switch -c vendored-survey-YYYY-MM-DD main
```

### 2. Discovery sweep (what the lint cannot see)

The lint guarantees every file under `resources/`, `hub-client/public/` and
`crates/*/vendor/` is registered. Sweep the places it does not cover and
report any third-party content that is not in the registry:

```bash
# CDN URLs in Rust string literals (each should be a `cdn` group asset)
grep -rnoE 'https://(cdn\.jsdelivr\.net|unpkg\.com|cdnjs\.cloudflare\.com)/[^"\\ )]+' \
  crates --include='*.rs' | grep -v '/tests/'

# Compile-time embeds outside the coverage roots (one line per target).
# Expect ~60 repo-native hits; project templates are filtered out here.
grep -rnE 'include_(dir|str|bytes)!' crates --include='*.rs' \
  | grep -v '/tests/' | grep -vE '\.\./\.\./resources/|"\$CARGO_MANIFEST_DIR/\.\./\.\./resources' \
  | grep -oE '^[^:]+:[0-9]+:.*include_(dir|str|bytes)!\(\s*"[^"]+"' \
  | grep -v 'resources/templates/' \
  | sed -E 's/:[0-9]+:.*include_(dir|str|bytes)!\(\s*/ /' | sort -u

# Provenance headers anywhere in crates/
grep -rElI '^[#;/* ]*(Source|Vendored|Upstream|Copied from):' crates \
  --include='*.scm' --include='*.lua' --include='*.css' --include='*.scss' \
  --include='*.R' --include='*.html' --include='*.js'
```

Known repo-native hits (2026-10-21 survey; skip them): listing/feed
`templates/*.template`, the tree-sitter grammar query/node-types files,
`quarto-core/resources/{styles.css,js/quarto-color-mode.js}`, the
`qmd-syntax-helper` resources, xtask lint fixtures (`external-sources/…`),
build-time embed dirs (`$QUARTO_*_EMBED_DIR`), `quarto-math/spec/commands.json`,
`error_catalog.json` and `custom-node-schema.json`. Third-party embeds
outside the roots (CSL style, Pandoc template, knitr R) are already registered.

For each hit that is third-party and unregistered, **propose** a registry
entry in the plan doc (with the fields you can fill in confidently). Do not
add it silently. If the user agrees during the session, add it on the survey
branch and re-run the lint.

### 3. Survey

```bash
cargo xtask vendored outdated --json > "$SCRATCH/vendored-outdated.json"
cargo xtask vendored outdated            # the same, as a table, for the doc
```

`$SCRATCH` is your scratchpad directory. Needs the network (`curl`, `git`).
The first run clones blobless copies of the path-tracked repos
(quarto-cli, pandoc, …) into `target/vendored-cache/`; later runs fetch.

Each asset report has a `status`:

| status | meaning | action |
|---|---|---|
| `current` | nothing newer (for `git` + `paths`: no upstream commit touched the copied paths) | none; bump `last_reviewed` |
| `outdated` | upstream moved | candidate for a strand (step 5) |
| `unrecorded` | the registry has no version — an inventory gap | strand, if `detail` shows upstream activity worth checking |
| `unpinned` | deliberately floating (`*`) | none, unless the notes say otherwise |
| `manual` | pinned ref not resolvable upstream (e.g. a CSL `<updated>` stamp) | compare by hand using `detail` |
| `error` | the query failed | retry once; report in the doc, don't file |

`detail` lists, for path-tracked git upstreams, the upstream commits that
touched the copied paths since the pinned ref. That list *is* the change set
to review — read the subjects, and open interesting ones
(`git -C target/vendored-cache/<repo>.git show <sha> -- <path>`).

Judge each `outdated` row with its `notes`, not just the version delta. In
particular:

- **A new upstream tag is not by itself a reason to refresh** a path-tracked
  asset; the survey already filtered those to "did the copied paths change".
- **Major bumps** (mermaid 11 → 12, MathJax 3 → 4, algoliasearch 4 → 5) are
  code changes. Say so in the strand.
- **Coupled assets move together** (Bootstrap SCSS + JS; reveal.js dist + its
  npm pins + SCSS template; the pandoc-filters' `@preview` imports +
  `typst-packages`; the pandoc pin everywhere `pandoc-pin` checks). Name the
  coupling in the strand.
- **Quarto 1 parity**: assets copied from quarto-cli generally should not
  move *ahead* of what Quarto 1 ships (e.g. Bootstrap). The npm/CDN latest is
  informational for those.

### 4. Reconcile previously-filed `Vendored:` strands

**Run this before filing anything new.** It is also invocable alone.

```bash
braid list --json | jq -r '.[] | select(.title|test("^Vendored:")) |
  [.id,.status,.updated_at[0:10],.title] | @tsv'
```

For each open strand, compare what it asked for with today's registry and
survey:

- **superseded** — the asset (or group) is now `current`: someone did the
  refresh. Close it with evidence (`git log -1 --format='%h %s' -- <path>`).
- **obsolete** — the asset is gone from the registry. Close it, citing the
  commit that removed it.
- **still real, changed** — upstream moved further. Update the description
  with the new delta instead of filing a duplicate.
- **still real** — leave it; reference it from the plan instead of filing a
  new one.

### 5. File strands

One strand per **group** with at least one actionable row (`outdated`, or an
`unrecorded`/`manual` row whose `detail` shows real upstream activity); an
ungrouped asset gets its own strand. Skip groups where every row is
`current`/`unpinned`.

```bash
braid create "Vendored: <group title or asset id> — <short delta>" \
  -t chore -p 3 -l deps -l vendored --deps discovered-from:bd-xm7l \
  -d "Surfaced by the vendored survey YYYY-MM-DD (claude-notes/plans/YYYY-MM-DD-vendored-survey.md).

Assets: <id> <current> → <latest> (<status>), …
Followers that move with them: <ids>
Upstream changes under the copied paths: <the detail lines, trimmed>
Coupling / gotchas (from the registry notes): <one or two lines>
Procedure: <procedure file(s)>
Guards to run after the refresh: <guards>"
```

Put the substance in the description: the next agent should be able to plan
the refresh from the strand plus the registry, without re-running the
survey. `--deps discovered-from:bd-xm7l` links it to the vendored-deps epic.

### 6. Update `last_reviewed`

For every `track` asset whose status was `current`, `outdated`, `unpinned`
or `manual` (i.e. the query succeeded), set `last_reviewed = "YYYY-MM-DD"` in
`resources/vendored.toml`. Leave `error` rows untouched. Re-run
`cargo xtask lint` afterwards.

### 7. Write the plan doc

`claude-notes/plans/YYYY-MM-DD-vendored-survey.md`:

```markdown
---
title: 'Vendored dependency survey — YYYY-MM-DD'
date: YYYY-MM-DD
description: '<one sentence: how many upstreams moved, strands filed>'
braid:
  strand: bd-xm7l
  labels: [deps, vendored]
---

**Skill:** `.claude/skills/upgrade-vendored-deps/SKILL.md`
**Branch:** `vendored-survey-YYYY-MM-DD` (off `main` @ `<short-sha>`)
**Previous survey:** <link, or "none">

## TL;DR

- Upstreams that moved: N (strands: bd-…, …)
- Inventory gaps (unrecorded versions): K
- Reconciled prior strands: P closed, Q updated
- Unregistered third-party content found by discovery: R (proposals below)
- Query errors: E

## Survey

<the `vendored outdated` table, verbatim, in a fenced block>

## Strands filed

| Strand | Group / asset | Delta | Why it matters (one line) |
|---|---|---|---|

## Reconciled prior strands

| Strand | Verdict | Evidence |
|---|---|---|

## Discovery

<proposed registry entries for unregistered content, or "nothing new">

## Notes

<judgement calls; anything the registry notes should learn (update them)>
```

Before committing, run `q2 render --strict <file>` from `claude-notes/`
(escape `$`, possessive `s'`, bare `[x]`).

### 8. Commit and report

```bash
git add resources/vendored.toml claude-notes/plans/YYYY-MM-DD-vendored-survey.md
git commit -m "vendored: survey YYYY-MM-DD (bd-xm7l)"
```

braid strands need no commit. Report: the plan path, the strands filed and
closed, any discovery proposals awaiting a decision, and query errors. **Do
not push** without explicit approval (AGENTS.md GIT PUSH POLICY).

## Improving the registry as you go

When a survey teaches you something about an asset — a coupling, a local
patch, why a "newer" upstream is irrelevant — write it into that asset's
`notes`. The registry is how the next survey avoids rediscovering it. An
`unrecorded` version that you can establish (e.g. the bytes are identical to
a specific upstream commit) should be recorded, together with a
`file-contains` version source in the asset's README.

## Failure modes

- **`vendored outdated` errors on one row**: network or rate limits.
  Retry once with `--id <asset>`. The run never aborts on one failure.
- **`git clone` of a large repo is slow the first time**: expected (blobless
  clone). Later runs fetch incrementally.
- **A subtree row reports "registry drift"**: someone ran
  `pull-extension-subtree` without updating `version`. Fix the registry
  (the trailer is the truth) as part of the survey commit.
- **"no git-subtree-split trailer found"**: the checkout is shallow; run
  `git fetch --unshallow` or survey from a full clone.

## See also

- `resources/vendored.toml` (header documents every field)
- `crates/xtask/src/lint/vendored_registry.rs` (offline checks)
- `crates/xtask/src/vendored/outdated.rs` (upstream queries)
- `claude-notes/plans/2026-10-21-vendored-deps-registry-and-skill.md` (design)
- `.claude/skills/upgrade-cargo-deps/SKILL.md` (the Cargo sibling)
- Braid epic: bd-xm7l
