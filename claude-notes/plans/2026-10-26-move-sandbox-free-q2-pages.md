---
title: 'Move the sandboxed preview to quarto-dev.github.io/q2-sandbox and free /q2 for the Q2 docs'
date: 2026-10-26
description: 'quarto-dev.github.io/q2 serves the q2-sandboxed-preview iframe today, and the live quarto-hub.com bundle points at it. Move the sandbox to a new repo quarto-dev/q2-sandbox, repoint hub-client, retire the old Pages workflow, then publish the Q2 docs at /q2/ with a self-unregistering service worker that cleans up stale sandbox workers.'
---

Strand: **bd-73lilznv**. Follow-up (out of scope here): **bd-gotwa715**, which
gives the sandbox its own origin.

## Overview

### Current state (verified 2026-10-26)

- `quarto-dev/q2` has GitHub Pages in **GitHub Actions mode**
  (`build_type: workflow`), so there is no `gh-pages` branch. Pages serves
  whatever artifact the last workflow run uploaded.
- That workflow is `.github/workflows/deploy-sandboxed-preview.yml`. It builds
  `hub-client/quarto-hub-sandboxed-preview/` and deploys `dist/` on pushes to
  `main` that touch the sandbox package, `ts-packages/preview-renderer/**`,
  `resources/js|revealjs|attribution/**`, or the workflow itself. The
  `github-pages` environment allows deploys from `main` only (custom branch
  policy).
- `Q2SandboxedPreviewIframe.tsx:67` defaults
  `VITE_Q2_SANDBOXED_PREVIEW_URL` to `https://quarto-dev.github.io/q2/`.
- **Production** (`quarto-dev/quarto-hub-deployment`,
  `.github/workflows/build-and-deploy.yml`) does not override the variable.
  The live quarto-hub.com bundle contains `quarto-dev.github.io/q2/`. The
  `q2 preview --ui editor` embed (`npm run build:preview-embed` in
  `release-pipeline.yml`) uses the same default.
- The iframe only loads for documents with `format: q2-sandboxed-preview`.
- e2e, `scripts/local-dev.sh`, and `hub-client-e2e.yml` all override the URL,
  so this move doesn't affect them.

### Decisions (user, 2026-10-26)

- The new home is **`https://quarto-dev.github.io/q2-sandbox/`**, served from
  a new repo `quarto-dev/q2-sandbox`. The name is free (404 on the API as of
  today).
- Breaking `q2-sandboxed-preview` in already-released q2 binaries is
  acceptable (early development). A changelog line is enough.
- The new URL has the same origin as the docs (`quarto-dev.github.io`), which
  is a known weakness. It is no worse than today, and bd-gotwa715 tracks the
  fix.

### Hazard: stale service workers at scope `/q2/`

`quarto-hub-sandboxed-preview/src/entry.tsx:400` calls `initServiceWorker()`
on every page load, including a direct top-level visit. The worker
(`serviceWorker.ts`) intercepts **every** in-scope GET except `''`,
`index.html`, `serviceWorker.js` and `q2-preview-assets/*`, and proxies it to
the parent frame. A top-level navigation has no `clientId`, so the proxy
answers **502 "No client for VFS proxy request"**.

So anyone who ever opened `https://quarto-dev.github.io/q2/` directly would
get 502s on every docs page except the landing page. Workers registered
inside the quarto-hub.com iframe are partitioned by top-level site in current
browsers and don't affect direct visits.

Removing `serviceWorker.js` (a 404) does **not** unregister the old worker.
The docs site must ship a replacement `serviceWorker.js` at the same path.
Its job is to activate, unregister itself, and reload any open pages.
Browsers check for a worker update on in-scope navigations, and the landing
page (`index.html`, exempt from interception) always loads from the network,
so recovery is quick. The landing page also gets a small unregister snippet
as a backup.

## Work items

### Phase 1: Create `quarto-dev/q2-sandbox` and deploy to it

