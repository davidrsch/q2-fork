# Issue #944 — alias differing from its page only by case (Q-5-25) is a hard error with no escape hatch

- **GitHub**: https://github.com/quarto-dev/q2/issues/944
- **Reporter**: @cscheid (Carlos Scheidegger), 2026-10-27, relaying @atheriel's Posit Connect docs port
- **Triage date**: 2026-10-28
- **Branch**: `issue-944` in the main checkout (topic branch, no worktree — per user request), based on `main` \@ `49cb226de` (v0.37.0)
- **Braid strand**: bd-ex8qvuwu
- **Scope**: the whole issue — it is a single report. Both shapes of `Q-5-25` are in scope (alias vs. rendered page, which is the reporter's case, and alias vs. alias), because one option must govern both.

## Summary

Reproduced exactly as described at `main` \@ `49cb226de`. A page at `r/index.qmd`
declaring `aliases: [/R/]` fails the render with `Q-5-25`, and there is no
configuration that lets it through. The behaviour is not a bug in the usual
sense: it implements decision 3 of
`claude-notes/plans/2026-08-12-aliases-redirect-stubs.md` ("case-insensitive
collisions — error, on every platform"), which was taken on the premise that
"no realistic scenario was identified in which an author wants a colliding
alias to survive the render". Connect's deployment is that scenario: the site
is served from S3, where `R/index.html` and `r/index.html` are distinct keys,
and the old `/R/` URLs must keep working. The issue therefore asks for a
deliberate, partial reversal of that decision.

**Direction (set by the user at triage):** keep the error as the default and
add an *affirmative* project option that declares the deployment target
case-sensitive. With the option set, the case-only pair is allowed and the
stub is written. The discrepancy with Quarto 1 gets documented. Filed as a
feature strand; fix scope below.

## Reproduction

Fixture: `claude-notes/issue-reports/944/repro/` (`_quarto.yml` website
project, `index.qmd`, and `r/index.qmd` with `aliases: [/R/]`). Binary built
from `main` \@ `49cb226de` by the pre-flight verify.

```bash
cd claude-notes/issue-reports/944/repro
../../../../target/debug/q2 render
```

Observed (macOS, APFS case-insensitive):

```text
Rendering project: .../issue-reports/944/repro (type: website)
Error: [Q-5-25] Alias differs only by case from a rendered page
   ╭─[ r/index.qmd:4:5 ]
 4 │   - /R/
   │     ─┬─
   │      ╰─── `/R/` in `r/index.qmd` resolves to `R/index.html`, which differs only in
   │           capitalization from `r/index.html` rendered by `r/index.qmd`. macOS and
   │           Windows treat those as one file.
───╯
ℹ Rename the alias so it cannot collide with the page?
```

Pages render (`_site/index.html`, `_site/r/index.html` exist) but the render
aborts in the post-render hook: no "Rendered N of N" summary, no stubs, no
`search.json`. Exit status is 1.

Control: the same project with the alias changed to `/old-r/` renders
`Rendered 2 of 2 files` and writes `_site/old-r/index.html` as a redirect
stub. So the only thing standing between Connect and its redirects is the
case-fold check.

## Localization

- **Detection** — `crates/quarto-core/src/project/aliases.rs`
  - `plan_alias_stubs` (≈ line 326) buckets every stub claim by
    `fold(stub_href)` (ASCII lowercase, line ≈ 510), and puts rendered pages
    in the same folded keyspace.
  - `plan_one_bucket` (≈ line 374) emits `AliasConflict::CaseOnlyPageCollision`
    when the bucket's folded key matches a page but the spelling differs
    (≈ line 413), and `AliasConflict::CaseOnlyAliasCollision` when two claims
    in one bucket spell the path differently (≈ line 428). Both set `fatal`,
    so the stub is dropped from the plan.
  - `plan_alias_stubs` is **pure** — it takes only `&[DocumentProfile]` and
    has no access to project config. Threading an option in means a new
    parameter (or an options struct), which touches its unit tests.
- **Reporting** — `crates/quarto-core/src/project/website_post_render.rs`
  - `write_alias_redirects` (≈ line 826): any non-empty `plan.conflicts`
    becomes a `QuartoError::Parse` and nothing is written.
  - `alias_conflict_diagnostic` (≈ lines 1055 and 1083): the two `Q-5-25`
    diagnostics. Neither mentions an escape hatch (there is none).
- **Config readers** — `crates/quarto-core/src/project/website_config.rs`
  holds the `website.*` readers; `website_llms_txt_enabled` is the model for
  a boolean key. The "key has no effect in this project type" warning
  pattern for non-website projects is in
  `crates/quarto-core/src/project/orchestrator.rs` ≈ line 426.
- **Docs that assert the current policy** and will need to change:
  `docs/errors/project/Q-5-25.qmd` (sections "Why it is checked everywhere",
  "How to fix it", "Difference from Quarto 1"),
  `docs/guides/projects/aliases.qmd` ("When aliases conflict" and
  "Differences from Quarto 1" → "Capitalization counts, on every platform"),
  and the `Q-5-25` entry in `crates/quarto-error-catalog/error_catalog.json`
  (its `message_template` repeats the "checked on every platform" rationale).
- **Tests** — `crates/quarto-core/tests/integration/website_aliases.rs`
  (`aliases_case_only_collision_is_error`, ≈ line 591) and the unit tests in
  `aliases.rs` (`plan_rejects_a_case_only_collision_*`,
  `plan_case_folding_is_platform_independent`, ≈ lines 1012–1060).

## What Quarto 1 does (for the documentation)

`external-sources/quarto-cli/src/project/types/website/website-aliases.ts`:
the overwrite guard is `allOutputFiles.find((outputFile) => outputFile ===
targetFile)` — an exact string compare. For the repro:

- **Linux (case-sensitive)**: Q1 writes `R/index.html` as a redirect to
  `../r/index.html`. Both files exist. This is what Connect's CI produces and
  what `https://docs.posit.co/connect/cookbook/.../viewer/R/` serves today.
- **macOS / Windows (case-insensitive)**: the same write lands on
  `r/index.html` and **replaces the page with a redirect to itself**
  (`../r/index.html` resolved from `R/` is the same file). Q1 does not notice.

So Quarto 1's output for this input depends on the build host's filesystem.
Quarto 2's current rule makes it deterministic by forbidding the input; the
proposed option makes it deterministic by letting the author state which
host they are building for.

## Open questions — resolved during triage

**Q1. Is this a bug or a design reversal?** A reversal. Decision 3 in the
2026-08-12 plan is explicit and the docs (`Q-5-25.qmd`, `aliases.qmd`) defend
it. The premise it rested on ("no realistic scenario") is what the issue
refutes. The user has confirmed the reversal is acceptable, gated behind an
affirmative option.

**Q2. Downgrade to a warning by default (reporter's option 1), or opt-in
(option 2 / 3)?** Opt-in, per the user. Rationale: an unconditional downgrade
would, on a case-insensitive output directory, write the stub *over the
rendered page* — the post-render hook runs after every page has been written,
so the stub wins and the page becomes a self-redirect. That is the Q1 macOS
failure mode, and a warning in a long build log is not enough protection
against it.

**Q3. With the option on, what happens on a case-insensitive output
directory?** This is the one design point the fix must get right, and the
reporter's option 3 is the answer: the writer has to know whether the output
directory is case-insensitive. If it is, the stub must **not** be written
(it would clobber the page) — skip it and say so, as a warning. On such a
host the page already answers for the aliased URL, so nothing is lost
locally; the stub only matters on the case-sensitive deployment target.
Checking requires a runtime probe of the *output directory* (not the project
directory — they can be on different volumes): either write a probe file and
test `path_exists` on its case-swapped name, or compare `canonicalize`
results for the two spellings. `SystemRuntime` already exposes
`file_write`, `path_exists`, `file_remove`, and `canonicalize`
(`crates/quarto-system-runtime/src/native.rs`). Without the option set,
nothing changes and no probe runs.

**Q4. Does the option also cover two *aliases* that differ only by case?**
Yes — same bucket, same mechanism, same deployment-target semantics. The
case-insensitive-host skip from Q3 applies to the second stub in the bucket.

**Q5. Option name.** The reporter floated `website.aliases.case-sensitive`.
Two concerns: `aliases` is a document-level *list* key, so a `website.aliases`
*map* invites confusion; and the fact being declared is a property of the
deployment target, not of aliases. Recommendation:
`website: case-sensitive-paths: true`, with the semantics "paths that differ
only by case are distinct on the host this site is served from". That name
also leaves room for the adjacent check q2 does not do today (two *pages*
whose output paths differ only by case, e.g. `R/index.qmd` and `r/index.qmd`,
which silently overwrite each other on macOS). **Final name is the user's
call**; alternatives considered: `website.aliases-case-sensitive`,
`project.case-sensitive-output`.

**Q6. Should the option, when set, silence `Q-5-25` or leave an info-level
note?** Silence. An affirmative option is the author saying "I know". The
only diagnostic with the option on is the Q3 skip warning on a
case-insensitive host, which is actionable (build on CI / the deployment
target to get the stub).

**Q7. Is there an existing strand?** `braid search alias` finds
bd-hzwecpyk (closed as duplicate) and bd-aliases-redirects-missing-sch7cd1g
(the implementation strand, still `in_progress` even though the feature
shipped and the plan is `status: complete`). Neither covers this request;
filed a new strand and linked it `related:` to the implementation strand.
The stale `in_progress` status is an incidental finding — flagged to the
user, not closed here.

## Outcome / recommended next step

Filed **bd-ex8qvuwu** with this fix scope:

1. **Option.** Add a `website.*` boolean reader in `website_config.rs`
   (name per Q5, to be confirmed). Thread it into
   `plan_alias_stubs` (new parameter or options struct); when set, the two
   `CaseOnly*` conflicts are not raised and the stubs are planned with their
   declared spelling. Warn "has no effect in this project type" for
   non-website projects, mirroring `website.llms-txt` in `orchestrator.rs`.
2. **Case-insensitive host guard (Q3).** In `write_alias_redirects`, when the
   option is on and a planned stub's folded path matches a rendered page or an
   earlier stub, probe the output directory. Case-insensitive → skip the stub
   with a warning naming the file and the reason. Case-sensitive → write it.
3. **Diagnostics.** Both `Q-5-25` messages gain a hint naming the option.
   Update `error_catalog.json`'s `message_template` so it no longer claims
   the check is unconditional.
4. **Docs.** `docs/errors/project/Q-5-25.qmd`: add the option to "How to fix
   it", qualify "Why it is checked everywhere", and expand "Difference from
   Quarto 1" with the host-dependent Q1 behaviour above (Linux writes both;
   macOS/Windows replace the page with a self-redirect).
   `docs/guides/projects/aliases.qmd`: add the option under "When aliases
   conflict" and amend "Capitalization counts, on every platform". Consider a
   short "Deploying to a case-sensitive host" subsection. Amend decision 3 in
   `claude-notes/plans/2026-08-12-aliases-redirect-stubs.md` with a pointer
   to this triage so the plan does not read as current policy.
5. **Tests.** Unit: `plan_alias_stubs` with the option on produces stubs for
   both `CaseOnly*` shapes. Integration (`website_aliases.rs`): option off is
   unchanged (`aliases_case_only_collision_is_error` stays); option on renders
   to completion; the stub assertion must branch on the *runtime* probe
   result, not on `cfg!(target_os)` — a macOS developer's APFS is normally
   case-insensitive but can be formatted case-sensitive, and Linux CI's tmpfs
   is case-sensitive. Expect both files on a case-sensitive dir, and the page
   intact plus the skip warning on a case-insensitive one. Cross-check
   Windows with `cargo clippy -p quarto-core --all-targets --target
   x86_64-pc-windows-msvc -- -D warnings` per `.claude/rules/cross-platform.md`.

Out of scope, noted for later: q2 has no check for two *pages* whose output
paths differ only by case (they overwrite each other on macOS/Windows with no
diagnostic). The option name in Q5 is chosen so it could gate that check too.

## Verification commands used

```bash
cargo xtask verify --skip-hub-build            # pre-flight on main @ 49cb226de — green (exit 0)
gh issue view 944 --repo quarto-dev/q2 --json title,body,author,createdAt,labels,comments
git switch -c issue-944
cd claude-notes/issue-reports/944/repro && ../../../../target/debug/q2 render   # reproduces Q-5-25
# control: same fixture with the alias renamed to /old-r/ renders 2 of 2 and writes _site/old-r/index.html
braid search alias; braid show bd-aliases-redirects-missing-sch7cd1g; braid show bd-hzwecpyk
```

## Cross-references

- bd-aliases-redirects-missing-sch7cd1g — implementation strand for `aliases:` (still `in_progress`, see Q7)
- bd-hzwecpyk — earlier duplicate, closed
- `claude-notes/plans/2026-08-12-aliases-redirect-stubs.md` — decisions 2 and 3 are what this issue revisits
- `docs/errors/project/Q-5-25.qmd`, `docs/guides/projects/aliases.qmd`
- `external-sources/quarto-cli/src/project/types/website/website-aliases.ts` — Q1 behaviour
- `.claude/rules/cross-platform.md` — runtime probe over `#[cfg]` gating for the tests
- GH #939 — the sibling list this issue was split out of
