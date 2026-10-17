/**
 * Vitest global setup: build the wasm-qmd-parser nodejs pkg when missing or
 * stale. Best-effort by default: environments without the wasm toolchain
 * (wasm-pack + a wasm32-capable clang) get a marker file instead, and the
 * parser-tier tests skip themselves (see src/test-setup.ts) — the same
 * gated-tier pattern as the network-gated live tests. Failing the whole
 * suite on a toolchain-less CI leg would hide every non-parser test result.
 *
 * QUARTO_QMD_PARSER_REQUIRED=1 makes a failed build fatal instead. CI's
 * parser-tier step sets it, because that step has the toolchain: there, a
 * failure means the parser does not compile, not that the toolchain is
 * absent. Without it, the v0.34.0 release break reported as 21 skipped tests
 * on a green step (bd-yt3x1gwk).
 */

import { writeFileSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { ensureQmdParserPkg } from './qmd-parser-pkg.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const pkgRoot = join(here, '..');
const marker = join(pkgRoot, '.qmd-parser-unavailable');

/**
 * Build the parser via `ensure`, then either clear the marker (success),
 * rethrow (failure while required), or write the marker (failure otherwise).
 * A stale marker is removed in every case but the last, so a required run
 * can never inherit a skip from an earlier best-effort run.
 * @param {{ensure: () => unknown, marker: string, required: boolean, warn?: (msg: string) => void}} opts
 */
export function prepareQmdParser({ ensure, marker, required, warn = console.warn }) {
  try {
    ensure();
    rmSync(marker, { force: true });
  } catch (err) {
    if (required) {
      rmSync(marker, { force: true });
      const reason = err instanceof Error ? err.message : String(err);
      throw new Error(`QUARTO_QMD_PARSER_REQUIRED=1 but the qmd parser build failed: ${reason}`, {
        cause: err,
      });
    }
    const reason = err instanceof Error ? err.message.split('\n')[0] : String(err);
    writeFileSync(marker, `${reason}\n`);
    warn(
      `[qmd-parser] unavailable (${reason}) — parser-tier tests will skip. ` +
        'Install wasm-pack and a wasm32-capable clang to run them.',
    );
  }
}

export default function setup() {
  prepareQmdParser({
    ensure: () => ensureQmdParserPkg({ quiet: true }),
    marker,
    required: process.env.QUARTO_QMD_PARSER_REQUIRED === '1',
  });
}
