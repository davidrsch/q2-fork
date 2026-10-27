---
title: 'q2 add / q2 use brand: allow --trust with --dry-run (bd-cjqhcaz6)'
date: 2026-10-27
description: 'A non-interactive dry run of a remote source is impossible because --trust is rejected with --dry-run; allow the pairing in both commands while still rejecting --force with --dry-run.'
status: done  # Implemented 2026-10-27
braid:
  strand: bd-cjqhcaz6
---

**Branch:** `braid/bd-cjqhcaz6-trust-with-dry-run` (topic branch in the main checkout, based on `main` @ `85f19da7f`)
**Design questions resolved 2026-10-27; see Design decisions.**

Pre-flight `cargo xtask verify --skip-hub-build` is green at `85f19da7f`. The first run hit a one-off failure in `marimo_engine_e2e::sc8` (a uv client-cache rename race in `~/.cache/uv`). The test passed when run alone, and the full rerun passed.

## Triage verdict

**Ready to design.** The strand is fresh (filed today), the cause is exactly where it says, the fix is small and symmetric across two commands, and the only open points are wording and test-helper placement.

## Issue context

P2 bug, labels `cli`, `extensions`, filed 2026-10-27 by an agent session.

Without a terminal (`--no-prompt`, `--json`, CI, an agent shell), `q2 add <remote> --dry-run` and `q2 use brand <remote> --dry-run` cannot run:

- without `--trust`, the trust gate refuses ("There is no terminal to ask on … Re-run with --trust");
- with `--trust`, the flag guard refuses ("--trust cannot be combined with --dry-run … there is nothing for --trust to authorize").

The guard's rationale is right for `--force` (it only authorizes overwriting local state, which a dry run never does) but wrong for `--trust`: a dry run of a remote source still **downloads** the archive to build the `[Install]/[Update]/[No Change]` table (add) or the file plan (use brand), and the trust gate governs that download, not the write.

Proposed fix (from the strand): drop `--trust` from both guard loops, keep `--force` rejected, reword the no-prompter hint so it is correct in dry-run mode, update docs.

## Dependency graph

- **discovered-from**: bd-0i6fii0k (in progress) — the product-doc-theme (posit-docs) assessment. The session was installing `posit-dev/product-doc-theme@q2` into a downstream website port with q2 0.36.0 and hit the contradiction while trying to preview the install non-interactively.
- **related**: bd-rfz4e3bw (closed) — the `q2 add` implementation. That work copied the `q2 use brand` flag rule ("Same rule as `q2 use`") into `add`, which is how both commands ended up with the same guard.
- No `blocks` edges; nothing waits on this. Urgency is user-facing friction for agents and CI, not a blocker.

## What the code looks like today

All paths in the strand still exist with the described shape at `85f19da7f`:

- `crates/quarto/src/commands/add/mod.rs:56-71` — guard loop over `[("--force", args.force), ("--trust", args.trust)]`.
- `crates/quarto/src/commands/use_cmd/mod.rs:46-64` — identical loop in `execute_brand`, with a comment that frames both flags as "authorize something".
- `crates/quarto/src/commands/add/resolve.rs:343` `confirm_trust` — returns early on `req.trust`; with no prompter, refuses with hint "Re-run with --trust to confirm you trust the authors of this extension." Called from the `Target::Remote` arm (line 134) regardless of `dry_run`.
- `crates/quarto/src/commands/use_cmd/brand.rs:366` `confirm_trust` — same shape, hint "Re-run with --trust to confirm you trust this source." Called from `resolve_brand_files` (line 285) before `fetch_into`, regardless of `dry_run`.
- `confirm_install` (`add/resolve.rs:400`) already skips its "Would you like to continue?" prompt in dry-run mode, so allowing `--trust` introduces no second prompt.

Tests:

- `crates/quarto/tests/integration/use_brand.rs:446` `trust_with_dry_run_is_rejected` — runs `brand --trust --dry-run` with **no target** (starter brand) and asserts failure. After the fix this exact invocation would succeed (trust is a no-op for the starter brand), so the test must be rewritten, not just flipped.
- `crates/quarto/tests/integration/add.rs:432` `dry_run_shows_the_table_and_writes_nothing` checks only `--force --dry-run`.
- `add.rs:588` has a localhost `TestServer` (serves `/archive.tar.gz`, records request paths) used by `a_remote_source_without_trust_is_refused_before_any_request` and `a_trusted_archive_url_installs`. `use_brand.rs` has **no** HTTP server; its only remote test (`a_remote_target_is_refused_non_interactively_without_trust`, line 589) relies on refusal happening before any request. `q2 use brand` does accept archive URLs (`quarto_source_fetch::resolve_target`), so a localhost server works for it too.

