# Repro: extension-fragment paths mis-resolved (bd-74ktvzv7)

`fixture/` is a minimal custom-project-type website: extension `x` sets
`website.favicon`, `navbar.logo`, `navbar.logo-href` and `sidebar.logo` to the
bundled `assets/logo.svg`, plus a navbar item `href: index.qmd`. It has a root
page and `sub/page.qmd`.

```bash
cp -R claude-notes/plans/navbar-logo-prefix-doubled-investigation/fixture <scratch>/fx
cd <scratch>/fx && q2 render
grep -oE '<(a|link|img)[^>]*(logo|index)[^>]*>' _site/index.html _site/sub/page.html
```

Observed at `main` @ 7584d6276 (2026-10-22). `_site/_extensions/x/assets/logo.svg`
is copied correctly; the emitted references are not.

| Element | `index.html` | `sub/page.html` | Expected (root / sub) |
|---|---|---|---|
| favicon `<link>` | `_extensions/x/assets/logo.svg` ✅ | `../../_extensions/x/assets/logo.svg` ❌ | `…` / `../_extensions/x/assets/logo.svg` |
| navbar logo `<img>` | `_extensions/x/_extensions/x/assets/logo.svg` ❌ | `../_extensions/_extensions/x/assets/logo.svg` ❌ | same as favicon |
| `logo-href` `<a>` | `_extensions/x/assets/logo.svg` ✅* | `../_extensions/x/assets/logo.svg` ✅* | same as favicon |
| navbar item `href: index.qmd` | `_extensions/x/index.qmd` ❌ + Q-13-2 | `_extensions/x/index.qmd` ❌ | `index.html` / `../index.html` |
| sidebar logo | not rendered | not rendered | (unimplemented, bd-ht0n) |

\* `logo-href` is right by accident. It is not in `FRAGMENT_PATH_PATTERNS`, so
it is never rebased, and `resolve_metadata_path` resolves the raw
`assets/logo.svg` against the extension dir. Once the ❌ rows are fixed, it
would break unless it gets rebased too.

Q-13-2 warning: "`_extensions/x/index.qmd` is not in the project index",
anchored at `_extensions/x/_extension.yml:15:19`.

Control (same layout, values in `_quarto.yml`, no extension): `sub/page.html`
gets `../logo.svg` for both favicon and navbar logo, which is correct. So the
subdirectory double-relativization only happens on the extension path.
