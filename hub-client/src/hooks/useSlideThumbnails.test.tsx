/**
 * Tests for useSlideThumbnails — the slide → thumbnail map behind the
 * outline panel. Rendering and rasterising are stubbed (renderSlide → null,
 * html2canvas → a controllable promise); what is under test is which slides
 * get captured, what line they are keyed by, and that a capture requested
 * while one is running is re-run afterwards instead of being dropped
 * (bd-cursor-slide-untitled-heading-p2wuuc13).
 *
 * @vitest-environment jsdom
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import html2canvas from 'html2canvas';
import { useSlideThumbnails } from './useSlideThumbnails';

vi.mock('html2canvas', () => ({ default: vi.fn() }));
vi.mock('../components/render/ReactAstSlideRenderer', () => ({ renderSlide: () => null }));

const html2canvasMock = vi.mocked(html2canvas);

/** A deck whose only located slide has its heading on 1-based `headingLine`. */
function deckJson(title: string, headingLine: number): string {
  return JSON.stringify({
    'pandoc-api-version': [1, 23, 1],
    meta: {},
    blocks: [
      {
        t: 'Div',
        c: [['title-slide', ['section', 'title-slide', 'center'], []], [{ t: 'Header', c: [1, ['', ['title'], []], [{ t: 'Str', c: title }]] }]],
      },
      {
        t: 'Div',
        c: [
          ['slide', ['section'], []],
          [{ t: 'Header', c: [2, ['', [], []], [{ t: 'Str', c: 'Slide' }]], l: { f: 0, b: { o: 0, l: headingLine, c: 1 }, e: { o: 0, l: headingLine, c: 9 } } }],
        ],
      },
    ],
  });
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => { resolve = r; });
  return { promise, resolve };
}

const fakeCanvas = () => document.createElement('canvas') as HTMLCanvasElement;

// Matches the debounce in useSlideThumbnails plus the render-settle delay.
const DEBOUNCE_MS = 100;
const SETTLE_MS = 50;

async function advance(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

describe('useSlideThumbnails', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    html2canvasMock.mockReset();
    // jsdom has no canvas implementation; the hook only needs drawImage + toDataURL.
    HTMLCanvasElement.prototype.getContext = (() => ({ drawImage: () => {} })) as unknown as typeof HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.toDataURL = () => 'data:image/png;base64,AAAA';
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('keys each located slide by its 0-based start line and skips the title slide', async () => {
    html2canvasMock.mockImplementation(async () => fakeCanvas());
    const { result } = renderHook(() =>
      useSlideThumbnails({ astJson: deckJson('Deck', 6), currentFilePath: 'index.qmd', previewReady: true, contentVersion: 1, enabled: true })
    );
    await advance(DEBOUNCE_MS + SETTLE_MS);
    expect(html2canvasMock).toHaveBeenCalledTimes(1);
    expect(Array.from(result.current.keys())).toEqual([5]);
  });

  it('does nothing when the format is not a slide deck', async () => {
    html2canvasMock.mockImplementation(async () => fakeCanvas());
    const { result } = renderHook(() =>
      useSlideThumbnails({ astJson: deckJson('Deck', 6), currentFilePath: 'index.qmd', previewReady: true, contentVersion: 1, enabled: false })
    );
    await advance(DEBOUNCE_MS + SETTLE_MS);
    expect(html2canvasMock).not.toHaveBeenCalled();
    expect(result.current.size).toBe(0);
  });

  it('re-runs a capture requested while one was in progress, against the newer deck', async () => {
    const first = deferred<HTMLCanvasElement>();
    html2canvasMock.mockImplementationOnce(() => first.promise).mockImplementation(async () => fakeCanvas());

    const { result, rerender } = renderHook(
      (props: { astJson: string; contentVersion: number }) =>
        useSlideThumbnails({ ...props, currentFilePath: 'index.qmd', previewReady: true, enabled: true }),
      { initialProps: { astJson: deckJson('Deck', 6), contentVersion: 1 } }
    );
    // First capture starts and blocks inside html2canvas.
    await advance(DEBOUNCE_MS + SETTLE_MS);
    expect(html2canvasMock).toHaveBeenCalledTimes(1);

    // The deck changes while that capture is still running.
    rerender({ astJson: deckJson('Deck', 10), contentVersion: 2 });
    await advance(DEBOUNCE_MS);
    expect(html2canvasMock).toHaveBeenCalledTimes(1); // still blocked, request queued

    // Let the stale capture finish: the queued one must run with the new deck.
    await act(async () => {
      first.resolve(fakeCanvas());
      await vi.advanceTimersByTimeAsync(SETTLE_MS);
    });
    expect(html2canvasMock).toHaveBeenCalledTimes(2);
    expect(Array.from(result.current.keys())).toEqual([9]);
  });
});
