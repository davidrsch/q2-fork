/**
 * Tests for useDocumentTitle.
 *
 * Regression coverage for https://github.com/quarto-dev/q2/issues/721:
 * the browser titlebar kept showing the last-opened project/file after
 * exiting to the project selector, because nothing restored the
 * default title when the Editor unmounted.
 *
 * @vitest-environment jsdom
 */

import { describe, expect, it } from 'vitest';
import { renderHook } from '@testing-library/react';

import {
  DEFAULT_DOCUMENT_TITLE,
  editorDocumentTitle,
  useDocumentTitle,
} from './useDocumentTitle';

describe('useDocumentTitle', () => {
  it('sets document.title while mounted', () => {
    renderHook(() => useDocumentTitle('my project — index.qmd — Quarto Hub'));
    expect(document.title).toBe('my project — index.qmd — Quarto Hub');
  });

  it('updates document.title when the title changes', () => {
    const { rerender } = renderHook(({ title }) => useDocumentTitle(title), {
      initialProps: { title: 'proj — a.qmd — Quarto Hub' },
    });
    expect(document.title).toBe('proj — a.qmd — Quarto Hub');

    rerender({ title: 'proj — b.qmd — Quarto Hub' });
    expect(document.title).toBe('proj — b.qmd — Quarto Hub');
  });

  it('restores the default title on unmount', () => {
    const { unmount } = renderHook(() =>
      useDocumentTitle('my project — index.qmd — Quarto Hub'),
    );
    expect(document.title).not.toBe(DEFAULT_DOCUMENT_TITLE);

    unmount();
    expect(document.title).toBe(DEFAULT_DOCUMENT_TITLE);
  });
});

// https://github.com/quarto-dev/q2/issues/930: the project name leads so
// that narrow (right-truncated) tabs stay distinguishable.
describe('editorDocumentTitle', () => {
  it('puts the project name before the filename', () => {
    expect(editorDocumentTitle('My doc', 'index.qmd')).toBe(
      'My doc — index.qmd — Quarto Hub',
    );
  });

  it('uses only the basename of a nested path', () => {
    expect(editorDocumentTitle('My doc', 'chapters/intro.qmd')).toBe(
      'My doc — intro.qmd — Quarto Hub',
    );
  });

  it('omits the file segment when no file is open', () => {
    expect(editorDocumentTitle('My doc')).toBe('My doc — Quarto Hub');
  });
});