Docs:

- `docs/guides/authoring/extensions.qmd:53` — "Remote sources must be trusted" bullet.
- `docs/guides/authoring/brand.qmd:1067-1097` — "Previewing changes" and "Trusting a remote source" sections; neither mentions the dry-run interaction. The "`--force` and `--trust` are different permissions" callout is a natural place to say why one combines with `--dry-run` and the other does not.

The symptom is reproducible by reading: no code path between the guard and `confirm_trust` could make the pairing work. No separate repro fixture was needed.

## Design decisions (2026-10-27, with the user)

1. **Hint wording: mode-aware.** The no-prompter refusal tells a dry-run caller to re-run with `--dry-run --trust`, and a normal caller to re-run with `--trust`. Cost is low: both `confirm_trust` functions already take `req`, and both `AddRequest` and `BrandRequest` carry `dry_run`, so no new parameter is threaded.
2. **Test server: shared helper.** `TestServer` and `tar_gz` move out of `add.rs` into a shared module under `crates/quarto/tests/integration/`, used by both `add.rs` and `use_brand.rs`.
3. **Consent for the fetch in every mode.** A dry run of a remote source still needs trust (prompt or `--trust`). Letting a dry run fetch without trust stays rejected.
4. **Interactive dry run keeps prompting.** With a terminal, a remote `--dry-run` still asks the trust question before downloading. The docs get a note explaining why a preview can ask a question.

## Phases

- **Phase 0 — Shared test helper.** Move `TestServer` and `tar_gz` (plus whatever `tar_gz` needs) from `tests/integration/add.rs` into a new module, e.g. `tests/integration/archive_server.rs`, registered alphabetically in `main.rs`. `add.rs` imports from it. No behavior change; `add` tests stay green.
- **Phase 1 — Failing tests.**
  - `add.rs`: `--no-prompt --dry-run --trust <server url>` succeeds, prints the `[Install]` table, leaves `_extensions/` absent, and the server saw exactly one `/archive.tar.gz` request.
  - `add.rs`: `--no-prompt --dry-run <server url>` (no `--trust`) is refused before any request, and the hint names `--dry-run --trust`. The existing non-dry-run refusal test checks that its hint does not mention `--dry-run`.
  - `use_brand.rs`: replace `trust_with_dry_run_is_rejected` with a remote `--trust --dry-run` against the shared server that previews and writes nothing (no brand files, `_quarto.yml` unchanged, one request).
  - `use_brand.rs`: `--no-prompt --dry-run <remote>` without `--trust` is refused before any request, with the dry-run hint.
  - Both: `--force --dry-run` stays rejected (add already covers it; add a `use brand` case if missing).
  - Unit tests in `add/resolve.rs` and `use_cmd/brand.rs` near the existing `confirm_trust` tests, for the two hint variants.
- **Phase 2 — Guards.** Reduce both loops in `add/mod.rs` and `use_cmd/mod.rs` to a single `--force` check. Rewrite the comments: `--trust` is allowed because it authorizes the fetch, which a dry run performs; `--force` is not because it authorizes an overwrite, which a dry run never does.
- **Phase 3 — Mode-aware hints.** In both `confirm_trust` functions, pick the re-run sentence from `req.dry_run`.
- **Phase 4 — Docs.**
  - `docs/guides/authoring/extensions.qmd:53`: a non-interactive dry run of a remote source needs `--trust`. Add the note that an interactive dry run asks the trust question, because previewing still downloads the source.
  - `docs/guides/authoring/brand.qmd`: the same two points in "Trusting a remote source" (and/or "Previewing changes"). Extend the "`--force` and `--trust` are different permissions" callout: `--trust` combines with `--dry-run`, `--force` does not.
- **Phase 5 — Verify.** `cargo xtask verify`. No changelog entry: the only changelog in the repo is `hub-client/changelog.md`.

## Implementation notes

All phases landed as planned. The shared helper is `crates/quarto/tests/integration/archive_server.rs`. The new and rewritten tests were observed failing before the guard and hint changes, and passing after. Docs also gained an example of the non-interactive form (`--dry-run --trust --no-prompt`).

## Risks / tradeoffs (draft)

- Low risk: the change removes a rejection; `confirm_trust` already honors `req.trust` independently of `dry_run`.
- The existing `trust_with_dry_run_is_rejected` test was deliberate, so the comments in both `mod.rs` files and the brand docs encode the old reasoning; all of it needs updating together or the two commands drift (their comments promise they stay in step).
- A remote dry run now does real network I/O with `--trust` in CI. That was already true interactively; nothing new is written to the project.
