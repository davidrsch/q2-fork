---
title: 'Vendored dependency survey — 2026-10-21'
date: 2026-10-21
description: 'First run of the upgrade-vendored-deps skill over 33 tracked assets: 12 outdated, 5 unrecorded with upstream activity, 1 for manual comparison; eight strands filed, and the Bootstrap group skipped for Quarto 1 parity.'
braid:
  strand: bd-xm7l
  labels: [deps, vendored]
---

**Skill:** `.claude/skills/upgrade-vendored-deps/SKILL.md` (first run, bd-xm7l Phase 4)
**Branch:** `braid/bd-xm7l-vendored-deps-registry` (the registry is not on `main` yet, so this survey ran on the branch that introduces it, off `main` @ `f6e950d29`)
**Previous survey:** none

## TL;DR

- Upstream changes worth acting on: 8 strands filed (bd-jsfg1494, bd-7vum2l3y, bd-nrbxc8di, bd-u1l2llha, bd-2qy6mdhq, bd-mowfwe7m, bd-kn7kxsib, bd-grbnhs0y).
- Skipped deliberately: the Bootstrap group (`bootstrap-scss` 5.3.1 → 5.3.8, `bootstrap-icons` 1.13.1 → 1.13.2 on npm), because Quarto 1 v1.11.6 still ships 5.3.1 and 1.13.1, and these follow Quarto 1 for theme parity.
- Inventory gaps (unrecorded versions): 6 at the start of the survey, 5 at the end. `reveal-menu-css` was established as byte-identical to the lockfile-resolved `reveal.js-menu` 2.1.0 and recorded.
- Reconciled prior strands: none (first survey; no `Vendored:` strands existed).
- Unregistered third-party content found by discovery: none.
- Query errors: none.

## Survey

`cargo xtask vendored outdated`, after recording `reveal-menu-css`:

```

== (ungrouped)
  chicago-author-date-csl        manual       2025-08-07T00:00:00+00:00 -> 420698a0a8    
      last upstream change under the copied paths: 1aaf551f 2026-04-14 Chicago: supply `sub-verbo` with shortened note
  clipboard-js                   current      2.0.11         -> 2.0.11        
  headroom                       current      0.12.0         -> 0.12.0        
  hephaestus-fonts               current      0.4.1          -> 0.4.1         
  julia-highlights               current      e0f9dcd180     -> e0f9dcd180    
  mermaid                        outdated     11.12.0        -> 12.2.0        
  pdfjs-viewer                   outdated     6.3.289        -> v6.4.299      
  react-npm                      outdated     19.2.7         -> 19.3.1        
      followers: react
  subtree-julia-engine           current      f6d75d172c     -> f6d75d172c    

== bootstrap: Bootstrap SCSS / JS / Icons
  bootstrap-icons                outdated     1.13.1         -> 1.13.2        
  bootstrap-scss                 outdated     5.3.1          -> 5.3.8         
      followers: bootstrap-js

== cdn: Libraries loaded from a CDN at page load (not vendored bytes)
  cdn-algolia-insights           unpinned     *              -> 2.17.3        
  cdn-algoliasearch              outdated     4.5.1          -> 5.60.0        
  cdn-katex                      outdated     0.18.9         -> 0.19.0        
  cdn-marimo-islands             unpinned     *              -> 0.25.2        
  cdn-mathjax                    outdated     3              -> 4.1.3         

== pandoc: Pandoc-derived assets
  pandoc-html-template           unrecorded   (unrecorded)   -> 3.12.1        
      last upstream change under the copied paths: 15729063e 2026-08-31 Dark mode support for HTML output (#11831)
  pandoc-wasm                    outdated     3.11           -> 3.12.1        

== quarto-cli: Assets copied from quarto-cli (Quarto 1)
  bootstrap-quarto-layer         unrecorded   (unrecorded)   -> v1.11.6       
      last upstream change under the copied paths: d742b9083 2026-09-02 Restore keyboard focus indicator on bare .btn buttons (#14785)
  formats                        current      v1.11.3        -> v1.11.6       
  highlight-styles               current      2e6695811      -> v1.11.6       
  knitr-r-scripts                unrecorded   (unrecorded)   -> v1.11.6       
      last upstream change under the copied paths: e24bcbd1f 2026-09-22 Fix original.code attribute leaking into every chunk's .cell div (#14928)
  language                       outdated     45caede32a     -> v1.11.6       
      4 upstream commit(s) touch the copied paths since 45caede32a0f987c4a377a952120cac6d624cb31:
        971730608 2026-10-05 Make overflowing code and output keyboard-accessible (#14816)
        a79a39a9a 2026-09-11 Add Azerbaijani localization (#14878)
        5f85ecc4f 2026-09-09 Give every website and book `nav` landmark a distinct, localizable label (#14813)
        7eebb00b9 2026-09-01 Add skip-to-content link to Bootstrap-themed HTML output (#14685)
  pandoc-filters                 outdated     v1.11.3        -> v1.11.6       
      7 upstream commit(s) touch the copied paths since v1.11.3:
        7d04eebe5 2026-10-12 Keep dashboard HTML output mistaken for bslib output in its card (#15024)
        b311ccddc 2026-10-07 Support lof and lot in Typst documents (#15001)
        742f015bc 2026-10-03 typst: resolve CSS generic font families to an installed font
        994986aef 2026-10-05 Fix heading detection when metadata contains footnotes (#14992)
        39c153ea8 2026-09-28 Fix Typst px image sizes for images with alt text (#14951)
        1a0817119 2026-09-16 Fix quarto.version crash on build-metadata version strings (#14894)
        61c81493f 2026-09-03 Fix dead toc_title auto-fallback in typst outline template
  quarto-builtin-extensions      unrecorded   (unrecorded)   -> v1.11.6       
      followers: videojs
      last upstream change under the copied paths: bc2d42e00 2026-07-23 Fix `{{< placeholder >}}` shortcode failing on non-SVG output (#14724)
  quarto-listing-js              current      19a7da95dc     -> v1.11.6       
  quarto-search-js               unrecorded   (unrecorded)   -> v1.11.6       
      followers: algolia-autocomplete
      last upstream change under the copied paths: b3d901f54 2026-03-02 Fix search ?q= scroll overriding hash-fragment navigation (#14141)
  typst-packages                 current      v1.11.3        -> v1.11.6       

== reveal-js: reveal.js runtime, theme template and plugins
  reveal-menu-css                current      2.1.0          -> 2.1.0         
  revealjs-dist                  outdated     6.0.0          -> 6.0.2         
      followers: revealjs-scss-template

== rust-vendored: Vendored or forked Rust crates
  mitex                          current      985d8e7        -> 23ab8dffda    
      followers: mitex-default-spec

== typst: Typst packages, subtrees and the Typst WASM stack
  subtree-orange-book            current      2b59b76727     -> 2b59b76727    
      followers: typst-pkg-orange-book
  typst-wasm                     current      0.7.0          -> 0.7.0         

Not queried: 7 follow (reported with their leader), 2 frozen, 3 external, 7 native.
```

