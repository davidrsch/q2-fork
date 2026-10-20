/**
 * Tests for useCursorToSlide — the hook wrapper over slideStartLines.
 * The mapping arithmetic is covered in slideStartLines.test.ts; this checks
 * the hook's handling of missing / unparsable AST JSON and that it reads the
 * transformed AST alone (no symbols).
 *
 * @vitest-environment jsdom
 */

import { describe, it, expect, vi, afterEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useCursorToSlide } from './useCursorToSlide';

const deckJson = JSON.stringify({
  'pandoc-api-version': [1, 23, 1],
  meta: {},
  blocks: [
    { t: 'Div', c: [['title-slide', ['section', 'title-slide', 'center'], []], [{ t: 'Header', c: [1, ['', ['title'], []], [{ t: 'Str', c: 'Deck' }]] }]] },
    { t: 'Div', c: [['first', ['section'], []], [{ t: 'Header', c: [2, ['', [], []], [{ t: 'Str', c: 'First' }]], l: { f: 0, b: { o: 0, l: 6, c: 1 }, e: { o: 0, l: 6, c: 9 } } }]] },
    { t: 'Div', c: [['section', ['section'], []], [{ t: 'Header', c: [2, ['', [], []], []], l: { f: 0, b: { o: 0, l: 10, c: 1 }, e: { o: 0, l: 10, c: 3 } } }]] },
    { t: 'Div', c: [['third', ['section'], []], [{ t: 'Header', c: [2, ['', [], []], [{ t: 'Str', c: 'Third' }]], l: { f: 0, b: { o: 0, l: 14, c: 1 }, e: { o: 0, l: 14, c: 9 } } }]] },
  ],
});

describe('useCursorToSlide', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('returns null for every line when there is no AST', () => {
    const { result } = renderHook(() => useCursorToSlide(null));
    expect(result.current(0)).toBeNull();
    expect(result.current(42)).toBeNull();
  });

  it('returns null (and logs) when the AST JSON does not parse', () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    const { result } = renderHook(() => useCursorToSlide('{not json'));
    expect(result.current(3)).toBeNull();
    expect(error).toHaveBeenCalledOnce();
  });

  it('maps lines to the reveal index of the enclosing top-level block', () => {
    const { result } = renderHook(() => useCursorToSlide(deckJson));
    expect(result.current(2)).toBe(0); // front matter → title slide
    expect(result.current(7)).toBe(1);
    expect(result.current(11)).toBe(2); // untitled slide
    expect(result.current(15)).toBe(3); // not 2
  });

  it('keeps the same function identity while the AST is unchanged', () => {
    const { result, rerender } = renderHook(({ ast }) => useCursorToSlide(ast), {
      initialProps: { ast: deckJson },
    });
    const first = result.current;
    rerender({ ast: deckJson });
    expect(result.current).toBe(first);
    rerender({ ast: null });
    expect(result.current).not.toBe(first);
  });
});
