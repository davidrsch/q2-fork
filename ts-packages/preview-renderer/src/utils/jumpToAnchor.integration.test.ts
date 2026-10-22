/**
 * bd-21n09dwj: same-document anchor clicks in the preview iframes are
 * intercepted (`routeLinkClick`), so the helper that replaces the native
 * jump must also move focus the way the browser would: onto the target
 * when it can take focus (the skip link's `main[tabindex="-1"]`), and
 * nowhere otherwise.
 */

import { describe, expect, it, beforeEach, vi } from 'vitest';
import { jumpToAnchor } from './jumpToAnchor';

beforeEach(() => {
    document.body.innerHTML = `
        <a id="link" href="#quarto-document-content">Skip</a>
        <h2 id="sec">Section</h2>
        <main id="quarto-document-content" tabindex="-1">Body</main>
    `;
    Element.prototype.scrollIntoView = vi.fn();
});

describe('jumpToAnchor', () => {
    it('scrolls to and focuses a focusable target', () => {
        const main = document.getElementById('quarto-document-content')!;
        expect(jumpToAnchor(document, 'quarto-document-content')).toBe(true);
        expect(main.scrollIntoView).toHaveBeenCalled();
        expect(document.activeElement).toBe(main);
    });

    it('scrolls to a non-focusable target without moving focus', () => {
        const link = document.getElementById('link') as HTMLAnchorElement;
        link.focus();
        expect(jumpToAnchor(document, 'sec')).toBe(true);
        expect(document.getElementById('sec')!.scrollIntoView).toHaveBeenCalled();
        expect(document.activeElement).toBe(link);
    });

    it('returns false for a missing target', () => {
        expect(jumpToAnchor(document, 'nope')).toBe(false);
    });
});