The q2 workflow can't deploy Pages for another repo through
`actions/deploy-pages`. **Recommended:** keep the build in q2, where the
trigger paths already live, and push `dist/` to the `gh-pages` branch of
`q2-sandbox` with a deploy key. The alternative, a workflow in `q2-sandbox`
triggered by `repository_dispatch` that checks out q2 and builds, splits the
build config across two repos. Confirm the choice before Phase 1 runs.

- [ ] Create public repo `quarto-dev/q2-sandbox` with a README explaining
      that it's a deploy target and the source lives in
      `quarto-dev/q2/hub-client/quarto-hub-sandboxed-preview`. *(Outward-facing:
      confirm with the user before running `gh repo create`.)*
- [ ] Generate an SSH deploy key pair. Add the public key to `q2-sandbox`
      with write access, and the private key to `quarto-dev/q2` as secret
      `Q2_SANDBOX_DEPLOY_KEY`. *(Confirm before writing secrets. The user
      may prefer to do this by hand.)*
- [ ] Add a new workflow in q2, `.github/workflows/deploy-q2-sandbox.yml`.
      It uses the same triggers and build steps as
      `deploy-sandboxed-preview.yml`, adds `.nojekyll` to `dist/`, and
      pushes `dist/` to `q2-sandbox` `gh-pages` (force-push a single commit,
      e.g. `peaceiris/actions-gh-pages` with `external_repository` +
      `deploy_key`, pinned to a version at least 2 weeks old). It needs only
      `contents: read`, with no `pages`/`id-token`.
- [ ] After the first push, enable Pages on `q2-sandbox` from the `gh-pages`
      branch (`gh api -X POST repos/quarto-dev/q2-sandbox/pages ...`).
- [ ] Verify that `https://quarto-dev.github.io/q2-sandbox/`,
      `/q2-sandbox/serviceWorker.js` and a `q2-preview-assets/` chunk all
      return 200 with the right content types.

Run the old and new workflows side by side until Phase 3. `/q2/` keeps
serving the sandbox until production stops using it.

### Phase 2: Repoint hub-client (q2 PR)

- [ ] `hub-client/src/components/render/q2-sandboxed-preview/Q2SandboxedPreviewIframe.tsx:67`:
      change the default to `https://quarto-dev.github.io/q2-sandbox/` and
      update the comment above it.
- [ ] `assetPolicy.test.ts`: update `PAGES_SCOPE` (line 20) and `base` (line
      113). Check for any other `/q2/` literals the tests depend on.
- [ ] Docs and notes: rewrite `.github/workflows/github-pages.md` to cover
      the new split (q2 Pages = docs, q2-sandbox = sandbox). Update
      `claude-notes/designs/q2-sandboxed-preview-separate-domain.md`
      (architecture diagram and deployment bullet) and
      `.claude/skills/snyk-pr/references/katex.md` (workflow name). Leave
      the historical 2026-09-01 port plan as it is.
- [ ] Add a `hub-client/changelog.md` entry.
- [ ] Origin pinning doesn't change: `sandboxTargetOrigin()` derives
      `https://quarto-dev.github.io` from both URLs.
- [ ] Tests: `npm run test` in hub-client (assetPolicy unit tests) and the
      `q2-sandboxed-preview` e2e spec, which uses the same-origin copy and
      should be unaffected.
- [ ] Merge to `main`. *(Push and PR only with explicit go-ahead.)*

### Phase 3: Production cutover

- [ ] Trigger a quarto-hub.com rebuild from `quarto-hub-deployment` so it
      picks up the new default from q2 `main`. *(Confirm before triggering
      a production deploy.)*
- [ ] Verify the live bundle contains `quarto-dev.github.io/q2-sandbox/` and
      no longer contains `quarto-dev.github.io/q2/`, using the same
      curl-and-grep against `https://quarto-hub.com/` as before.
