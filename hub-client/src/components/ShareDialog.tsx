/**
 * Share Dialog Component
 *
 * Modal dialog for sharing with a shareable URL. Two text variants:
 * 'project' (default) shares a single project; 'projectSet' is the
 * link-another-browser flow, whose link grants access to the whole
 * project list — the warning wording broadens accordingly.
 * Displays a warning about permanent access and allows copying the link.
 */

import { useState, useCallback, useEffect } from 'react';
import ModalDialog from './ModalDialog';
import { common, dialogs } from '../strings';
import './ShareDialog.css';

export interface ShareDialogProps {
  isOpen: boolean;
  shareableUrl: string | undefined;
  onClose: () => void;
  onCopied?: () => void;
  /** Which text to show; see the component comment. Default 'project'. */
  variant?: 'project' | 'projectSet';
}

export default function ShareDialog({
  isOpen,
  shareableUrl,
  onClose,
  onCopied,
  variant = 'project',
}: ShareDialogProps) {
  // Parents mount this dialog only while it is open, so every open is a
  // fresh mount and `copied` starts false — no reset effect (bd-cp9wzu6o).
  const [copied, setCopied] = useState(false);
  // The input is tracked as state (not a ref) because it mounts a commit
  // after the dialog does — ModalDialog resolves its portal target from a
  // hidden marker first — so an effect keyed only on `isOpen` would run
  // before the input exists and never select (bd-lsf2v5n0).
  const [urlInput, setUrlInput] = useState<HTMLInputElement | null>(null);

  // Select all text shortly after the dialog opens; the timeout is
  // cleared on close/unmount so it can't fire twice across a rapid
  // close/reopen or land on a detached input.
  useEffect(() => {
    if (!isOpen || !urlInput) return;
    const timer = setTimeout(() => urlInput.select(), 100);
    return () => clearTimeout(timer);
  }, [isOpen, urlInput]);

  const handleCopyLink = useCallback(async () => {
    if (!shareableUrl) return;
    try {
      // Try modern Clipboard API first
      if (navigator.clipboard && navigator.clipboard.writeText) {
        await navigator.clipboard.writeText(shareableUrl);
      } else {
        // Fallback for older browsers or HTTP contexts
        const textArea = document.createElement('textarea');
        textArea.value = shareableUrl;
        textArea.style.position = 'fixed';
        textArea.style.left = '-9999px';
        document.body.appendChild(textArea);
        textArea.select();
        document.execCommand('copy');
        document.body.removeChild(textArea);
      }

      setCopied(true);
      onCopied?.();

      // Close dialog after a brief delay to show the success state
      setTimeout(() => {
        onClose();
      }, 500);
    } catch (err) {
      console.error('Failed to copy to clipboard:', err);
    }
  }, [shareableUrl, onCopied, onClose]);

  // Enter copies the link; Escape and Tab containment are owned by ModalDialog.
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter') {
        // Buttons handle their own Enter activation (see the onKeyDown
        // contract in ModalDialog). The copy path only closes after a
        // 500ms success-state delay, but prevent the default anyway so
        // the synthesized click can never land on the restore target if
        // that delay ever goes away (GH #635 hardening).
        if (e.target instanceof HTMLButtonElement) return;
        e.preventDefault();
        handleCopyLink();
      }
    },
    [handleCopyLink]
  );

  if (!isOpen || !shareableUrl) return null;

  const text =
    variant === 'projectSet'
      ? { ...dialogs.share, ...dialogs.linkBrowser }
      : dialogs.share;

  return (
    <ModalDialog
      title={text.title}
      className="share-dialog"
      onClose={onClose}
      onKeyDown={handleKeyDown}
    >
        <div className="dialog-content">
          <div className="warning-box">
            <span className="warning-icon">&#9888;</span>
            <p>
              <strong>{text.warning}</strong>
            </p>
            <p className="warning-detail">
              {text.warningDetail}
            </p>
          </div>

          <div className="url-field">
            <label htmlFor="shareable-url">{text.linkLabel}</label>
            <input
              ref={setUrlInput}
              id="shareable-url"
              type="text"
              className="qh-input focus-accent"
              value={shareableUrl}
              readOnly
              onClick={(e) => (e.target as HTMLInputElement).select()}
            />
          </div>
        </div>

        <div className="dialog-actions">
          <button className="qh-btn outline" onClick={onClose}>
            {common.cancel}
          </button>
          <button
            className={`qh-btn primary copy-btn ${copied ? 'copied' : ''}`}
            onClick={handleCopyLink}
          >
            {copied ? text.copied : text.copyLink}
          </button>
        </div>
    </ModalDialog>
  );
}
