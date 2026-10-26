/**
 * Behavior tests for the announcement-bar half of
 * `resources/js/quarto-nav/quarto-nav.js`
 * (bd-website-announcement-w1gtpzki): reveal, remembered dismissal,
 * per-view dismissal (`data-announcement-remember="false"`), and
 * re-registration when the banner element is replaced (the preview
 * re-mounting its chrome).
 *
 * Each case runs the real script in a fresh `JSDOM` window: the script
 * installs a body-wide MutationObserver that cannot be torn down, so
 * sharing the suite's global `document` would leak observers (and click
 * handlers) between cases. The source is read with `fs` rather than a
 * `?raw` import because Vite returns empty for `?raw` files outside the
 * package root (see `vitest.config.ts`).
 */

import { describe, expect, it } from 'vitest';
import { JSDOM } from 'jsdom';
import { readFileSync } from 'fs';
import path from 'path';

const NAV_JS = readFileSync(
    path.resolve(__dirname, '../../../../resources/js/quarto-nav/quarto-nav.js'),
    'utf-8',
);

function banner(id: string, opts: { remember?: boolean; dismissable?: boolean } = {}) {
    const remember = opts.remember === false ? ' data-announcement-remember="false"' : '';
    const close =
        opts.dismissable === false
            ? ''
            : '<button type="button" class="quarto-announcement-action" aria-label="Close announcement">' +
              '<i class="bi bi-x-lg" aria-hidden="true"></i></button>';
    return (
        `<div id="quarto-announcement" data-announcement-id="${id}"${remember} ` +
        `class="alert alert-primary hidden"><div class="quarto-announcement-content">Hi</div>` +
        `${close}</div>`
    );
}

async function page(bannerHtml: string, storage: Record<string, string> = {}) {
    const dom = new JSDOM(
        `<!doctype html><html><body><header id="quarto-header" class="headroom fixed-top">` +
            `${bannerHtml}</header><main>Body</main></body></html>`,
        { runScripts: 'outside-only', url: 'https://example.org/page.html' },
    );
    for (const [k, v] of Object.entries(storage)) {
        dom.window.localStorage.setItem(k, v);
    }
    dom.window.eval(NAV_JS);
    // A fresh JSDOM is still `loading`; the script defers to
    // DOMContentLoaded, as on a native page.
    if (dom.window.document.readyState === 'loading') {
        await new Promise((resolve) =>
            dom.window.document.addEventListener('DOMContentLoaded', resolve),
        );
    }
    return dom.window;
}

/** Let MutationObserver callbacks (microtasks) run. */
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('quarto-nav.js announcement bar', () => {
    it('reveals a banner with no remembered dismissal', async () => {
        const win = await page(banner('abc'));
        const el = win.document.getElementById('quarto-announcement');
        expect(el).not.toBeNull();
        expect(el!.classList.contains('hidden')).toBe(false);
    });

    it('removes a banner whose dismissal is remembered', async () => {
        const win = await page(banner('abc'), { 'quarto-announce-abc': 'true' });
        expect(win.document.getElementById('quarto-announcement')).toBeNull();
    });

    it('a dismissal for different content does not apply', async () => {
        const win = await page(banner('new'), { 'quarto-announce-old': 'true' });
        const el = win.document.getElementById('quarto-announcement');
        expect(el!.classList.contains('hidden')).toBe(false);
    });

    it('closing removes the banner and remembers it', async () => {
        const win = await page(banner('abc'));
        (win.document.querySelector('.quarto-announcement-action') as HTMLElement).click();
        expect(win.document.getElementById('quarto-announcement')).toBeNull();
        expect(win.localStorage.getItem('quarto-announce-abc')).toBe('true');
    });

    it('remember="false": closing removes the banner but stores nothing', async () => {
        const win = await page(banner('abc', { remember: false }));
        (win.document.querySelector('.quarto-announcement-action') as HTMLElement).click();
        expect(win.document.getElementById('quarto-announcement')).toBeNull();
        expect(win.localStorage.length).toBe(0);
    });

    it('remember="false": a stored dismissal is ignored', async () => {
        const win = await page(banner('abc', { remember: false }), { 'quarto-announce-abc': 'true' });
        const el = win.document.getElementById('quarto-announcement');
        expect(el!.classList.contains('hidden')).toBe(false);
    });

    it('a non-dismissable banner is revealed without a close control', async () => {
        const win = await page(banner('abc', { dismissable: false }));
        const el = win.document.getElementById('quarto-announcement');
        expect(el!.classList.contains('hidden')).toBe(false);
        expect(el!.querySelector('.quarto-announcement-action')).toBeNull();
    });

    it('a re-mounted banner element is revealed again', async () => {
        const win = await page(banner('abc', { remember: false }));
        (win.document.querySelector('.quarto-announcement-action') as HTMLElement).click();
        expect(win.document.getElementById('quarto-announcement')).toBeNull();

        // The preview replaces its chrome HTML wholesale on edits.
        const header = win.document.getElementById('quarto-header')!;
        header.innerHTML = banner('abc', { remember: false });
        await flush();

        const el = win.document.getElementById('quarto-announcement');
        expect(el!.classList.contains('hidden')).toBe(false);
        // And the new element's close control is wired.
        (el!.querySelector('.quarto-announcement-action') as HTMLElement).click();
        expect(win.document.getElementById('quarto-announcement')).toBeNull();
    });
});
