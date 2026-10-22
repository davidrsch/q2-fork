---
title: 'Preview: render the skip-to-content link in hub-client preview (bd-21n09dwj)'
date: 2026-10-22
description: 'Decide whether the q2-preview iframe renders the skip-to-content link that render emits since bd-rbd3jq4n, and turn the red hub smoke-all E2E job green either way.'
status: done  # PR #902 merged 2026-10-22
braid:
  strand: bd-21n09dwj
---

**Branch:** `braid/bd-21n09dwj-preview-skip-link` (topic branch in the main checkout, no worktree; based on `main` @ `a5f86afdc`)

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

## Decisions (2026-10-22, with the user)

1. **Render in preview (Option A).** With decisions 3 and 5 below, A
   needs less new code than B, which would have needed a per-selector
   exemption mechanism in the E2E runner.
2. **No stopgap.** Don't add `DOM_ASSERTIONS_PENDING_PARITY` entries; fix
   the job directly.
3. **Keep the link inside React's `#root`, and relax the fixtures.** Drop
   `body >` / `:first-child` from the skip-link selectors. Render's "first
   element of body" guarantee moves into an `ensureFileRegexMatches`
   regex, which both runners evaluate against the full render.
4. **General focus fix.** Every intercepted `#frag` jump focuses its
   target when the target can take focus. `focus()` is a no-op otherwise,
   so heading targets behave as before.
5. **Every q2-preview context**, with no embed-only condition.

## Implementation (as built)

- `crates/quarto-core/tests/integration/preview_language_terms.rs`:
  pins that the q2-preview AST carries `meta.quarto.language`
  (`skip-to-content`, English and `lang: fr`). The preview label depends
  on this.
- `SkipLinkSlot` (`chromeSlots.tsx`), rendered as the first element of
  the non-minimal branch of `PreviewDocument`. The label comes from
  `meta.quarto.language['skip-to-content']`, falling back to "Skip to
  main content", and React escapes it.
- `utils/jumpToAnchor.ts`: scroll plus `focus({preventScroll: true})`.
  It is shared by all three anchor-scroll sites: the q2-preview
  `entry.tsx`, `MorphIframe` (q2-html-render, which already showed
  render's link with the same focus bug), and `DoubleBufferedIframe`.
- Fixtures: the four failing fixtures are relaxed. `skip-link.qmd` gains
  the `<body…>\s*<a id="quarto-skip-link"` regex, and the website
  fixture keeps `a#quarto-skip-link ~ header#quarto-header`. That holds
  in preview because the link and the header are siblings in the
  fragment `PreviewDocument` returns.

## Phases

- [x] Phase 0 — Tests first: the Rust preview-AST contract test,
  `PreviewDocument` skip-link tests (order, localized and escaped,
  fallback, absent in minimal), and `jumpToAnchor` tests (focuses a
  focusable target, leaves focus alone otherwise).
- [x] Phase 1 — `SkipLinkSlot` + label.
- [x] Phase 2 — `jumpToAnchor` at the three call sites.
- [x] Phase 3 — Relax the fixtures; the native smoke-all run passes.
- [x] Phase 4 — Hub E2E smoke-all for the skip-link fixtures
  (chromium + firefox), and the full preview-renderer suites.
  Results: 10/10 E2E passed. The preview-renderer suites, the hub-client
  unit, integration and DOM-parity WASM tests, and `tsc -b` are all
  green. `cargo xtask verify --skip-hub-build`: 15918/15919 Rust tests
  passed. The one failure,
  `quarto-preview editor_ui_serves_pdfjs_from_the_viewer_bundle`, comes
  from the local environment: `q2-preview-spa/dist` was built before
  pdf.js was bundled, so it has no `pdfjs/` tree. Verify cancels on that
  failure, so its later TS stages were run by hand as listed above.
- [x] Phase 5 — hub-client changelog entry.

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
