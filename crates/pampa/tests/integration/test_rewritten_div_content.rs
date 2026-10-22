/*
 * test_rewritten_div_content.rs
 * Copyright (c) 2026 Posit, PBC
 *
 * The reader rewrites a `.definition-list` / `.list-table` div into a
 * `DefinitionList` / `Table` before it visits the div's content. That
 * content must still get the whole postprocess pass: the structural
 * desugaring set (nested divs, implicit figures) and the reader's
 * concrete-syntax clean-up (note references, inline attributes, header
 * ids, warnings). GH #895, bd-definition-list-rewrite-content-baxb5j8e,
 * bd-9fy9p4zl.
 *
 * Run with: cargo nextest run -p pampa test_rewritten_div_content
 */

use pampa::pandoc::{ASTContext, Block, Pandoc};
use pampa::readers;
use pampa::writers;
use quarto_error_reporting::DiagnosticMessage;

fn read(input: &str) -> (Pandoc, Vec<DiagnosticMessage>) {
    let (pandoc, _context, warnings) = readers::qmd::read(
        input.as_bytes(),
        false,
        "test.qmd",
        &mut std::io::sink(),
        true,
        None,
    )
    .expect("input should parse");
    (pandoc, warnings)
}

/// The native writer refuses AST the reader should have lowered
/// (`NoteReference`, standalone `Attr`, editorial-mark nodes), so a
/// successful write means the postprocess pass reached every node.
/// (It also refuses `[^n]: …` note definitions, so the inputs below
/// reference notes without defining them.)
fn native(pandoc: &Pandoc) -> String {
    let mut buf = Vec::new();
    writers::native::write(pandoc, &ASTContext::anonymous(), &mut buf)
        .unwrap_or_else(|errors| panic!("native writer rejected the AST: {errors:?}"));
    String::from_utf8(buf).unwrap()
}

fn codes(warnings: &[DiagnosticMessage]) -> Vec<&str> {
    warnings.iter().filter_map(|w| w.code.as_deref()).collect()
}

/// The single definition's blocks of a one-entry `DefinitionList`.
fn only_definition(block: &Block) -> &Vec<Block> {
    let Block::DefinitionList(list) = block else {
        panic!("expected DefinitionList, got {block:?}");
    };
    let [(_, definitions)] = list.content.as_slice() else {
        panic!("{:?}", list.content);
    };
    let [definition] = definitions.as_slice() else {
        panic!("{definitions:?}");
    };
    definition
}

#[test]
fn nested_divs_in_a_definition_list_are_rewritten() {
    let (pandoc, _) = read(
        "::: {.definition-list}
* Outer term
  - ::: {.definition-list}
    * Inner term
      - Inner definition.
    :::
* Table term
  - ::: {.list-table}
    * - a
      - b
    * - 1
      - 2
    :::
:::
",
    );
    let [Block::DefinitionList(list)] = pandoc.blocks.as_slice() else {
        panic!("{:?}", pandoc.blocks);
    };
    let [(_, first), (_, second)] = list.content.as_slice() else {
        panic!("{:?}", list.content);
    };
    assert!(
        matches!(first[0].as_slice(), [Block::DefinitionList(_)]),
        "{first:?}"
    );
    assert!(
        matches!(second[0].as_slice(), [Block::Table(_)]),
        "{second:?}"
    );
}

#[test]
fn a_definition_list_in_a_list_table_cell_is_rewritten() {
    let (pandoc, _) = read(
        "::: {.list-table}
* - ::: {.definition-list}
    * Cell term
      - Cell definition.
    :::
  - Plain cell
:::
",
    );
    let [Block::Table(table)] = pandoc.blocks.as_slice() else {
        panic!("{:?}", pandoc.blocks);
    };
    let cell = &table.head.rows[0].cells[0];
    assert!(
        matches!(cell.content.as_slice(), [Block::DefinitionList(_)]),
        "{:?}",
        cell.content
    );
}

#[test]
fn an_image_paragraph_in_a_loose_definition_is_a_figure() {
    let (pandoc, _) = read(
        "::: {.definition-list}
* Figure term

  - Some text.

    ![An alt text](img.png)

:::
",
    );
    let [outer] = pandoc.blocks.as_slice() else {
        panic!("{:?}", pandoc.blocks);
    };
    assert!(
        matches!(
            only_definition(outer).as_slice(),
            [Block::Paragraph(_), Block::Figure(_)]
        ),
        "{outer:?}"
    );
}

#[test]
fn a_doubled_brace_fence_in_a_definition_warns() {
    let (_, warnings) = read(
        "::: {.definition-list}
* Term
  - ```{{python}}
    1 + 1
    ```
:::
",
    );
    assert!(codes(&warnings).contains(&"Q-2-50"), "{warnings:?}");
}

#[test]
fn reader_lowerings_reach_a_definition_list() {
    let (pandoc, _) = read(
        "::: {.definition-list}
* Term with a note[^n] and [++ an insert]
  - Definition with $x$ {#eq-a} and a note[^n].
:::
",
    );
    // Panics if a NoteReference, standalone Attr or editorial mark survived.
    native(&pandoc);
}

#[test]
fn reader_lowerings_reach_a_list_table_cell() {
    let (pandoc, _) = read(
        "::: {.list-table}
* - see[^n] and $x$ {#eq-a}
  - [++ an insert]
:::
",
    );
    native(&pandoc);
}

#[test]
fn a_header_in_a_definition_gets_an_auto_id() {
    let (pandoc, _) = read(
        "::: {.definition-list}
* Term
  - ## A Header
:::
",
    );
    let [outer] = pandoc.blocks.as_slice() else {
        panic!("{:?}", pandoc.blocks);
    };
    let [Block::Header(header)] = only_definition(outer).as_slice() else {
        panic!("{outer:?}");
    };
    assert_eq!(header.attr.0, "a-header");
}

#[test]
fn an_orphan_caption_in_a_list_table_cell_falls_back_to_a_paragraph() {
    // Used to panic: the cell's CaptionBlock never reached the handler
    // that attaches or flattens captions.
    let (pandoc, warnings) = read(
        "::: {.list-table}
* - a
* - : b
:::
",
    );
    let [Block::Table(table)] = pandoc.blocks.as_slice() else {
        panic!("{:?}", pandoc.blocks);
    };
    let cell = &table.bodies[0].body[0].cells[0];
    assert!(
        matches!(cell.content.as_slice(), [Block::Paragraph(_)]),
        "{:?}",
        cell.content
    );
    assert!(!warnings.is_empty());
}

#[test]
fn a_list_table_is_not_subject_to_the_pipe_table_caption_heuristic() {
    // `.with_table` turns a trailing one-cell row starting with ':' into
    // the caption of a *pipe* table. Processing a list-table's content
    // must not re-apply it to the list-table itself.
    let (pandoc, _) = read(
        "::: {.list-table}
* - a
* - \\: b
:::
",
    );
    let [Block::Table(table)] = pandoc.blocks.as_slice() else {
        panic!("{:?}", pandoc.blocks);
    };
    assert!(
        table
            .caption
            .long
            .as_ref()
            .is_none_or(|blocks| blocks.is_empty()),
        "{:?}",
        table.caption
    );
    assert_eq!(table.bodies[0].body.len(), 1, "the last row was taken");
}
