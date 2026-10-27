/*
 * tests/integration/lua_os_exit_cli.rs
 * Copyright (c) 2026 Posit, PBC
 *
 * `os.exit(1)` in a Lua filter fails the document, not the process
 * (GH #941). The scenario from the issue: a three-page website whose
 * filter exits on page A, where page C also has a parse error. Before the
 * fix the process died at A: nothing in `_site/`, no summary, and C's
 * error never reported.
 */

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

const Q2_BIN: &str = env!("CARGO_BIN_EXE_q2");

const DIE_LUA: &str = r#"
function Pandoc(doc)
  local title = pandoc.utils.stringify(doc.meta.title)
  if title == "A" then
    io.stderr:write("fatal: validation failed on A\n")
    os.exit(1)
  end
  return doc
end
"#;

fn write(root: &Path, rel: &str, content: &str) {
    std::fs::write(root.join(rel), content).unwrap();
}

#[test]
fn os_exit_fails_one_document_and_the_render_goes_on() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(root, "die.lua", DIE_LUA);
    write(
        root,
        "_quarto.yml",
        "project:\n  type: website\nfilters:\n  - die.lua\n",
    );
    write(root, "a.qmd", "---\ntitle: A\n---\n\npage a\n");
    write(root, "b.qmd", "---\ntitle: B\n---\n\npage b\n");
    write(root, "c.qmd", "---\ntitle: C\n---\n\n*unclosed emphasis\n");

    let out = Command::new(Q2_BIN)
        .current_dir(root)
        .args(["render"])
        .output()
        .expect("q2 runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let all = format!("{stdout}\n{stderr}");

    // The render as a whole still fails...
    assert!(!out.status.success(), "expected a non-zero exit:\n{all}");
    assert!(
        out.status.code().is_some(),
        "the process must exit, not die of a signal: {:?}",
        out.status
    );

    // ...but only page A is lost: B rendered, and C's own error was
    // reached and reported.
    let site = root.join("_site");
    assert!(site.join("b.html").is_file(), "b.html missing:\n{all}");
    assert!(
        !site.join("a.html").exists(),
        "a.html must not render:\n{all}"
    );
    assert!(
        all.contains("Q-2-12"),
        "c.qmd's parse error not reported:\n{all}"
    );

    // A's failure names the filter and the exit code, and the filter's
    // own stderr output came through first.
    assert!(all.contains("fatal: validation failed on A"), "{all}");
    assert!(
        all.contains("die.lua called os.exit(1); the document was not rendered"),
        "{all}"
    );
    assert!(all.contains("a.qmd"), "the failing page is named:\n{all}");

    // And the summary is printed.
    assert!(all.contains("Rendered 1 of 3 files"), "{all}");
}

#[test]
fn os_exit_zero_also_fails_the_document() {
    // Quarto 1 parity: pandoc exiting 0 without output was still a
    // failed render. The code is reported so the author sees what ran.
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(root, "stop.lua", "function Pandoc(doc) os.exit() end\n");
    write(
        root,
        "doc.qmd",
        "---\ntitle: D\nfilters: [stop.lua]\n---\n\nhello\n",
    );
    let out = Command::new(Q2_BIN)
        .current_dir(root)
        .args(["render", "doc.qmd"])
        .output()
        .expect("q2 runs");
    let all = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!out.status.success(), "{all}");
    assert!(!root.join("doc.html").exists(), "{all}");
    assert!(all.contains("stop.lua called os.exit(0)"), "{all}");
}
