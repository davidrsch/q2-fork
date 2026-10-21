---
title: 'Vendored dependencies: a checked registry, an `outdated` command, and an upgrade skill (bd-xm7l)'
date: 2026-10-21
description: 'Assessment of how the repo records and refreshes vendored third-party assets after PR #876, and a proposed design: a machine-readable registry that a lint keeps honest, an xtask that asks upstream what is newer, and a skill that turns the answer into strands.'
status: draft  # proposal for review; no implementation yet
braid:
  strand: bd-xm7l
  priority: P2
  labels: [deps, vendored]
---

**Strand:** bd-xm7l (open epic since 2026-05-04, "Audit non-cargo / vendored dependencies and expand upgrade skill"). **Branch:** `braid/bd-xm7l-vendored-deps-registry` (topic branch in the main checkout, off `main` @ `f6e950d29`). **Prior plan:** `2026-05-04-vendored-deps-audit.md` (its Phase 1 landed; Phases 2–3 never started). **Prior inventory:** `claude-notes/research/vendored-dependencies-inventory.md`. **Trigger:** PR #876 (bd-qr8ryv79) vendored `@preview/orange-book:0.7.1` into `resources/typst-packages/` to stop CDN flakes.

## The question

PR #876 fixed a flake by vendoring one more Typst package. The fix is right, but it shows a gap. Nothing in the repo answers three questions in one place:

1. **What do we vendor, and where?**
2. **Which version is each asset at, and what decides that version?**
3. **Is upstream ahead, and what would updating take?**

The goal of this work is a skill that does the periodic "check vendored packages for updates" chore. A skill is only as good as the data it reads, so most of this plan is about that data.

## Assessment

### An epic already exists, and its data is stale

bd-xm7l was filed on 2026-05-04 with an inventory doc and a plan. The plan's Phase 2 was "extend the upgrade skill with a vendored-audit phase". It never started. Since May the inventory has drifted a lot:

- **18 vendored assets are missing from it.** All of them were added after May: `resources/typst-packages/`, `resources/pandoc-filters/` (the quarto-cli Lua filter stack at `v1.11.3`), the two git subtrees in `resources/extension-subtrees/` (orange-book and julia-engine), reveal.js 6 `dist/` and its SCSS template, mermaid 11.12.0, the hephaestus Roboto fonts, Algolia autocomplete, headroom.js, clipboard.js, Bootstrap JS, the React ESM build, List.js, `resources/language/`, the highlight-style themes, `resources/formats/`, and the mitex crates in `crates/quarto-math/vendor/`.
- **One entry describes something that no longer exists.** Entry I (quarto-system-runtime JS bundles) was removed with deno_core (bd-3e3sam51).
- **Three download manifests aren't covered at all.** `resources/{pandoc-wasm,typst-wasm,pdfjs-viewer}.json` pin bytes that are fetched rather than committed.
- **Several CDN references aren't covered either.** KaTeX is pinned and guarded. `mathjax@3`, the algolia-insights script, and `@marimo-team/islands` are unpinned.

The cause is structural. The inventory is prose in `claude-notes/research/`, and nothing fails when it falls out of date. Adding a vendored asset never touches it, so nobody updates it.

### What already works, and should be kept

Per-asset provenance is often good. It's just scattered:

- **READMEs next to the bytes.** Most vendored directories have one, giving source, version, and update steps. Examples: `resources/typst-packages/README.md`, `resources/mermaid/README.md`, `resources/revealjs/README.md`, `crates/quarto-math/vendor/VENDORED.md`. These are the right place for *procedure*.
- **Guard tests for some assets.** Each one ties the vendored bytes to a declared version: `vendored_reveal_assets_match_npm_package`, `vendored_react_matches_build_info`, `bootstrap_js_version_matches_scss_readme`, `katex_cdn_version_matches_npm_pin`, mermaid's `vendored_bundle_matches_pinned_version`, and now `test_vendored_packages_cover_orange_book_template_imports`.
- **Refresh tooling for some assets.** `cargo xtask pull-extension-subtree`, `cargo xtask vendor-react`, `cargo xtask gen-math-spec`, and `scripts/fetch-pandoc-wasm.mjs`.
- **Some lint coverage.** `crates/xtask/src/lint/` already has `vendored_pandoc_filters` and `pandoc_pin`, so a vendoring lint has an obvious home.

What's missing is an **index** over all of this that a machine can read, plus a check that the index matches the tree.

### "Update available" means different things for different assets

This is the main design constraint. The typst-packages directory alone shows three different policies:

