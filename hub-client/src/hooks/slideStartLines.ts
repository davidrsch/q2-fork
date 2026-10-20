/**
 * Slide → source-line mapping for the revealjs (`q2-slides`) preview.
 *
 * The transformed AST's top-level blocks are, one for one, the horizontal
 * slides the preview renders: `RevealDeck` (ts-packages/preview-renderer,
 * `q2-preview/RevealDeck.tsx`) maps `ast.blocks` by index, so a block's array
 * index *is* the reveal `h` index that the `SET_SLIDE` / `SLIDE_CHANGED`
 * messages carry. Every node that came from the source carries an `l`
 * location (`framework/sourceLoc.ts` documents the shape; `l.b.l` is the
 * 1-based start line). Generated wrappers — the section Divs, the title
 * slide — have none, but the heading and content inside a section keep
 * theirs, so a slide's start line is the line of its first located
 * descendant.
 *
 * Earlier versions paired `parseSlides()` output with the LSP header symbols
 * by index. An untitled heading (`##`, `## ![alt](img)`,
 * `## {background-image="…"}`) produces no symbol, so every later slide
 * inherited the next header's line and the preview landed one slide early
 * (bd-cursor-slide-untitled-heading-p2wuuc13). Reading the lines off the AST
 * the preview actually rendered removes the pairing altogether.
 */

import type { PandocAST } from '@quarto/preview-renderer/framework';

export interface SlideMapping {
  /** 0-based editor line where this slide begins. */
  startLine: number;
  /** Horizontal reveal index (= index into `ast.blocks`). */
  slideIndex: number;
}

interface ResolvedLoc {
  b?: { l?: unknown };
}

/** 0-based start line of `node`'s own `l` location, or null if it has none. */
function ownStartLine(node: object): number | null {
  const loc = (node as { l?: ResolvedLoc }).l;
  const line = loc?.b?.l;
  return typeof line === 'number' && Number.isFinite(line) ? line - 1 : null;
}

/**
 * 0-based start line of the first located node in a pre-order walk of
 * `value` (the node itself, then its children in document order), or null
 * when nothing under it came from the source.
 */
export function firstSourceLine(value: unknown): number | null {
  if (Array.isArray(value)) {
    for (const item of value) {
      const line = firstSourceLine(item);
      if (line !== null) return line;
    }
    return null;
  }
  if (value !== null && typeof value === 'object') {
    const own = ownStartLine(value);
    if (own !== null) return own;
    for (const [key, child] of Object.entries(value)) {
      if (key === 'l') continue;
      const line = firstSourceLine(child);
      if (line !== null) return line;
    }
  }
  return null;
}

/**
 * Per top-level block (= per horizontal slide), the 0-based source line the
 * slide starts on, or null for a slide with no source text (the generated
 * title slide).
 */
export function slideSourceLines(ast: PandocAST): (number | null)[] {
  return ast.blocks.map(firstSourceLine);
}

/**
 * Cursor-line → slide mappings, ascending by line. The first slide always
 * owns line 0 (a generated title slide has no line of its own, and the
 * front matter above the first heading belongs to whatever comes first).
 * Any other slide without a source line is left out: the cursor can never
 * be inside it.
 */
export function buildSlideMappings(ast: PandocAST): SlideMapping[] {
  const mappings: SlideMapping[] = [];
  slideSourceLines(ast).forEach((line, slideIndex) => {
    if (line !== null) {
      mappings.push({ startLine: line, slideIndex });
    } else if (slideIndex === 0) {
      mappings.push({ startLine: 0, slideIndex });
    }
  });
  // Document order already sorts them, but keep the lookup's precondition
  // explicit.
  mappings.sort((a, b) => a.startLine - b.startLine);
  return mappings;
}

/**
 * The slide the cursor on 0-based `line` belongs to: the last mapping that
 * starts at or before it, the first slide for lines above the first
 * mapping, or null when the deck has no slides at all.
 */
export function slideIndexForLine(mappings: SlideMapping[], line: number): number | null {
  if (mappings.length === 0) return null;
  for (let i = mappings.length - 1; i >= 0; i--) {
    if (mappings[i].startLine <= line) {
      return mappings[i].slideIndex;
    }
  }
  return mappings[0].slideIndex;
}
