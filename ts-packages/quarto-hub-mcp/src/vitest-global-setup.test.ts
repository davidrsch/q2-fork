/**
 * Unit tests for scripts/vitest-global-setup.mjs (bd-yt3x1gwk): a failed
 * parser build skips the parser tier by default, but is fatal under
 * QUARTO_QMD_PARSER_REQUIRED=1 — CI's parser-tier step once reported a
 * parser that did not compile as 21 skipped tests on a green step. Offline
 * by construction: the build is a stub and the marker lives in a temp dir.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
// @ts-expect-error — plain .mjs module without type declarations
import { prepareQmdParser } from '../scripts/vitest-global-setup.mjs';

let tmp: string;
let marker: string;
const ok = () => undefined;
const fail = () => {
  throw new Error('wasm-pack build failed for wasm-qmd-parser\nmore detail');
};
const quiet = () => undefined;

beforeEach(() => {
  tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'qmd-parser-setup-test-'));
  marker = path.join(tmp, '.qmd-parser-unavailable');
});

afterEach(() => {
  fs.rmSync(tmp, { recursive: true, force: true });
});

describe('prepareQmdParser', () => {
  it('clears a stale marker when the build succeeds', () => {
    fs.writeFileSync(marker, 'old\n');
    prepareQmdParser({ ensure: ok, marker, required: false, warn: quiet });
    expect(fs.existsSync(marker)).toBe(false);
  });

  it('writes the marker with the first error line when not required', () => {
    prepareQmdParser({ ensure: fail, marker, required: false, warn: quiet });
    expect(fs.readFileSync(marker, 'utf8')).toBe('wasm-pack build failed for wasm-qmd-parser\n');
  });

  it('rethrows and leaves no marker when required', () => {
    fs.writeFileSync(marker, 'old\n');
    expect(() => prepareQmdParser({ ensure: fail, marker, required: true, warn: quiet })).toThrow(
      /QUARTO_QMD_PARSER_REQUIRED=1 but the qmd parser build failed: wasm-pack build failed/,
    );
    expect(fs.existsSync(marker)).toBe(false);
  });

  it('succeeds quietly when required and the build succeeds', () => {
    prepareQmdParser({ ensure: ok, marker, required: true, warn: quiet });
    expect(fs.existsSync(marker)).toBe(false);
  });
});