- [ ] Smoke test: open a `format: q2-sandboxed-preview` doc on quarto-hub.com.
      The iframe should load from `/q2-sandbox/`, an image asset should
      render through the SW proxy, and the console should be clean.

### Phase 4: Retire the old workflow and publish docs at `/q2/`

- [ ] Delete `.github/workflows/deploy-sandboxed-preview.yml`. Leave Pages
      on `quarto-dev/q2` in GitHub Actions mode.
- [ ] Add a docs deploy workflow (e.g. `.github/workflows/deploy-docs.yml`)
      that builds q2, runs `cargo xtask stage-doc-examples`, renders `docs/`
      with **Q2** (`q2 render docs/`, not Q1), and deploys `docs/_site/` via
      `upload-pages-artifact` + `deploy-pages` to the existing
      `github-pages` environment. Trigger it on `main` pushes touching
      `docs/**`, plus `workflow_dispatch`. Decide during implementation
      whether a pinned q2 release binary or a source build is the better
      renderer for CI; the release pipeline's docs-embed step is prior art.
      Set `site-url: https://quarto-dev.github.io/q2/` if the site needs it
      (sitemap, llms.txt).
- [ ] Add a self-unregistering `serviceWorker.js` to the docs output root
      (e.g. `docs/serviceWorker.js` listed under `project.resources`):

  ```js
  self.addEventListener('install', () => self.skipWaiting());
  self.addEventListener('activate', (e) => e.waitUntil((async () => {
    await self.registration.unregister();
    for (const c of await self.clients.matchAll({ type: 'window' })) c.navigate(c.url);
  })()));
  ```

  Add a comment explaining why it exists and when it can go (no earlier
  than about 2027-04).
- [ ] Add a backup unregister snippet to the docs landing page: on load,
      unregister any `navigator.serviceWorker` registration whose scope is
      `/q2/`.
- [ ] Verification: in a browser profile, open the **old** sandbox page at
      `/q2/` top-level before the docs deploy, so its worker registers.
      Deploy the docs, then visit a docs subpage. Expect at most one 502
      followed by recovery, or none if the landing page is visited first.
      Confirm in `chrome://serviceworker-internals` that the registration
      is gone. If no pre-deploy profile was captured, record that the step
      was skipped.

## Out of scope

- Separate origin for the sandbox: **bd-gotwa715**.
- Stopping `entry.tsx` from registering the worker when not framed. This is
  noted on bd-gotwa715. It matters less once `/q2-sandbox/` has no
  first-party visitors besides us.

## Progress log

- **2026-10-26: Phase 1 GitHub setup done.** Created `quarto-dev/q2-sandbox`
  (public), added a write deploy key (title `quarto-dev/q2
  deploy-q2-sandbox.yml`), and set secret `Q2_SANDBOX_DEPLOY_KEY` on
  `quarto-dev/q2`. The local key was deleted right after. The first
  `gh-pages` commit was pushed **by hand** from a local `dist/` build (with
  `.nojekyll`), and Pages is enabled from `gh-pages` /. Verified 200 with
  correct content types for `/`, `index.html`, `serviceWorker.js`
  (byte-identical to the local build), the JS/CSS chunks and a KaTeX woff2.
  The CI workflow `deploy-q2-sandbox.yml` (plain git + deploy key, no
  third-party action) is written and passes actionlint. It has not run yet
  and first runs when this branch merges to `main`.
- **2026-10-26: Phase 2 code done on branch
  `braid/bd-73lilznv-move-sandboxed-preview-quarto`.** Changed the default
  URL, updated assetPolicy tests (plus a `/q2/` vs `/q2-sandbox/`
  scope-prefix case), `github-pages.md`, the design doc and the snyk katex
  reference, and marked `deploy-sandboxed-preview.yml` TRANSITIONAL. It
  keeps running until Phase 4. hub-client `npm run test` passes
  (1686/1686) and `tsc -b` is clean.
