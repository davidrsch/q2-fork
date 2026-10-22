---
title: 'Preview: render the skip-to-content link in hub-client preview (bd-21n09dwj)'
date: 2026-10-22
description: 'Decide whether the q2-preview iframe renders the skip-to-content link that render emits since bd-rbd3jq4n, and turn the red hub smoke-all E2E job green either way.'
status: draft  # Investigation — pending design alignment with user; do not implement before the go-ahead
braid:
  strand: bd-21n09dwj
---

**Branch:** `braid/bd-21n09dwj-preview-skip-link` (topic branch in the main checkout, no worktree; based on `main` @ `a5f86afdc`)
**Do not start implementation until the user gives the go-ahead.**

## Triage verdict

**Ready to design, and somewhat urgent.** The context is clear and the
change is small either way. The Hub-Client E2E smoke-all job has been red
on `main` since bd-rbd3jq4n merged (PR #889), so a decision is needed soon.
The real choice is design question 1: render the link in preview, or
exempt the fixtures.

## Issue context

bd-21n09dwj (task, P3, labels `a11y,hub-client`; filed 2026-10-21 by Carlos
Scheidegger, open). bd-rbd3jq4n added `#quarto-skip-link` to render's
`FULL_HTML_TEMPLATE` only; the "render-only for now" decision is recorded in
that strand's plan, decision 3. This strand decides whether
`PreviewDocument.tsx` renders the link too. If it doesn't, the strand
must make sure the parity tooling ignores the link.

Comment (2026-10-22): the Hub-Client E2E smoke-all job (nightly, and PRs
touching smoke-all fixtures) fails on `main` in chromium and firefox on
four fixtures:
`localization/lang-fr-skip-link.qmd`, `navigation/skip-link-website/index.qmd`,
`skip-link/skip-link-custom-layout.qmd`, `skip-link/skip-link.qmd`
(e.g. scheduled run 39739998088).

## Dependency graph

- **discovered-from**: bd-rbd3jq4n (in_progress; merged as PR #889,
  `f40864c9b`). Its plan is `claude-notes/plans/2026-10-21-skip-to-content-link.md`.
  Decision 3 made render the only target for now and filed this strand.
  Phase 5 already added `tabIndex={-1}` to the preview's `<main>` so that
  the DOM-parity root stays identical.
- No `blocks` / `related` edges. The only outside pressure is the red E2E
  job.

## What the code looks like today

**Why the E2E job fails.** `hub-client/e2e/helpers/smokeAllAssertions.ts`
runs `ensureFileRegexMatches` against a separate WASM render of the full
HTML. That render contains the link, so the regex assertions pass.
`ensureHtmlElements`, however, runs against the live **q2-preview iframe
DOM**. The failing selector in all four fixtures is
`body > a#quarto-skip-link:first-child[...]`. The website fixture also
asserts `body > a#quarto-skip-link ~ header#quarto-header`.

**The selector can't hold in preview even if the link is rendered.**
`hub-client/q2-preview.html` mounts React into `<body><div id="root">`, so
anything `PreviewDocument` returns lands under `body > div#root`. The
preview's `header#quarto-header` is under `#root` too. To satisfy the
fixtures as written, the link would need a portal or an imperative
insertion as `document.body.firstChild`, the same way `HeaderIncludesEffect`
writes to `document.head`. The website fixture's sibling selector would
also need the header to share a parent with the link, so it would still
fail. The alternative is to relax the selectors, e.g. `#quarto-skip-link`
without the `body >` anchoring, if the preview DOM may differ in that
respect.

**DOM-parity tooling already ignores the link.** `PARITY_ROOT_SELECTOR =
'main#quarto-document-content'` (`ts-packages/preview-renderer/src/test-utils/domParity.ts:205`)
covers both `domParity.ts` and `smokeAllParity.wasm.test.tsx`. The link
sits outside `<main>`, so that half of the strand needs no work.

**The preview's fragment links scroll but don't move focus.**
`routeLinkClick` (`ts-packages/preview-renderer/src/utils/iframeLinkHandlers.ts:124`)
intercepts every `#frag` click, then calls `onQmdLinkClick({anchor})`,
which ends in `scrollToAnchorInDocument` (`q2-preview/entry.tsx:332`).
That function calls `scrollIntoView` but not `focus()`. A skip link
rendered in preview would therefore scroll without moving keyboard focus,
which defeats its purpose (WCAG 2.4.1). To be useful, the handler would
also have to focus the target. `<main>` already has `tabIndex={-1}`.

**There is no other skip-exemption mechanism yet.** Two existing devices
come close:

- `DOM_ASSERTIONS_PENDING_PARITY` (`smokeAllDiscovery.ts`) skips *all*
  `ensureHtmlElements` for a fixture in q2-preview and requires a strand
  id. It is meant as a temporary staging list. Listing the four fixtures
  under bd-21n09dwj would turn CI green today, but would also drop the
  `main[tabindex]` and other selectors in those fixtures.
- `tests.run.skip` in fixture frontmatter skips the whole fixture,
  including in the native CLI runner. That is too broad.

**The hub-client chrome already has its own skip link.**
`hub-client/src/components/SkipLink.tsx` targets `#main-content` in the
editor app. A preview-iframe link would be a second "Skip to main content"
stop for keyboard users who tab into the iframe. Inside the iframe it
skips the preview's navbar/sidebar, which is the same motivation as on a
rendered website. The `dist-preview-embed` build (a reader/embed surface)
is where a preview-side link matters most.

**Styling would work in preview.** The preview uses the same compiled
Bootstrap theme CSS (via `UPDATE_THEME`), so `.visually-hidden-focusable`
and the `#quarto-skip-link:focus` rule from bd-rbd3jq4n's SCSS would apply.
The label needs `language.skip-to-content`. The draft-alert path already
reads language terms React-side, so follow `DraftAlertSlot` for that.

## Proposed phases (draft)

These depend on design question 1.

**Option A — render in preview:**

- Phase 0 — Tests: a unit test that `PreviewDocument` emits the link
  (localized, escaped by React), plus an E2E check that activating the link
  moves focus to `<main>`.
- Phase 1 — Insert the link as `document.body`'s first child: a portal
  or effect alongside `HeaderIncludesEffect`, outside `#root`. Skip it for
  minimal/theme-none, as render does.
- Phase 2 — Make `#quarto-document-content` (or any `#frag` whose target
  is focusable) also `focus({preventScroll: true})` in the anchor handler.
- Phase 3 — Reconcile the fixtures: the website fixture's
  `~ header#quarto-header` sibling selector can't hold, so relax it or
  list it under a preview-specific exemption.
- Phase 4 — Update the stale "render-only (bd-21n09dwj)" comment in
  `PreviewDocument.tsx:402`, and add a hub-client changelog entry.

**Option B — stay render-only:**

- Phase 1 — Add a narrow preview exemption for the body-anchored skip-link
  selectors only, not the whole fixture. Either use a per-selector
  allowlist in `smokeAllAssertions.ts`, or split the fixtures so the
  body-anchored selectors live in a fixture marked as not applicable to
  the hub runner.
- Phase 2 — Record the divergence in the PreviewDocument comment and in
  the bd-rbd3jq4n plan.

**Either way, Phase −1 (stopgap):** if the design takes more than a day,
add the four fixtures to `DOM_ASSERTIONS_PENDING_PARITY` under
bd-21n09dwj to turn the job green now, and remove them when this strand
closes.

## Open design questions for the user

1. **Render or exempt?** Should the q2-preview iframe render the skip link
   (Option A), or should preview stay render-only, with the hub E2E runner
   exempting the body-anchored selectors (Option B)? A favours the
   preview-embed/reader surface. B avoids a second "Skip to main content"
   stop in the editor, where hub-client's own `SkipLink` already exists.
2. **Stopgap now?** Should I land the `DOM_ASSERTIONS_PENDING_PARITY`
   entries for the four fixtures as a separate small PR today, so the
   nightly goes green while the real fix is designed?
3. **(If A) DOM placement.** Is inserting the link imperatively as
   `document.body.firstChild` (outside React's `#root`) acceptable? The
   alternative is to keep it inside `#root` and relax the fixtures to
   `a#quarto-skip-link` with no `body >` anchoring.
4. **(If A) Focus behaviour for fragment links.** Should the preview's
   `#frag` handler move focus for every fragment link whose target is
   focusable, or only for the skip link? Moving focus for every link is
   closer to native browser behaviour.
5. **(If A) Editor vs. embed.** Render the link in every q2-preview
   context, or only in the `VITE_PREVIEW_EMBED` / reader build, where the
   iframe is the whole page?

## Risks / tradeoffs (draft)

- An imperative `document.body` insertion outside React must be
  idempotent across re-renders and morphs, and must be cleaned up on
  unmount. `HeaderIncludesEffect` is the model, and it has had to handle
  the same issues.
- Changing the shared fragment-link handler to focus targets affects
  every in-document link in preview (TOC, cross-refs). Focusing a
  non-focusable target is a no-op, but scroll-sync and block-edit
  overlays should be checked.
- Any `DOM_ASSERTIONS_PENDING_PARITY` stopgap hides the other selectors
  in those four fixtures, including `main[tabindex="-1"]`, which already
  holds in preview. It is acceptable only as a short-lived measure.
- Pre-flight `cargo xtask verify --skip-hub-build` at `a5f86afdc`:
  10498/10499 Rust tests passed. The one failure,
  `revealjs::assemble::tests::vendored_reveal_assets_match_npm_package`,
  comes from the local environment: `node_modules/reveal.js` is 6.0.0,
  but `package-lock.json` pins 6.0.2 since `a3b6b4f79`, so it is a stale
  `npm install`. Verify stopped at the Rust stage, so the later TS stages
  did not run. Re-run after `npm install` before implementing.
