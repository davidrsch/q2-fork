/**
 * useCursorToSlide Hook
 *
 * Maps cursor line numbers to slide indices for cursor-driven slide navigation.
 * The mapping is read off the transformed AST the preview rendered; see
 * `slideStartLines.ts` for why (and for the untitled-heading regression it
 * replaces).
 */

import { useMemo } from 'react';
import type { PandocAST } from '@quarto/preview-renderer/framework';
import { buildSlideMappings, slideIndexForLine, type SlideMapping } from './slideStartLines';

/**
 * Hook that provides a function to map cursor line numbers to slide indices.
 *
 * @param astJson - The transformed (q2-slides) Pandoc AST JSON string
 * @returns A function that takes a line number (0-based) and returns the corresponding slide index, or null if no valid mapping exists
 */
export function useCursorToSlide(
  astJson: string | null
): (line: number) => number | null {
  const slideMapping = useMemo((): SlideMapping[] => {
    if (!astJson) {
      return [];
    }

    try {
      const ast: PandocAST = JSON.parse(astJson);
      return buildSlideMappings(ast);
    } catch (err) {
      console.error('Failed to build cursor-to-slide mapping:', err);
      return [];
    }
  }, [astJson]);

  // Return a function that maps a line number to a slide index
  return useMemo(() => {
    // null means "no valid mapping" so callers leave the current slide alone
    return (line: number): number | null => slideIndexForLine(slideMapping, line);
  }, [slideMapping]);
}
