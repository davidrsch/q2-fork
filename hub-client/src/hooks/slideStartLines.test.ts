/**
 * Tests for slideStartLines — the cursor-line ↔ reveal-slide mapping read
 * off the transformed q2-slides AST.
 *
 * The fixtures mirror what `render_qmd_to_preview_ast` emits for the
 * `q2-slides` pseudo-format (captured 2026-10-20 for
 * bd-cursor-slide-untitled-heading-p2wuuc13): one generated section Div per
 * horizontal slide, no `l` on the generated Divs or on the title slide, and
 * the original `l` on every heading and content block. Line numbers in the
 * builders are 1-based like `l.b.l`; expectations are 0-based like Monaco.
 */

import { describe, it, expect } from 'vitest';
import type { BlockNode, PandocAST } from '@quarto/preview-renderer/framework';
import {
  buildSlideMappings,
  firstSourceLine,
  slideIndexForLine,
  slideSourceLines,
} from './slideStartLines';

const loc = (line: number) => ({
  l: { f: 0, b: { o: 0, l: line, c: 1 }, e: { o: 0, l: line, c: 1 } },
});

const str = (text: string) => ({ t: 'Str', c: text });

/** A heading; `line` omitted = generated (no location). */
function header(level: number, inlines: unknown[], line?: number): BlockNode {
  return {
    t: 'Header',
    c: [level, ['', [], []], inlines],
    ...(line === undefined ? {} : loc(line)),
  } as BlockNode;
}

function para(text: string, line: number): BlockNode {
  return { t: 'Para', c: [str(text)], ...loc(line) } as BlockNode;
}

function section(id: string, blocks: BlockNode[], classes: string[] = ['section']): BlockNode {
  return { t: 'Div', c: [[id, classes, []], blocks] } as BlockNode;
}

const titleSlide = section(
  'title-slide',
  [header(1, [str('Repro deck')])],
  ['section', 'title-slide', 'center']
);

function deck(blocks: BlockNode[]): PandocAST {
  return { 'pandoc-api-version': [1, 23, 1], meta: {}, blocks };
}

/**
 * ---            (line 1)
 * title: Repro deck
 * format: revealjs
 * ---
 *
 * ## First slide  (line 6)
 *
 * First body.     (line 8)
 *
 * ##              (line 10)
 *
 * Untitled body.  (line 12)
 *
 * ## Third slide  (line 14)
 *
 * Third body.     (line 16)
 *
 * ## Fourth slide (line 18)
 *
 * Fourth body.    (line 20)
 */
const untitledDeck = deck([
  titleSlide,
  section('first-slide', [header(2, [str('First'), { t: 'Space' }, str('slide')], 6), para('First body.', 8)]),
  section('section', [header(2, [], 10), para('Untitled body.', 12)]),
  section('third-slide', [header(2, [str('Third'), { t: 'Space' }, str('slide')], 14), para('Third body.', 16)]),
  section('fourth-slide', [header(2, [str('Fourth'), { t: 'Space' }, str('slide')], 18), para('Fourth body.', 20)]),
]);

describe('firstSourceLine', () => {
  it('returns the 0-based line of the node itself when located', () => {
    expect(firstSourceLine(para('x', 8))).toBe(7);
  });

  it('descends into generated wrappers to the first located child', () => {
    expect(firstSourceLine(section('s', [header(2, [str('T')], 6), para('x', 8)]))).toBe(5);
    expect(firstSourceLine(section('outer', [section('inner', [para('x', 24)])]))).toBe(23);
  });

  it('is null for a subtree with no locations', () => {
    expect(firstSourceLine(titleSlide)).toBeNull();
    expect(firstSourceLine({ t: 'HorizontalRule' })).toBeNull();
    expect(firstSourceLine(null)).toBeNull();
  });

  it('ignores malformed location fields', () => {
    expect(firstSourceLine({ t: 'Para', c: [], l: { b: { l: 'nope' } } })).toBeNull();
    expect(firstSourceLine({ t: 'Para', c: [], l: null })).toBeNull();
  });
});

describe('slideSourceLines', () => {
  it('gives one entry per top-level block, null for the generated title slide', () => {
    expect(slideSourceLines(untitledDeck)).toEqual([null, 5, 9, 13, 17]);
  });
});