- **fontawesome, marginalia, octique, showybox, theorion** follow **quarto-cli `v1.11.3`**. If Typst Universe has showybox 2.0.5, that is *not* by itself a reason to update. The right question is whether quarto-cli moved.
- **orange-book 0.7.1** follows **the orange-book extension subtree**. Its `typst-show.typ` imports exactly `@preview/orange-book:0.7.1`. Vendoring 0.7.2 would do nothing until the subtree's import changes. Then it *must* be re-copied, or renders go back to the network.
- **The subtree itself** follows `quarto-ext/orange-book` `main`. Its README says "tag 0.2.0", but the actual pin is the split trailer `2b59b7672`.

The same pattern shows up elsewhere. About eight assets follow the quarto-cli tag (pandoc-filters, typst-packages, language, highlight styles, formats, Bootstrap SCSS/JS/Icons). reveal.js follows the npm lockfile. hephaestus fonts follow the hephaestus crate version. Some assets have no leader and follow their own upstream: mermaid, List.js, the mitex crates, the subtrees.

A naive "compare with latest" check would therefore produce mostly noise: one strand per quarto-cli-derived asset whenever a Typst package bumps a patch. The registry has to record **what each asset follows**. The check then reports one actionable item per *leader*, not one per asset.

### Hermeticity is a second, related invariant

PR #876's bug wasn't really "outdated". It was "embedded content imports a package we don't ship". The new test pins that invariant for the orange-book template only. The general invariant is: every `@preview/<name>:<version>` import in any embedded `.typ` must be in `resources/typst-packages/`. That covers `resources/pandoc-filters/typst-template/`, the built-in extensions, and the extension subtrees\' `_extensions/`. It's cheap to check offline. It also catches the update case: when a subtree refresh bumps an import, the test fails until the package is re-vendored.

## Proposed design

There are four pieces, each useful without the ones after it.

### 1. A registry: `resources/vendored.toml`

This is a single TOML file and the source of truth for *what* is vendored and *what it follows*. Each asset's README stays the source of truth for *how* to refresh it. Paths are repo-relative, so the registry can also list assets outside `resources/` (mitex, `hub-client/public/reveal-menu`).

```toml
[[group]]
id = "quarto-cli"
upstream = { kind = "git-tags", repo = "https://github.com/quarto-dev/quarto-cli" }
version = "v1.11.3"
procedure = "claude-notes/instructions/quarto-cli-resync.md"   # new; one walkthrough for the whole family

[[asset]]
id = "typst-pkg-showybox"
paths = ["resources/typst-packages/packages/preview/showybox/2.0.4"]
follows = "group:quarto-cli"
version = "2.0.4"
version_source = { kind = "path-segment" }                       # the lint reads the version from here
procedure = "resources/typst-packages/README.md#updating"
license = "MIT"

[[asset]]
id = "typst-pkg-orange-book"
paths = ["resources/typst-packages/packages/preview/orange-book/0.7.1"]
follows = "asset:subtree-orange-book"       # its version is whatever typst-show.typ imports
version = "0.7.1"
version_source = { kind = "path-segment" }
copy_of = "resources/extension-subtrees/orange-book/_extensions/orange-book/typst/packages/preview/orange-book/0.7.1"
guards = ["quarto-core typst_compile::tests::test_vendored_packages_cover_orange_book_template_imports"]
license = "MIT-0"

[[asset]]
id = "subtree-orange-book"
paths = ["resources/extension-subtrees/orange-book"]
upstream = { kind = "git-branch", repo = "https://github.com/quarto-ext/orange-book", ref = "main" }
version = "2b59b7672"
version_source = { kind = "subtree-split" }
procedure = "resources/extension-subtrees/README.md"
license = "MIT-0"

[[asset]]
id = "mermaid"
paths = ["resources/mermaid"]
upstream = { kind = "npm", package = "mermaid" }
version = "11.12.0"
version_source = { kind = "file-regex", file = "crates/quarto-core/src/transforms/mermaid.rs", regex = 'MERMAID_VERSION: &str = "([^"]+)"' }
procedure = "resources/mermaid/README.md"
license = "MIT"
```

Key fields:

