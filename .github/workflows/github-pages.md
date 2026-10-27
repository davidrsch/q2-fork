# GitHub Pages deployment

## Sandboxed preview → https://quarto-dev.github.io/q2-sandbox/

`deploy-q2-sandbox.yml` builds `hub-client/quarto-hub-sandboxed-preview/`
(root `npm ci` for the workspace deps the renderer source needs, then
`npm ci` + `npm run build` inside the package) and publishes its `dist/` —
`index.html`, hashed `q2-preview-assets/` (renderer bundle, KaTeX fonts), and
`serviceWorker.js` — to **https://quarto-dev.github.io/q2-sandbox/**,
where hub-client's `Q2SandboxedPreviewIframe.tsx` loads it cross-origin as its
iframe `src` (override with `VITE_Q2_SANDBOXED_PREVIEW_URL`). The page is
inert when visited directly — it only resolves content via `postMessage` to a
parent frame.

A workflow cannot deploy Pages for another repository, so the build runs here
and the result is force-pushed as a single commit to the `gh-pages` branch of
[`quarto-dev/q2-sandbox`](https://github.com/quarto-dev/q2-sandbox), whose
Pages site serves that branch (with `.nojekyll`). Auth is a write deploy key
on `q2-sandbox` only; its private half is this repo's
`Q2_SANDBOX_DEPLOY_KEY` secret. To rotate it, generate a new ed25519 key,
replace the deploy key on `q2-sandbox`, and `gh secret set
Q2_SANDBOX_DEPLOY_KEY -R quarto-dev/q2`.

It runs on any push to `main` touching the sandbox package, the
preview-renderer source, or the vendored resources it embeds, or manually via
`gh workflow run deploy-q2-sandbox.yml`.

## Q2 docs → https://quarto-dev.github.io/q2/

`deploy-docs.yml` builds q2 from the pushed commit, stages the example
projects (`cargo xtask stage-doc-examples`), renders `docs/` with that q2, and
deploys `docs/_site` to this repo's own Pages site through the
`github-pages` environment. Pages here is in "GitHub Actions" mode (no
`gh-pages` branch; it serves the artifact the last deploy uploaded), and the
`/q2/` path is fixed by GitHub: project Pages sites always live at
`https://<org>.github.io/<repo>/`.

It runs on any push to `main` touching `docs/` or `examples/`, or manually
via `gh workflow run deploy-docs.yml`. Pull requests touching those paths run
the build job only, so a docs page that fails to render is caught before
merge.

Until 2026-10 this site served the sandboxed preview, whose service worker
breaks page loads for anyone who still has it registered.
`docs/serviceWorker.js` replaces it with one that unregisters itself; see
the comment there for when it can go (bd-73lilznv).

Note that both sites share the `quarto-dev.github.io` origin; giving the
sandbox an origin of its own is bd-gotwa715.
