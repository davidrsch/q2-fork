---
title: 'Skill: autonomously-fix-small-issue'
date: 2026-10-27
description: 'Codify the issue -> strand -> plan -> branch -> fix -> PR workflow used for GH #930 / PR #934 as a user-invoked-only skill, with a scope gate up front.'
---

Strand: **bd-t18wrsqo**. Reference run: GH #930 → PR #934.

## Overview

The #930 fix ran end-to-end without user input: read the issue, filed a
strand and plan, branched, extracted a testable helper, verified (including
telling pre-existing lint failures apart from new ones), opened a PR, and
commented the PR link on the strand. This skill makes that repeatable.

The name says "autonomously" on purpose: the run is pre-approved, so the
safeguards are structural rather than per-step confirmations:

- `disable-model-invocation: true`, so only the user can start it.
- A scope gate before any strand or branch exists, which hands off to
  `/triage` on failure.
- A hard stop at the PR: no merge, no issue close, no strand close.
- PR body reports every check run, pre-existing failures, and what wasn't
  verified.

## Work items

- [x] `.claude/skills/autonomously-fix-small-issue/SKILL.md`
- [x] Open PR
