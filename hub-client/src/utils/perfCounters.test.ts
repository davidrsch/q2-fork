/**
 * Tests for the perf counters utility.
 *
 * @vitest-environment jsdom
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  perfEnabled,
  perfNow,
  perfSegment,
  perfMark,
  perfExtra,
  perfDump,
  perfReset,
  setPerfOverrideForTests,
} from './perfCounters';

beforeEach(() => {
  setPerfOverrideForTests(true);
  perfReset();
});

afterEach(() => {
  setPerfOverrideForTests(null);
  perfReset();
  vi.restoreAllMocks();
});

describe('perf counters', () => {
  it('perfNow returns a timestamp when enabled', () => {
    expect(perfNow()).toBeGreaterThan(0);
  });

  it('perfNow returns 0 when disabled, so callers can skip branching', () => {
    setPerfOverrideForTests(false);
    expect(perfEnabled()).toBe(false);
    expect(perfNow()).toBe(0);
    // And recording is a no-op.
    perfSegment('x', 0);
    perfMark('y');
    perfExtra('y', 'k');
    const log = vi.spyOn(console, 'log').mockImplementation(() => {});
    perfDump();
    expect(log).not.toHaveBeenCalled();
  });

  it('accumulates segment runs, totals and maxima', () => {
    perfSegment('effect.x', 10, 15); // 5ms
    perfSegment('effect.x', 20, 24); // 4ms
    const log = vi.spyOn(console, 'log').mockImplementation(() => {});
    perfDump();
    expect(log).toHaveBeenCalledTimes(1);
    const line = log.mock.calls[0][0] as string;
    expect(line).toMatch(/^perf\.effect\.x runs=2 /);
    expect(line).toContain('total_ms=9.000');
    expect(line).toContain('avg_ms=4.500');
    expect(line).toContain('max_ms=5.000');
  });

  it('measures up to the call when no end is passed', () => {
    perfSegment('effect.x', performance.now() - 1);
    const log = vi.spyOn(console, 'log').mockImplementation(() => {});
    perfDump();
    const line = log.mock.calls[0][0] as string;
    expect(line).toMatch(/runs=1 total_ms=/);
  });

  it('marks count without a duration and omit timing fields', () => {
    perfMark('effect.y.early-out');
    perfMark('effect.y.early-out');
    const log = vi.spyOn(console, 'log').mockImplementation(() => {});
    perfDump();
    expect(log.mock.calls[0][0]).toBe('perf.effect.y.early-out runs=2');
  });

  it('extras accumulate as sorted key=value columns', () => {
    perfExtra('effect.z', 'changed', 2);
    perfExtra('effect.z', 'files', 100);
    perfExtra('effect.z', 'changed', 1);
    perfMark('effect.z');
    const log = vi.spyOn(console, 'log').mockImplementation(() => {});
    perfDump();
    expect(log.mock.calls[0][0]).toBe(
      'perf.effect.z runs=1 changed=3 files=100',
    );
  });

  it('dumps gauges sorted by name', () => {
    perfMark('b');
    perfMark('a');
    const log = vi.spyOn(console, 'log').mockImplementation(() => {});
    perfDump();
    expect(log.mock.calls.map((c) => c[0])).toEqual(['perf.a runs=1', 'perf.b runs=1']);
  });

  it('reset clears all gauges', () => {
    perfMark('a');
    perfReset();
    const log = vi.spyOn(console, 'log').mockImplementation(() => {});
    perfDump();
    expect(log).not.toHaveBeenCalled();
  });
});
