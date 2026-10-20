/**
 * useSlideThumbnails Hook
 *
 * Generates thumbnail images for presentation slides.
 * Each thumbnail is 128x64 pixels (maintaining 3:2 aspect ratio).
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import ReactDOM from 'react-dom/client';
import html2canvas from 'html2canvas';
import { renderSlide, type Slide } from '../components/render/ReactAstSlideRenderer';
import type { BlockNode, DivBlock, PandocAST } from '@quarto/preview-renderer/framework';
import { slideSourceLines } from './slideStartLines';

/**
 * Map from a slide's 0-based source start line to its thumbnail data URL.
 *
 * The line comes from the first located node inside the slide — for a titled
 * slide that is its heading, so the key equals the heading symbol's
 * `range.start.line`, which is what `OutlinePanel` looks thumbnails up by.
 * See `slideStartLines.ts`.
 */
export type ThumbnailMap = Map<number, string>;

interface UseSlideThumbnailsOptions {
  /** Pandoc AST JSON string. */
  astJson: string | null;
  /** Current file path for resolving relative image paths. */
  currentFilePath: string;
  /** Whether the preview is ready for thumbnail capture. */
  previewReady: boolean;
  /** Trigger value that changes when preview content updates. */
  contentVersion: number;
  /** Whether the current document is in slide format. When false, returns an empty map. */
  enabled: boolean;
}

/**
 * The renderable slide for one top-level block of the transformed AST: a
 * section Div contributes its contents, anything else is a one-block slide.
 */
function slideForBlock(block: BlockNode): Slide {
  const blocks = block.t === 'Div' ? (block as DivBlock).c[1] : [block];
  return { type: 'content', blocks };
}

/**
 * Generate thumbnails for presentation slides.
 *
 * Renders each slide off-screen and captures it as a thumbnail.
 * Thumbnails are captured at 128x64 pixels (3:2 aspect ratio).
 */
export function useSlideThumbnails({
  astJson,
  currentFilePath,
  previewReady,
  contentVersion,
  enabled,
}: UseSlideThumbnailsOptions): ThumbnailMap {
  const [thumbnails, setThumbnails] = useState<ThumbnailMap>(new Map());
  const captureInProgressRef = useRef(false);
  // A capture requested while one is running is deferred, not dropped:
  // the running capture may be rendering a deck the user has since edited.
  const rerunRequestedRef = useRef(false);
  const captureThumbnailsRef = useRef<() => Promise<void>>(async () => {});
  const debounceTimeoutRef = useRef<number | null>(null);

  // Clear thumbnails when disabled (e.g., switching away from slide format)
  useEffect(() => {
    if (!enabled) {
      setThumbnails(new Map());
    }
  }, [enabled]);

  const captureThumbnails = useCallback(async () => {
    // Skip thumbnail generation when not in slide format
    if (!enabled) return;
    // Avoid overlapping capture operations; rerun once the current one ends
    if (captureInProgressRef.current) {
      rerunRequestedRef.current = true;
      return;
    }
    if (!previewReady) return;
    if (!astJson) return;

    captureInProgressRef.current = true;

    try {
      const newThumbnails = new Map<number, string>();

      // Parse AST to get slides
      let ast: PandocAST;
      try {
        ast = JSON.parse(astJson);
      } catch (err) {
        console.error('Failed to parse AST for thumbnails:', err);
        return;
      }

      // One top-level block per horizontal slide (see slideStartLines.ts).
      const startLines = slideSourceLines(ast);

      for (let slideIndex = 0; slideIndex < ast.blocks.length; slideIndex++) {
        const startLine = startLines[slideIndex];

        // A slide with no source text (the generated title slide) has no
        // outline entry to attach a thumbnail to.
        if (startLine === null) {
          continue;
        }

        const slide = slideForBlock(ast.blocks[slideIndex]);

        // Create a temporary container for rendering this slide
        const container = document.createElement('div');
        container.style.position = 'absolute';
        container.style.left = '-10000px';
        container.style.top = '-10000px';
        container.style.width = '1050px';
        container.style.height = '700px';
        container.style.backgroundColor = 'white';
        container.style.boxShadow = '0 0 30px rgba(0,0,0,0.5)';
        document.body.appendChild(container);

        try {
          // Render the slide content using React
          const root = ReactDOM.createRoot(container);

          // Wait for rendering and capture
          await new Promise<void>((resolve) => {
            root.render(<div>{renderSlide(slide, currentFilePath)}</div>);

            // Give React time to render
            setTimeout(async () => {
              try {
                // Capture the container
                const canvas = await html2canvas(container, {
                  backgroundColor: '#ffffff',
                  useCORS: true,
                  logging: false,
                  scale: 0.3, // Lower scale for performance
                  width: 1050,
                  height: 700,
                });

                // Resize to thumbnail size (128x64 maintains 3:2 ratio)
                const thumbnailCanvas = document.createElement('canvas');
                thumbnailCanvas.width = 128;
                thumbnailCanvas.height = 64;
                const ctx = thumbnailCanvas.getContext('2d');
                if (ctx) {
                  ctx.drawImage(canvas, 0, 0, 128, 64);
                  const dataUrl = thumbnailCanvas.toDataURL('image/png');
                  newThumbnails.set(startLine, dataUrl);
                }
              } catch (error) {
                console.error(`Failed to capture thumbnail for slide ${slideIndex} (line ${startLine + 1}):`, error);
              } finally {
                root.unmount();
                resolve();
              }
            }, 50); // Short delay for React to render
          });
        } finally {
          // Clean up the temporary container
          document.body.removeChild(container);
        }
      }

      setThumbnails(newThumbnails);
    } finally {
      captureInProgressRef.current = false;
      if (rerunRequestedRef.current) {
        rerunRequestedRef.current = false;
        // Through the ref so the rerun sees the latest astJson, not the one
        // this closure captured.
        void captureThumbnailsRef.current();
      }
    }
  }, [astJson, currentFilePath, previewReady, enabled]);

  useEffect(() => {
    captureThumbnailsRef.current = captureThumbnails;
  }, [captureThumbnails]);

  // Debounced capture thumbnails when content changes
  useEffect(() => {
    if (debounceTimeoutRef.current) {
      clearTimeout(debounceTimeoutRef.current);
    }

    debounceTimeoutRef.current = window.setTimeout(() => {
      captureThumbnails();
    }, 100);

    return () => {
      if (debounceTimeoutRef.current) {
        clearTimeout(debounceTimeoutRef.current);
      }
    };
  }, [contentVersion, captureThumbnails]);

  return thumbnails;
}
