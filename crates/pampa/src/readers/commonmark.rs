/*
 * commonmark.rs
 * Copyright (c) 2025 Posit, PBC
 *
 * CommonMark reader using comrak for parsing.
 *
 * This provides an alternative to the QMD reader for pure CommonMark content,
 * leveraging the comrak parser and the comrak-to-pandoc conversion library.
 */

use crate::pandoc::ast_context::ASTContext;
use comrak::{Arena, Options, parse_document};
use comrak_to_pandoc::{SourceLocationContext, convert_document_with_source};
use quarto_pandoc_types::Pandoc;
use quarto_source_map::{FileId, SourceContext};

/// Read CommonMark input and convert to Pandoc AST with source tracking.
///
/// # Arguments
/// * `input` - The CommonMark source text
/// * `filename` - The filename for error reporting and source tracking
///
/// # Returns
/// A tuple of (Pandoc document, ASTContext with source info)
pub fn read(input: &str, filename: &str) -> (Pandoc, ASTContext) {
    // Register the input exactly once, as the qmd reader does: replace the
    // context `with_filename` built (whose file 0 has no content) and stamp
    // nodes with the FileId `add_file` actually returns (GH #883). Reuse the
    // already-normalized filename from `context.filenames[0]` rather than the
    // raw `filename` again, so this doesn't reintroduce backslashes on Windows.
    let mut context = ASTContext::with_filename(filename.to_string());
    let normalized_filename = context.filenames[0].clone();
    context.source_context = SourceContext::new();
    let file_id = context
        .source_context
        .add_file(normalized_filename, Some(input.to_string()));

    (convert(input, file_id), context)
}

/// Convert CommonMark input to Pandoc AST, stamping every span with
/// `file_id`.
///
/// For callers that register `input` in a `SourceContext` of their own
/// (under `file_id`); [`read`] is this plus a fresh single-file context.
pub fn convert(input: &str, file_id: FileId) -> Pandoc {
    let arena = Arena::new();

    // Use pure CommonMark options (no GFM extensions)
    let options = Options::default();

    // Parse with comrak
    let root = parse_document(&arena, input, &options);

    // Create source location context for conversion
    let source_ctx = SourceLocationContext::new(input, file_id);

    // Convert to Pandoc AST with source tracking
    convert_document_with_source(root, Some(&source_ctx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use quarto_pandoc_types::{Block, Inline};

    #[test]
    fn test_simple_paragraph() {
        let (pandoc, _ctx) = read("Hello world.\n", "test.md");
        assert_eq!(pandoc.blocks.len(), 1);
        match &pandoc.blocks[0] {
            Block::Paragraph(p) => {
                // Should be: Str("Hello"), Space, Str("world.")
                assert_eq!(p.content.len(), 3);
                match &p.content[0] {
                    Inline::Str(s) => assert_eq!(s.text, "Hello"),
                    _ => panic!("Expected Str"),
                }
            }
            _ => panic!("Expected Paragraph"),
        }
    }

    #[test]
    fn test_heading() {
        let (pandoc, _ctx) = read("# Hello\n", "test.md");
        assert_eq!(pandoc.blocks.len(), 1);
        match &pandoc.blocks[0] {
            Block::Header(h) => {
                assert_eq!(h.level, 1);
            }
            _ => panic!("Expected Header"),
        }
    }

    #[test]
    fn test_source_tracking() {
        let input = "Hello world.\n";
        let (pandoc, _ctx) = read(input, "test.md");

        // Check that source info is present
        match &pandoc.blocks[0] {
            Block::Paragraph(p) => {
                // Paragraph should have source info covering the whole line
                assert!(p.source_info.start_offset() < p.source_info.end_offset());

                // First Str should have precise source info
                match &p.content[0] {
                    Inline::Str(s) => {
                        assert_eq!(s.source_info.start_offset(), 0);
                        assert_eq!(s.source_info.end_offset(), 5); // "Hello"
                    }
                    _ => panic!("Expected Str"),
                }
            }
            _ => panic!("Expected Paragraph"),
        }
    }

    /// GH #883: the input is registered once, and the nodes point at the
    /// entry that holds it, so an in-memory input (no file on disk to fall
    /// back on) still maps.
    #[test]
    fn in_memory_input_is_registered_once_and_maps() {
        let input = "hello *world*\n";
        let (pandoc, ctx) = read(input, "<mem>");

        let file = ctx
            .source_context
            .get_file(FileId(0))
            .expect("file 0 should exist");
        assert_eq!(file.content.as_deref(), Some(input));
        assert!(
            ctx.source_context.get_file(FileId(1)).is_none(),
            "the input must be registered exactly once"
        );

        let Block::Paragraph(p) = &pandoc.blocks[0] else {
            panic!("Expected Paragraph");
        };
        let emph = p
            .content
            .iter()
            .find(|i| matches!(i, Inline::Emph(_)))
            .expect("Expected Emph");
        assert_eq!(emph.source_info().root_file_id(), Some(FileId(0)));
        let mapped = emph
            .source_info()
            .map_offset(0, &ctx.source_context)
            .expect("Emph should map into the registered input");
        assert_eq!(mapped.file_id, FileId(0));
        assert_eq!(mapped.location.offset, 6);
    }

    /// `convert` stamps every span with the caller's FileId, for callers
    /// that register the input in their own context.
    #[test]
    fn convert_stamps_the_given_file_id() {
        let pandoc = convert("hello *world*\n", FileId(42));
        let Block::Paragraph(p) = &pandoc.blocks[0] else {
            panic!("Expected Paragraph");
        };
        assert_eq!(p.source_info.root_file_id(), Some(FileId(42)));
        for inline in &p.content {
            assert_eq!(inline.source_info().root_file_id(), Some(FileId(42)));
        }
    }

    #[test]
    fn test_emphasis_source() {
        let input = "*hello*\n";
        let (pandoc, _ctx) = read(input, "test.md");

        match &pandoc.blocks[0] {
            Block::Paragraph(p) => {
                match &p.content[0] {
                    Inline::Emph(e) => {
                        // Emph should cover "*hello*"
                        assert_eq!(e.source_info.start_offset(), 0);
                        assert_eq!(e.source_info.end_offset(), 7);
                    }
                    _ => panic!("Expected Emph"),
                }
            }
            _ => panic!("Expected Paragraph"),
        }
    }
}
