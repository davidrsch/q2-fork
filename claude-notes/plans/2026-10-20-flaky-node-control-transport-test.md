---
title: 'Flaky node-control-transport test ("pongpong") on PR #865'
date: 2026-10-20
description: 'The connectControl round-trip test replied once per TCP data event, so a split token/ping write produced two pongs; make the fake server reply once per full request.'
---

**PR:** #865 (branch `feature/bd-yt3x1gwk-pampa-fails-compile-without`), where
the failure surfaced. The flake is unrelated to that PR's change.
**Failing CI job:** https://github.com/quarto-dev/q2/actions/runs/39430511172/job/118654033594
(macos-latest, `npm run test -w @quarto/engine-host-node`).

## Symptom

```
FAIL src/node-control-transport.test.ts > connectControl >
     writes the token first, then exchanges bytes in both directions
AssertionError: expected 'pongpong' to be 'pong'
```

## Diagnosis (test bug, not a product bug)

The fake server in the test did `socket.write("pong")` on **every** `data`
event. The client sends two writes: the token `secret-token\n` (inside
`connectControl`) and then `ping`. TCP has no message boundaries:

- Usual case (all local runs, 40/40): both writes are in the kernel buffer
  before the server reads, so it sees one event `["secret-token\nping"]` and
  sends one pong.
- Loaded CI runner: the server reads the token alone, replies `pong` **to
  the token**, then reads `ping` and replies again. Both pongs coalesce into
  one client-side socket read, so the reader's first chunk is `pongpong`.

A scratch repro that delayed the `ping` write showed server events
`["secret-token\n", "ping"]` (two pongs sent), which confirms the mechanism.
The final coalescing step on the client depends on scheduling and can't be
forced in-process.

Production is unaffected. The bytes the server received were still exactly
`secret-token\nping` with the token first, and the real control channel uses
length-prefixed framing (`framing.ts`), so chunk boundaries don't matter.

## Fix

- [x] In `ts-packages/quarto-engine-host-node/src/node-control-transport.test.ts`,
  the fake server accumulates bytes and writes `pong` once, after the full
  `secret-token\nping` has arrived. The final assertion reuses the same
  `expected` constant.
- [x] Local: file passes 5/5 runs.
- [x] Full `@quarto/engine-host-node` suite passes locally (161/161).
- [x] Commit plan and fix on the PR #865 branch; push.
- [x] Monitor PR #865 CI: all checks green on `d07a58d5` (2026-10-20).
- [x] Merged (`9758c3a2`). `gh pr merge 865 --merge` from the CLI was refused
  ("base branch policy prohibits the merge"; mergeStateStatus BLOCKED, no
  reviews, cause unclear), so the user merged from the GitHub UI.
