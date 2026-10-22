/**
 * Jump to a same-document anchor in a preview iframe.
 *
 * The preview iframes intercept `#frag` clicks (`routeLinkClick` in
 * `iframeLinkHandlers.ts`), so this replaces the browser's native jump
 * and has to do both halves of it: scroll the target into view, and move
 * focus to it when it can take focus. Without the focus move a skip link
 * (`#quarto-skip-link` → `main[tabindex="-1"]`, bd-21n09dwj) scrolls but
 * leaves the next Tab in the navigation it was meant to bypass.
 * `focus()` is a no-op on elements that can't take focus (a section
 * heading, say), so ordinary TOC and cross-reference links keep focus
 * where it was, as they do natively.
 *
 * Returns whether the target exists.
 */
export function jumpToAnchor(doc: Document, anchor: string): boolean {
    const element = doc.getElementById(anchor);
    if (!element) return false;
    element.scrollIntoView({ behavior: 'instant', block: 'start' });
    // Duck-typed, not `instanceof HTMLElement`: callers in the host window
    // pass an iframe's document, whose elements belong to another realm.
    (element as HTMLElement).focus?.({ preventScroll: true });
    return true;
}