- **`follows`** is one of `upstream` (the default when `upstream` is set), `group:<id>`, `asset:<id>`, `frozen`, or `native`. `frozen` covers test fixtures such as the CSL fixtures and the OOXML XSDs: they are recorded but never reported as outdated. `native` covers repo-owned code that discovery keeps flagging, such as the tree-sitter-qmd grammar and the Q1 JS ports. Those entries exist only so discovery stops flagging them.
- **`version_source`** says where the lint reads the *actual* version from: `path-segment`, `file-regex`, `toml-key`, `json-key`, or `subtree-split`. This is what keeps the registry honest. The version also lives in a README, a banner, a constant, or a directory name, and the lint checks that the two agree. So the registry can't drift silently the way the May inventory did.
- **`upstream.kind`** is one of `typst-universe`, `npm`, `crates-io`, `git-tags`, `git-branch`, or `github-release`. Each kind maps to one upstream query in piece 3.
- **`copy_of`** records an in-repo duplicate, such as orange-book in both the subtree and typst-packages. The lint checks that the two copies are byte-identical.

The prose inventory in `claude-notes/research/` would be retired. Its discovery recipes move into the skill, and its entries move into the registry. The research doc gets a short header pointing to the registry, so old links still resolve.

**Why TOML in `resources/` and not YAML, or `.claude/`.** xtask already depends on `serde`. `resources/` is where most of the bytes live and where a contributor adding an asset is already working. A `.claude/` location would hide it from people who aren't using Claude. See open question 1.

### 2. Offline checks: `cargo xtask lint` rule `vendored-registry`, plus a generalized hermeticity test

These run in CI on every PR, with no network access.

- **Consistency.** For every asset: each `paths` entry exists, `version_source` yields exactly `version`, each `copy_of` pair is byte-identical, every `follows` target resolves, and there are no cycles.
- **Coverage.** Every `include_dir!` / `include_str!` / `include_bytes!` whose target is under `resources/` must fall under some registry asset's `paths`. So must every committed file under `hub-client/public/`, and every `crates/*/vendor/` directory. That covers both `native` entries and real vendored ones. An uncovered target fails with a message naming the file and pointing at the registry. **This is what answers "what do we vendor and where" permanently.** You can't add a vendored asset without registering it.
- **Typst hermeticity (a test in quarto-core, not a lint).** Generalize the PR #876 test: walk every embedded tree that can contribute `.typ` source, collect all `@preview/<name>:<version>` imports, and assert that `gather_packages` stages them with `downloaded == 0`. The orange-book-specific test becomes one case of this one, or is removed in its favour.

### 3. Online check: `cargo xtask vendored outdated [--json] [--id <id>] [--group <id>]`

This is deterministic and scriptable, and it is the only piece that needs the network. It reads the registry, groups assets by leader, queries each leader's upstream once, and prints current → latest with the policy applied:

```
LEADER                 CURRENT     LATEST      FOLLOWERS
group:quarto-cli       v1.11.3     v1.11.5     typst-pkg-{fontawesome,marginalia,octique,showybox,theorion}, pandoc-filters, language, …
asset:subtree-orange-book  2b59b7672  9c01d3e…  typst-pkg-orange-book (re-copy if typst-show.typ import changed)
asset:mermaid          11.12.0     11.13.1     —
asset:list-js          (unknown)   2.3.1       —   ⚠ registry gap
```

Implementation: xtask has no HTTP client. Rather than add one, it shells out to tools that every dev machine and CI image already has: `git ls-remote` for `git-tags` and `git-branch`, and `curl` for the Typst Universe `index.json`, the npm registry, crates.io (with a User-Agent), and GitHub releases. A failed query is reported per leader and never aborts the run, as the May plan required. `--json` output is what the skill reads.

### 4. The skill: `/upgrade-vendored-deps`

This sits alongside `upgrade-cargo-deps` and follows the same shape so the two feel the same to use. It would live in `.claude/skills/upgrade-vendored-deps/SKILL.md`.

1. **Pre-flight.** Clean tree, `cargo xtask lint` green. A failing registry lint is reported first, since everything after it depends on the registry.
2. **Discovery sweep.** Run the coverage lint, plus the softer heuristics the lint can't encode: provenance headers, new `package.json` files, CDN URLs in Rust string literals. New candidates get proposed registry entries. They are not silently added.
3. **Survey.** `cargo xtask vendored outdated --json`.
4. **Reconcile prior strands.** Close `Vendored:` chore strands whose leader is now current. This mirrors cargo's step 2b, and it's the only step that shrinks the backlog.
5. **File strands.** One `chore` strand per outdated *leader* (`Vendored: quarto-cli v1.11.3 → v1.11.5`), labelled `deps,vendored`, P3. Each strand lists the followers and links the procedure.
6. **Update `last_reviewed`** in the registry for every leader that was checked successfully. Then write `claude-notes/plans/YYYY-MM-DD-vendored-survey.md`, commit both on a topic branch, and report.
7. **Optional, on request only: `--apply <leader>`.** Run that leader's procedure in a worktree, run the followers\' guards plus `cargo xtask verify`, and stop for review. v1 of the skill would ship without this. Vendored refreshes like quarto-cli resyncs and reveal.js bumps usually need judgement: local patches, parity snapshots, visual checks. See open question 3.