describe('buildSlideMappings + slideIndexForLine', () => {
  const lookup = (ast: PandocAST, line: number) => slideIndexForLine(buildSlideMappings(ast), line);

  it('bd-cursor-slide-untitled-heading-p2wuuc13: slides after an untitled heading map to their own index', () => {
    expect(buildSlideMappings(untitledDeck)).toEqual([
      { startLine: 0, slideIndex: 0 },
      { startLine: 5, slideIndex: 1 },
      { startLine: 9, slideIndex: 2 },
      { startLine: 13, slideIndex: 3 },
      { startLine: 17, slideIndex: 4 },
    ]);
    // Cursor on each body paragraph (0-based lines 7, 11, 15, 19).
    expect(lookup(untitledDeck, 7)).toBe(1);
    expect(lookup(untitledDeck, 11)).toBe(2); // the untitled slide itself
    expect(lookup(untitledDeck, 15)).toBe(3); // not 2
    expect(lookup(untitledDeck, 19)).toBe(4); // last slide is reachable
    // The heading line belongs to its own slide.
    expect(lookup(untitledDeck, 9)).toBe(2);
    expect(lookup(untitledDeck, 13)).toBe(3);
  });

  it('maps the front matter to the title slide', () => {
    expect(lookup(untitledDeck, 0)).toBe(0);
    expect(lookup(untitledDeck, 4)).toBe(0);
  });

  it('handles image-only and attribute-only headings the same way', () => {
    const image = { t: 'Image', c: [['', [], []], [str('Picture')], ['missing.png', '']] };
    const imageDeck = deck([
      titleSlide,
      section('first-slide', [header(2, [str('First')], 6), para('First body.', 8)]),
      section('picture', [header(2, [image], 10), para('Image body.', 12)]),
      section('third-slide', [header(2, [str('Third')], 14), para('Third body.', 16)]),
    ]);
    expect(lookup(imageDeck, 11)).toBe(2);
    expect(lookup(imageDeck, 15)).toBe(3);

    const bgDeck = deck([
      titleSlide,
      section('first-slide', [header(2, [str('First')], 6), para('First body.', 8)]),
      section('section', [
        { t: 'Header', c: [2, ['', [], [['background-image', 'missing.png']]], []], ...loc(10) } as BlockNode,
        para('Bg body.', 12),
      ]),
      section('third-slide', [header(2, [str('Third')], 14), para('Third body.', 16)]),
    ]);
    expect(lookup(bgDeck, 11)).toBe(2);
    expect(lookup(bgDeck, 15)).toBe(3);
  });

  it('maps a preamble slide and a # Section stack by their first located node', () => {
    /**
     * Preamble paragraph.  (line 6)
     * # Section one        (line 8)
     * ## Sub A             (line 10)   A body. (12)
     * ## Sub B             (line 14)   B body. (16)
     * ## After             (line 18)   After body. (20)
     * ---                  (line 22)
     * Split by rule.       (line 24)
     */
    const stackDeck = deck([
      titleSlide,
      section('', [para('Preamble paragraph.', 6)]),
      section('section-one', [
        section('section-one', [header(1, [str('Section one')], 8)]),
        section('sub-a', [header(2, [str('Sub A')], 10), para('A body.', 12)]),
        section('sub-b', [header(2, [str('Sub B')], 14), para('B body.', 16)]),
        section('after', [header(2, [str('After')], 18), para('After body.', 20)]),
        section('', [para('Split by rule.', 24)]),
      ]),
    ]);
    expect(buildSlideMappings(stackDeck)).toEqual([
      { startLine: 0, slideIndex: 0 },
      { startLine: 5, slideIndex: 1 },
      { startLine: 7, slideIndex: 2 },
    ]);
    expect(lookup(stackDeck, 5)).toBe(1);
    // Vertical children all resolve to the stack's horizontal index
    // (bd-cursor-slide-vertical-index-wykcyjxb tracks mirroring `v` too).
    expect(lookup(stackDeck, 15)).toBe(2);
    expect(lookup(stackDeck, 23)).toBe(2);
  });

  it('owns the lines above the first slide when there is no title slide', () => {
    const noTitle = deck([section('only-slide', [header(2, [str('Only slide')], 5), para('Body.', 7)])]);
    expect(buildSlideMappings(noTitle)).toEqual([{ startLine: 4, slideIndex: 0 }]);
    expect(lookup(noTitle, 0)).toBe(0);
    expect(lookup(noTitle, 6)).toBe(0);
  });

  it('skips an unlocated slide that is not the first, leaving it unreachable', () => {
    const generatedMiddle = deck([
      section('a', [header(2, [str('A')], 6)]),
      section('gen', [{ t: 'Para', c: [str('generated')] } as BlockNode]),
      section('b', [header(2, [str('B')], 14)]),
    ]);
    expect(buildSlideMappings(generatedMiddle)).toEqual([
      { startLine: 5, slideIndex: 0 },
      { startLine: 13, slideIndex: 2 },
    ]);
    expect(lookup(generatedMiddle, 10)).toBe(0);
  });

  it('returns null for a deck with no slides', () => {
    expect(buildSlideMappings(deck([]))).toEqual([]);
    expect(lookup(deck([]), 3)).toBeNull();
  });
});
