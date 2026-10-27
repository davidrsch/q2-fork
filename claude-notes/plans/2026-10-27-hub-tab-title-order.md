---
title: 'hub-client: project name before filename in the browser tab title'
date: 2026-10-27
description: 'GH #930. Swap the Editor tab title from "file — project — Quarto Hub" to "project — file — Quarto Hub" so narrow tabs show which project they belong to.'
---

Strand: **bd-d72tb7eh**. Issue: https://github.com/quarto-dev/q2/issues/930

## Overview

The Editor sets the browser tab title to
`index.qmd — My doc — Quarto Hub` (`Editor.tsx`, via `useDocumentTitle`).
Browsers truncate narrow tabs from the right, so with several hub tabs
open every tab reads `index.qmd …` and they are indistinguishable. The
project name is the more distinguishing part; put it first:
`My doc — index.qmd — Quarto Hub`.

The no-file case (`My doc — Quarto Hub`) and the default title restored
on unmount (`Quarto Hub`, #721) are unchanged.

## Work items

- [x] Extract the title format into a pure `editorDocumentTitle(projectName, filePath?)`
      helper next to `useDocumentTitle`, so the ordering is unit-tested
      rather than buried in the Editor component.
- [x] Swap the order; keep the basename-of-path behaviour.
- [x] Unit tests for both shapes (with file / without file, nested path).
- [x] Update the example titles in the existing hook tests and doc comment.
- [x] Run the hub-client unit tests for the hook.
