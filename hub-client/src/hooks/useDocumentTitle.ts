import { useEffect } from 'react';

/**
 * Default browser tab title, matching the `<title>` in index.html.
 * Restored whenever the component that set a specific title unmounts.
 */
export const DEFAULT_DOCUMENT_TITLE = 'Quarto Hub';

/**
 * Browser tab title for the Editor: "project — file — Quarto Hub", or
 * "project — Quarto Hub" when no file is open. `filePath` is reduced to
 * its basename.
 *
 * The project name comes first because browsers truncate narrow tabs
 * from the right; leading with the (often identical) filename made
 * several open hub tabs indistinguishable
 * (https://github.com/quarto-dev/q2/issues/930).
 */
export function editorDocumentTitle(projectName: string, filePath?: string): string {
  if (!filePath) {
    return `${projectName} — ${DEFAULT_DOCUMENT_TITLE}`;
  }
  const fileName = filePath.split('/').pop() || filePath;
  return `${projectName} — ${fileName} — ${DEFAULT_DOCUMENT_TITLE}`;
}

/**
 * Sets `document.title` while the calling component is mounted and
 * restores {@link DEFAULT_DOCUMENT_TITLE} on unmount.
 *
 * Views that set a specific title (e.g. the Editor's
 * "project — file — Quarto Hub") must not leave it behind after
 * unmount — otherwise the stale title persists on the project
 * selector (https://github.com/quarto-dev/q2/issues/721).
 */
export function useDocumentTitle(title: string): void {
  useEffect(() => {
    document.title = title;
    return () => {
      document.title = DEFAULT_DOCUMENT_TITLE;
    };
  }, [title]);
}