## Work items

### Phase 1 — registry and migration

- [ ] Define the registry schema, as a serde struct in `crates/xtask/src/vendored/registry.rs`, with a short schema section in the registry file's header comment.
- [ ] Populate `resources/vendored.toml` with every asset from the 2026-10-21 survey (about 35 entries across the groups `quarto-cli`, `reveal.js`, `bootstrap`, and `hephaestus`), plus `native` and `frozen` entries for the known false positives.
- [ ] Close the gaps the survey found: record List.js's version, add `resources/formats/README.md`, and fix the orange-book subtree README's "tag 0.2.0" wording to match the actual `main` + split-trailer pin.
- [ ] Retire `claude-notes/research/vendored-dependencies-inventory.md`: replace its body with a pointer to the registry and keep the dead-scaffolding guidance, which is still useful.
- [ ] Mention the registry in `CLAUDE.md` § External Sources Policy: "adding a vendored asset? register it in `resources/vendored.toml`".

### Phase 2 — offline checks (TDD: each check first fails on a deliberately broken fixture registry)

- [ ] `vendored-registry` lint: consistency checks (paths, `version_source`, `copy_of`, `follows`).
- [ ] `vendored-registry` lint: coverage over embed-macro targets, `hub-client/public/`, and `crates/*/vendor/`.
- [ ] Generalized typst hermeticity test over all embedded `.typ` sources.

### Phase 3 — `cargo xtask vendored outdated`

- [ ] Upstream query per kind (`git ls-remote`, `curl`), each with a recorded-response unit test so CI never hits the network.
- [ ] Leader grouping, table output, and `--json` output.
- [ ] First real run, done by hand, with the results recorded in this plan.

### Phase 4 — the skill

- [ ] `.claude/skills/upgrade-vendored-deps/SKILL.md` (steps 1–6 above).
- [ ] First run via the skill. Refine it from that experience, then close bd-xm7l or narrow it to the `--apply` follow-up.

### Later / optional

- [ ] `--apply <leader>` mode.
- [ ] A scheduled GitHub workflow that runs `outdated --json` weekly and opens an issue when a leader moves. Without it, the skill stays on demand, like the cargo one.
- [ ] Pin `mathjax@3` and the other unpinned CDN URLs, recording them as `cdn` assets. This is a separate strand: pinning them changes behaviour, while the rest of this plan only adds bookkeeping.

## Open questions for review

1. **Registry location and format.** I proposed `resources/vendored.toml`. Alternatives are a repo-root `VENDORED.toml` (more visible, but adds root clutter) or YAML (matches `_quarto.yml` habits, but xtask parses TOML more naturally).
2. **Scope.** Should the registry cover the *download manifests* (`pandoc-wasm.json`, `typst-wasm.json`, `pdfjs-viewer.json`) and the *CDN URLs*? I lean yes for all three manifests: each records an upstream version, and pandoc-wasm already has a bump runbook. CDN URLs would be recorded but not enforced until they're pinned. The vendored Rust crates (`lua-src-wasm`, `wasm-bindgen-futures-patch`) already have a home in `upgrade-cargo-deps/PINS.md`. I'd reference them from the registry rather than duplicate them.
3. **Survey-only vs. apply.** Should v1 of the skill only file strands, as above, or should it also auto-apply a narrow "mechanical" class? An example of that class: a `copy_of` follower whose leader moved, where the re-copy is byte-for-byte and the guard tests decide.
4. **One skill or two.** Should this be a sibling `upgrade-vendored-deps` skill, or a phase inside `upgrade-cargo-deps`? The May plan leaned towards a phase. I lean towards a sibling: the two have different failure modes, different outputs (no lockfile commit here), and a different natural cadence (quarto-cli releases versus fortnightly cargo). A thin wrapper can run both later if that turns out to be wanted.
5. **Strand bookkeeping.** I've reused bd-xm7l as the home for this work rather than filing a new strand, and this plan supersedes the May plan's Phases 2–3. Would you rather have a fresh child strand per phase under bd-xm7l?
