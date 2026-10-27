/**
 * Regression tests for the sync connection-change handler (bd-lsf2v5n0).
 *
 * App registers one handler per project-open effect run; the handler then
 * outlives many renders. The pre-fix inline handler read `connectionError`
 * from its closure — frozen at registration time — so the
 * "Connection lost - working offline" banner never cleared on reconnect.
 * These tests pin the contract through a long-lived handler over real
 * React state.
 *
 * @vitest-environment jsdom
 */

import { describe, it, expect } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useMemo, useState } from 'react';
import {
  CONNECTION_LOST_MESSAGE,
  createConnectionChangeHandler,
} from './connectionHandlers';

function harness(projectOpen: boolean) {
  return renderHook(() => {
    const [isOnline, setIsOnline] = useState(true);
    const [connectionError, setConnectionError] = useState<string | null>(null);
    // Registered once, like App's [project]-scoped effect registration: the
    // stale-closure trap is a long-lived handler that reads frozen state.
    const onConnectionChange = useMemo(
      () =>
        createConnectionChangeHandler({ projectOpen, setIsOnline, setConnectionError }),
      [],
    );
    return { isOnline, connectionError, onConnectionChange, setConnectionError };
  });
}

describe('createConnectionChangeHandler', () => {
  it('sets the offline banner when the connection drops with a project open', () => {
    const { result } = harness(true);
    act(() => result.current.onConnectionChange(false));
    expect(result.current.connectionError).toBe(CONNECTION_LOST_MESSAGE);
    expect(result.current.isOnline).toBe(false);
  });

  it('clears the offline banner when the connection is restored', () => {
    const { result } = harness(true);
    act(() => result.current.onConnectionChange(false));
    expect(result.current.connectionError).toBe(CONNECTION_LOST_MESSAGE);
    act(() => result.current.onConnectionChange(true));
    expect(result.current.connectionError).toBeNull();
    expect(result.current.isOnline).toBe(true);
  });

  it('leaves a different connection error in place across a reconnect', () => {
    const { result } = harness(true);
    act(() => result.current.setConnectionError('sync exploded'));
    act(() => result.current.onConnectionChange(true));
    expect(result.current.connectionError).toBe('sync exploded');
  });

  it('does not set the banner when no project is open', () => {
    const { result } = harness(false);
    act(() => result.current.onConnectionChange(false));
    expect(result.current.connectionError).toBeNull();
    expect(result.current.isOnline).toBe(false);
  });
});
