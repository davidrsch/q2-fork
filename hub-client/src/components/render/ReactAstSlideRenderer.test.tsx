/**
 * Regression test for the slide-renderer migration to the framework
 * `extractMetaString` helper (Plan 2D Phase 6.0d).
 *
 * Behavior change: the previous local `extractMetaString` (lifted out
 * in 6.0d) walked only `Str` / `Space` inside `MetaInlines`. Any
 * inline markup (`Emph`, `Strong`, `Code`, `Link`, ...) was silently
 * dropped, so a slide title authored as `# *Hello* world` produced
 * the empty string for the title-slide's `title` field. The lifted
 * `framework/extractMetaString` walks the full inlines list via
 * `inlinesToPlainText`, so the same input now renders `"Hello world"`.
 *
 * This locks the new behavior; if a future edit re-narrows the
 * MetaInlines walk, the test breaks.
 */

import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import type { PandocAST } from '@quarto/preview-renderer/framework';
import { parseSlides, SlideAst } from './ReactAstSlideRenderer';

describe('ReactAstSlideRenderer slide-title meta coercion', () => {
    it('preserves inline emphasis text in MetaInlines title (Plan 2D 6.0d)', () => {
        const ast: PandocAST = {
            'pandoc-api-version': [1, 23, 1],
            meta: {
                title: {
                    t: 'MetaInlines',
                    c: [
                        { t: 'Str', c: 'Hello' },
                        { t: 'Space' },
                        { t: 'Emph', c: [{ t: 'Str', c: 'world' }] },
                    ],
                },
            },
            blocks: [],
        };

        const slides = parseSlides(ast);

        expect(slides.length).toBeGreaterThan(0);
        const titleSlide = slides[0];
        expect(titleSlide.type).toBe('title');
        expect(titleSlide.title).toBe('Hello world');
    });

    it('still handles MetaString titles (no regression)', () => {
        const ast: PandocAST = {
            'pandoc-api-version': [1, 23, 1],
            meta: {
                title: { t: 'MetaString', c: 'My Doc' },
            },
            blocks: [],
        };

        const slides = parseSlides(ast);
        expect(slides[0].type).toBe('title');
        expect(slides[0].title).toBe('My Doc');
    });
});

/**
 * Slide-index clamping (bd-cp9wzu6o): when an edit shrinks the deck under
 * the current slide, the UI must never commit an out-of-bounds index —
 * the counter reads "current / total" with the clamped value immediately,
 * not one effect-pass later.
 *
 * @vitest-environment jsdom
 */
describe('SlideAst slide-index clamping', () => {
    // jsdom lacks ResizeObserver (AspectRatioScaler subscribes with one).
    vi.stubGlobal('ResizeObserver', class {
        observe() {}
        unobserve() {}
        disconnect() {}
    });

    afterEach(cleanup);

    function deckAst(headings: string[]): PandocAST {
        return {
            'pandoc-api-version': [1, 23, 1],
            meta: {},
            blocks: headings.map((title) => ({
                t: 'Header',
                c: [1, ['', [], []], [{ t: 'Str', c: title }]],
            })),
        } as unknown as PandocAST;
    }

    const threeSlides = JSON.stringify(deckAst(['One', 'Two', 'Three']));
    const twoSlides = JSON.stringify(deckAst(['One', 'Two']));

    it('uncontrolled: clamps the current slide when the deck shrinks', () => {
        const { rerender } = render(
            <SlideAst astJson={threeSlides} currentFilePath="deck.qmd" />,
        );

        // Navigate to the last slide.
        fireEvent.keyDown(window, { key: 'ArrowRight' });
        fireEvent.keyDown(window, { key: 'ArrowRight' });
        expect(screen.getByText('3 / 3')).toBeDefined();

        // An edit removes the last slide: the index clamps.
        rerender(<SlideAst astJson={twoSlides} currentFilePath="deck.qmd" />);
        expect(screen.getByText('2 / 2')).toBeDefined();
    });

    it('controlled: clamps the display even before the parent catches up', () => {
        // The parent holds currentSlide but does not echo onSlideChange back
        // (unlike Editor, which updates immediately): the renderer must not
        // commit the stale index.
        const onSlideChange = vi.fn();
        const { rerender } = render(
            <SlideAst
                astJson={threeSlides}
                currentFilePath="deck.qmd"
                currentSlide={2}
                onSlideChange={onSlideChange}
            />,
        );
        expect(screen.getByText('3 / 3')).toBeDefined();

        rerender(
            <SlideAst
                astJson={twoSlides}
                currentFilePath="deck.qmd"
                currentSlide={2}
                onSlideChange={onSlideChange}
            />,
        );

        expect(screen.getByText('2 / 2')).toBeDefined();
        // At the clamped last slide, the next-slide button is disabled.
        expect(screen.getByRole('button', { name: '→' })).toHaveProperty('disabled', true);
    });
});
