# Repro: raw HTML `<img>` in config regions (bd-5r97wehd)

Run from this directory: `../../../../target/debug/q2 render` (or `q2 render`).
Then `ls -R _site/images _site/_extensions` and compare with the expectation
that every `*-raw.svg` is copied and `missing-raw.svg` raises Q-5-6.
