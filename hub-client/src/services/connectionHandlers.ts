/**
 * Connection-change handling for the project sync client (bd-lsf2v5n0).
 *
 * App registers one handler per project-open effect run, so the callback
 * outlives the render that created it and must never READ React state
 * from its closure: the pre-fix inline handler compared against a
 * `connectionError` frozen at registration time, and the
 * "Connection lost - working offline" banner never cleared on reconnect.
 * All state reads here go through functional updaters.
 */

export const CONNECTION_LOST_MESSAGE = 'Connection lost - working offline';

export interface ConnectionChangeHandlerOptions {
  /** Whether a project was open when the handler was registered. */
  projectOpen: boolean;
  setIsOnline: (online: boolean) => void;
  setConnectionError: (
    updater: (prev: string | null) => string | null,
  ) => void;
}

export function createConnectionChangeHandler({
  projectOpen,
  setIsOnline,
  setConnectionError,
}: ConnectionChangeHandlerOptions): (connected: boolean) => void {
  return (connected) => {
    setIsOnline(connected);
    if (!connected && projectOpen) {
      setConnectionError(() => CONNECTION_LOST_MESSAGE);
    } else if (connected) {
      // Clear only our own offline banner; a different error reported via
      // onError must survive the reconnect.
      setConnectionError((prev) =>
        prev === CONNECTION_LOST_MESSAGE ? null : prev,
      );
    }
  };
}
