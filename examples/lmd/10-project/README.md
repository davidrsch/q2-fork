# Modules across a project

A project whose `_quarto.yml` autoloads two lmd modules with `lmd: modules:`:
`components/cards.lmd` at the project root (bound as `cards`) and the
`badges` extension's `init.lmd` (bound as `badges`). The page also uses an
explicit project-root `require("/components/deck")`, whose module requires
its neighbour, and `quarto.include` twice: a parameterised `.lmd` partial
and a `/`-rooted markdown partial.

This example backs the lmd page of the Quarto 2 documentation
(`docs/guides/authoring/lmd.qmd`).

## Run it

From the repository root:

```bash
cargo run --bin q2 -- render examples/lmd/10-project
```

The output is `index.html` next to `index.qmd`.
