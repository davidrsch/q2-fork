/**
 * Accessibility tests for ShareDialog.
 *
 * Regression coverage for the WCAG 2.2 audit findings:
 * - 4.1.2: the dialog exposes role/aria-modal/aria-labelledby and the
 *   close button has an accessible name (previously only "×" text)
 *
 * @vitest-environment jsdom
 */

import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent } from '@testing-library/react';
import ShareDialog from './ShareDialog';

afterEach(cleanup);
// Restore prototype spies between tests: vi.spyOn on an already-spied
// method returns the SAME mock, so without this a later test's spy would
// inherit the earlier test's recorded calls.
afterEach(() => {
  vi.restoreAllMocks();
});

const defaultProps = {
  isOpen: true,
  shareableUrl: 'https://hub.example.com/p/abc#key',
  onClose: vi.fn(),
};

describe('ShareDialog accessibility', () => {
  it('exposes dialog semantics with the title as accessible name', () => {
    render(<ShareDialog {...defaultProps} />);
    const dialog = screen.getByRole('dialog');
    expect(dialog.getAttribute('aria-modal')).toBe('true');
    const labelId = dialog.getAttribute('aria-labelledby');
    expect(labelId).toBeTruthy();
    expect(document.getElementById(labelId!)?.textContent).toBe('Share project');
  });

  it('gives the close button an accessible name', () => {
    render(<ShareDialog {...defaultProps} />);
    expect(screen.getByRole('button', { name: 'Close' })).toBeDefined();
  });

  it('does not render when closed', () => {
    render(<ShareDialog {...defaultProps} isOpen={false} />);
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});

describe('ShareDialog variants', () => {
  it('uses project wording by default', () => {
    render(<ShareDialog {...defaultProps} />);
    expect(screen.getByText('Share project')).toBeDefined();
    expect(
      screen.getByText(/access and edit this project permanently/)
    ).toBeDefined();
  });

  it('uses project-set wording for the link-another-browser variant', () => {
    render(<ShareDialog {...defaultProps} variant="projectSet" />);
    expect(screen.getByText('Link another browser')).toBeDefined();
    expect(
      screen.getByText(/access and edit every project in your project list permanently/)
    ).toBeDefined();
    expect(screen.queryByText('Share project')).toBeNull();
  });
});

describe('ShareDialog select-all timeout (bd-lsf2v5n0)', () => {
  it('selects the URL text shortly after the dialog opens', () => {
    vi.useFakeTimers();
    try {
      const selectSpy = vi.spyOn(HTMLInputElement.prototype, 'select');
      render(<ShareDialog {...defaultProps} />);
      expect(selectSpy).not.toHaveBeenCalled();
      vi.advanceTimersByTime(100);
      expect(selectSpy).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it('cancels the pending select-all when the dialog closes and reopens quickly', () => {
    vi.useFakeTimers();
    try {
      const selectSpy = vi.spyOn(HTMLInputElement.prototype, 'select');
      const { rerender } = render(<ShareDialog {...defaultProps} isOpen={true} />);
      // Close before the 100ms select-all fires, then reopen: without
      // cleanup the first timer still lands on the remounted input, so the
      // text gets selected twice.
      vi.advanceTimersByTime(50);
      rerender(<ShareDialog {...defaultProps} isOpen={false} />);
      vi.advanceTimersByTime(10);
      rerender(<ShareDialog {...defaultProps} isOpen={true} />);
      vi.advanceTimersByTime(200);
      expect(selectSpy).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });
});

describe('ShareDialog Enter handling (GH #635 hardening)', () => {
  // Same contract as NewFileDialog (see ModalDialog's onKeyDown docs):
  // prevent Enter's default-action click, and leave button keydowns to
  // the button's own activation. ShareDialog's close is deferred 500ms
  // so the reopen bug can't bite today; these tests keep it that way if
  // the delay ever goes away.
  const stubClipboard = () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    });
    return writeText;
  };

  it('copies on Enter in the URL input and prevents the default action', () => {
    const writeText = stubClipboard();
    render(<ShareDialog {...defaultProps} />);

    const urlInput = screen.getByDisplayValue(defaultProps.shareableUrl);
    const notPrevented = fireEvent.keyDown(urlInput, { key: 'Enter' });

    expect(writeText).toHaveBeenCalledWith(defaultProps.shareableUrl);
    expect(notPrevented).toBe(false);
  });

  it('does not copy when Enter is pressed on the close button', () => {
    const writeText = stubClipboard();
    render(<ShareDialog {...defaultProps} />);

    const closeButton = screen.getByRole('button', { name: 'Close' });
    closeButton.focus();
    fireEvent.keyDown(closeButton, { key: 'Enter' });

    expect(writeText).not.toHaveBeenCalled();
  });
});
