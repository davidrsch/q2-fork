/*
 * tests/integration/pandoc_read_locations_e2e.rs
 * Copyright (c) 2026 Posit, PBC
 *
 * End-to-end regression tests for GH #884: diagnostics about nodes that
 * `pandoc.read` returned must not be located in the host document at the
 * string's own offsets.
 */

//! `pandoc.read` used to parse into a private context whose file 0 was the
//! string, so every node came back as `Original { FileId(0), offset }`. In
//! the host document FileId(0) is the document, and a later pass reporting
//! on such a node (here Q-2-49, a bracketed span without attributes)
//! underlined whatever sat at that byte of the document: the front matter.
//!
//! Each `pandoc.read` call now gets its own virtual file, labelled with the
//! call site, which the lmd stage and the user-filter stage register in the
//! document's source contexts.
//!
//! TDD note: written before the fix. On the unfixed tree both renders put
//! Q-2-49 at line 2, column 12 (byte 15 of the document, the `[` of the
//! string).

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

use super::coalesced_diagnostics::strip_ansi;

const Q2_BIN: &str = env!("CARGO_BIN_EXE_q2");

/// GH #884's repro: the bracketed text sits on line 14, inside the Lua
/// string; byte 15 of the document (the string offset of `[`) is on line 2.
const LMD_REPRO: &str = r#"---
title: diagnostic location from pandoc.read
---

## Section one

Some paragraph text that occupies line 7.

More text on line 9.

## Section two, cell starts on line 13

```{lmd}
local doc = pandoc.read('Allowed kinds: ["skills", "commands"] only.', 'markdown')
return doc.blocks
```

Trailing paragraph on line 18.
"#;

/// The same string read by a Lua filter (call on line 2 of `f.lua`) and
/// appended to a document whose byte 15 is again in the title line.
const FILTER_DOC: &str = "---\n\
title: filter variant\n\
filters: [f.lua]\n\
---\n\
\n\
Body line 6.\n";

const FILTER_LUA: &str = "function Pandoc(doc)\n  \
local extra = pandoc.read('Allowed kinds: [\"skills\", \"commands\"] only.', 'markdown')\n  \
doc.blocks:extend(extra.blocks)\n  \
return doc\n\
end\n";

fn run_q2_render(cwd: &Path, args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(Q2_BIN);
    cmd.current_dir(cwd);
    cmd.arg("render");
    cmd.args(args);
    cmd.output().expect("spawn q2 binary")
}

/// The single Q-2-49 record from a `--json-errors` render.
fn q_2_49_record(stdout_and_stderr: &str) -> serde_json::Value {
    let records: Vec<serde_json::Value> = stdout_and_stderr
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok())
        .flat_map(|v| match v {
            serde_json::Value::Array(items) => items,
            other => vec![other],
        })
        .filter(|v| v.get("code").and_then(|c| c.as_str()) == Some("Q-2-49"))
        .collect();
    assert_eq!(
        records.len(),
        1,
        "expected exactly one Q-2-49 record:\n{stdout_and_stderr}"
    );
    records.into_iter().next().unwrap()
}

/// stdout and stderr, without the color codes and hyperlinks the snippet
/// renderer interleaves with the text.
fn combined(output: &std::process::Output) -> String {
    strip_ansi(&format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

/// The warning is about text in the string, so it is located in the
/// string's virtual file: no `source_file` (the label is not a path on
/// disk), and certainly not the host document's line 2.
fn assert_not_in_host_document(record: &serde_json::Value, host: &str) {
    if let Some(file) = record.get("source_file").and_then(|f| f.as_str()) {
        assert!(
            !file.ends_with(host),
            "Q-2-49 must not be located in {host}: {record}"
        );
    }
}

/// Text output names the call site and shows the string's own text.
fn assert_text_frames_the_string(text: &str, call_site_label: &str) {
    assert!(
        text.contains(call_site_label),
        "expected the call-site label {call_site_label:?}:\n{text}"
    );
    assert!(
        text.contains(r#"Allowed kinds: ["skills", "commands"] only."#),
        "expected the string's own text as the snippet:\n{text}"
    );
    assert!(
        !text.contains("title:"),
        "the snippet must not be the front matter:\n{text}"
    );
}

#[test]
fn lmd_cell_pandoc_read_diagnostic_is_located_in_the_string() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    std::fs::write(dir.join("loc.qmd"), LMD_REPRO).unwrap();

    let json = run_q2_render(dir, &["loc.qmd", "--to", "html", "--json-errors"]);
    assert!(
        json.status.success(),
        "render must succeed:\n{}",
        combined(&json)
    );
    assert_not_in_host_document(&q_2_49_record(&combined(&json)), "loc.qmd");

    let text = run_q2_render(dir, &["loc.qmd", "--to", "html"]);
    assert_text_frames_the_string(&combined(&text), "loc.qmd[pandoc.read, line 14]");
}

#[test]
fn lua_filter_pandoc_read_diagnostic_is_located_in_the_string() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    std::fs::write(dir.join("doc.qmd"), FILTER_DOC).unwrap();
    std::fs::write(dir.join("f.lua"), FILTER_LUA).unwrap();

    let json = run_q2_render(dir, &["doc.qmd", "--to", "html", "--json-errors"]);
    assert!(
        json.status.success(),
        "render must succeed:\n{}",
        combined(&json)
    );
    assert_not_in_host_document(&q_2_49_record(&combined(&json)), "doc.qmd");

    let text = run_q2_render(dir, &["doc.qmd", "--to", "html"]);
    assert_text_frames_the_string(&combined(&text), "f.lua[pandoc.read, line 2]");
}
