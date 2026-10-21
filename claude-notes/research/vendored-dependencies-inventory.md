# Vendored / non-cargo dependency inventory (retired)

> **Superseded on 2026-10-21 (bd-xm7l).** This prose inventory was written on
> 2026-05-04 and drifted badly: by October it missed 18 vendored assets and
> described one that no longer existed. Its replacement is the machine-checked
> registry **`resources/vendored.toml`**, kept honest by the
> `vendored-registry` lint and surveyed by `cargo xtask vendored outdated` and
> the `/upgrade-vendored-deps` skill. Design:
> `claude-notes/plans/2026-10-21-vendored-deps-registry-and-skill.md`.
>
> - The per-asset entries (A–L) now live in the registry, with versions the
>   lint checks against the tree. Entry I (quarto-system-runtime JS bundles)
>   was removed with deno_core (bd-3e3sam51) and has no successor.
> - The discovery strategies moved into the skill (step 2), and the coverage
>   half of them is now enforced by the lint.
> - The section below is kept because it is still the right guidance when a
>   sweep finds dead fork scaffolding. The full May text is in git history
>   (`git log -- claude-notes/research/vendored-dependencies-inventory.md`).

## When discovery turns up *dead* scaffolding

A separate, important outcome of a vendored audit: discovery may
find files that *look* vendored but are actually dead weight
inherited from an old upstream fork — multi-language packaging
scaffolding for languages we don't build (`package.json`,
`setup.py`, `pyproject.toml`, `go.mod`, `Package.swift`,
`binding.gyp`, `CMakeLists.txt`, `Makefile`,
`bindings/{go,node,python,swift}/`, etc.) where the Rust crate's
`Cargo.toml` `include` list, `build.rs`, and any active workflow
make no reference to them.

**Delete it.** Leaving dead scaffolding in place misleads the
*next* audit run into re-classifying the crate as vendored, and
each cycle wastes a session on the same triage. Cleanup is in
scope for the audit, not over-stepping.

**Worked example — bd-7co9 (2026-05-04).** The
`crates/tree-sitter-qmd/` crate originated as a fork of MDeiml's
`tree-sitter-markdown`, but has been developed independently for a
long time. The first audit pass misclassified it as "vendored"
(see *Note on `tree-sitter-qmd`\'s stale "fork" framing* under
entry H below).

The cleanup landed in two commits on
`.worktrees/7co9-fork-framing-cleanup`:

1. **Narrow metadata fixes** (the explicit issue scope): rewrite
   `README.md` to drop the "fork of" lede, rewrite `tree-sitter.json`
   metadata, delete `package.json`/`package-lock.json` and the
   verbatim upstream `README.tree-sitter-md.md`.
2. **Broad scaffolding deletion** (follow-up after user
   confirmation): delete `binding.gyp`, `CMakeLists.txt`,
   `CONTRIBUTING.md`, `go.mod`, `Makefile`, `Package.{swift,resolved}`,
   `pyproject.toml`, `setup.py`, `bindings/{go,node,python,swift}/`,
   `scripts/`, `common/common.mak`, plus a parallel cleanup under
   `tree-sitter-markdown/`.

Files kept: `LICENSE` (preserves the original MIT copyright as
required), `bindings/rust/`, `tree-sitter.json` (used by the
`tree-sitter` CLI), `common/common.js` and
`common/html_entities.json` (referenced by `Cargo.toml`\'s
`include`), the grammar/queries/sources, and developer utilities
still referenced from source (e.g.
`tree-sitter-markdown/scripts/unicode-ranges.py` is mentioned in a
`grammar.js` comment).

Verification both commits: `cargo xtask verify --skip-hub-build
--skip-hub-tests` and `cargo xtask lint` (`--skip-hub-*` is fine
when the change is purely scaffolding outside any include path).