## Strands filed

| Strand | Group / asset | Delta | Why it matters |
|---|---|---|---|
| bd-jsfg1494 | quarto-cli copies | `pandoc-filters` v1.11.3 → v1.11.6 (7 commits), `language` (4 commits), 4 unrecorded assets with upstream activity | Typst lof/lot, Typst image sizing and font fixes, accessibility strings; the unrecorded ones need their refs established |
| bd-7vum2l3y | pandoc | 3.11 → 3.12.1; HTML template dark-mode change upstream | moves `PANDOC_PIN` everywhere the pandoc-pin lint checks, plus the wasm SHA pins |
| bd-nrbxc8di | reveal-js | 6.0.0 → 6.0.2 | patch; npm pins and the vendored dist move together |
| bd-u1l2llha | cdn | KaTeX 0.18.9 → 0.19.0; MathJax 3 → 4 and algoliasearch 4 → 5 are majors | KaTeX is pinned to the preview's npm KaTeX; the majors are code changes, and parity with Quarto 1 argues for staying put |
| bd-2qy6mdhq | mermaid | 11.12.0 → 12.2.0 (major) | API/theming review, not a byte refresh |
| bd-mowfwe7m | pdfjs-viewer | 6.3.289 → 6.4.299 | the fetch script's worker-fingerprint patch must still apply |
| bd-kn7kxsib | react-npm (+ `react`) | 19.2.7 → 19.3.1 | `cargo xtask vendor-react` after the npm bump |
| bd-grbnhs0y | chicago-author-date-csl | upstream changed 2026-04-14 | the default citation style; snapshot changes are behaviour changes |

## Reconciled prior strands

None: this is the first survey. Open strands that touch the same assets but are not upgrade chores are cross-referenced from the new strands: bd-1vwtdwtq (preview still loads mermaid from jsDelivr), bd-d88bj5r1 (replace Algolia Autocomplete), and bd-hva0 (local MathJax/KaTeX).

## Discovery

Nothing new. All seven CDN URLs in Rust sources are registered. The embed sweep outside the coverage roots found only repo-native content (project and listing templates, grammar query files, `quarto-core/resources/styles.css` (Posit-authored) and `quarto-color-mode.js`, lint fixtures, build-time embed dirs), plus the already-registered CSL style, Pandoc template and knitr scripts. The provenance-header sweep over `crates/` found nothing.

## Notes

- **Bootstrap is always `outdated` against npm**, by design. The actionable check is Quarto 1's banner at its latest tag. This is now written into the `bootstrap-scss` and `bootstrap-icons` notes, with the exact command.
- **Registry fix found by the survey.** `json-key` dotted paths cannot address `node_modules/reveal.js-menu`, because the segment contains a dot. `key` now also accepts an array of segments, with a lint test.
- **Skill refinement.** The embed sweep in step 2 was too noisy (about 60 template hits), so it now prints one line per target, filters project templates, and lists the known repo-native hits.
- **Pre-release tags.** quarto-cli's highest tag, v1.11.6, is a release. The tag parser ignores `-rc` style tags, but quarto-cli marks pre-releases only on GitHub, so a future survey should check the release page before treating a new tag as stable.
