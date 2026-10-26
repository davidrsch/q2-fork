//! Lint rule: rendered output is not committed (bd-9xbo5eee).
//!
//! Output that `q2` renders goes stale with every release, and nothing
//! keeps a committed copy current. `examples/lmd/10-project/index.html`
//! was committed by accident alongside its sources (the examples'
//! `.gitignore` only listed `document.html` and `slides.html`) and sat
//! there two minor versions out of date, rewritten by every
//! `cargo xtask stage-doc-examples` run.
//!
//! Rendered output belongs in one of two places:
//!
//! - **generated and gitignored**, when it should be current — the
//!   `stage-doc-examples` / `build-agents-docs` pattern;
//! - **frozen, under `claude-notes/plans/<name>-investigation/`**, when
//!   it is evidence of what a bug looked like at a given commit. Those
//!   files are allowed and must never be re-rendered.
//!
//! A tracked `.html` file is flagged when either
//!
//! 1. it carries q2's generator stamp
//!    (`<meta name="generator" content="quarto-rust-…">`), or
//! 2. it sits beside a tracked source with the same stem (`slides.html`
//!    next to `slides.qmd`) — in-place render output. This catches
//!    formats whose templates carry no stamp (revealjs) and output from
//!    other Quarto versions.

use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use super::Violation;

const RULE_NAME: &str = "committed-render-output";

/// q2's HTML generator stamp (`crates/quarto-core/src/template.rs`).
const Q2_GENERATOR: &str = "<meta name=\"generator\" content=\"quarto-rust-";

/// Source extensions whose same-stem `.html` sibling is render output.
const SOURCE_EXTENSIONS: &[&str] = &["qmd", "md", "ipynb", "Rmd"];

const SUGGESTION: &str = "untrack it with `git rm --cached <file>` and gitignore the output; \
     if it should stay current, generate it (like `cargo xtask stage-doc-examples`); if it is \
     a deliberately frozen repro, keep it under `claude-notes/plans/<name>-investigation/`";

/// Run the check over the files git tracks under `workspace_root`.
pub fn check(workspace_root: &Path) -> Result<Vec<Violation>> {
    let output = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(workspace_root)
        .output()
        .context("running `git ls-files`")?;
    if !output.status.success() {
        bail!(
            "`git ls-files` failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let tracked: Vec<String> = output
        .stdout
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| String::from_utf8_lossy(p).into_owned())
        .collect();
    check_files(workspace_root, &tracked)
}

/// Run the check over `tracked` (`/`-separated, root-relative paths).
fn check_files(workspace_root: &Path, tracked: &[String]) -> Result<Vec<Violation>> {
    let tracked_set: HashSet<&str> = tracked.iter().map(String::as_str).collect();
    let mut violations = Vec::new();
    for rel in tracked {
        let Some(stem) = rel.strip_suffix(".html") else {
            continue;
        };
        if is_allowed(rel) {
            continue;
        }
        let path = workspace_root.join(rel);
        // A tracked file can be missing from the working tree (deleted,
        // not yet committed); there is nothing to check then.
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        if let Some(line) = text.lines().position(|l| l.contains(Q2_GENERATOR)) {
            violations.push(Violation {
                file: path,
                line: line + 1,
                column: 1,
                rule: RULE_NAME,
                message: "committed file is q2 render output (it carries q2's generator stamp)"
                    .to_string(),
                suggestion: Some(SUGGESTION.to_string()),
            });
            continue;
        }
        if let Some(source) = SOURCE_EXTENSIONS
            .iter()
            .map(|ext| format!("{stem}.{ext}"))
            .find(|source| tracked_set.contains(source.as_str()))
        {
            violations.push(Violation {
                file: path,
                line: 1,
                column: 1,
                rule: RULE_NAME,
                message: format!("committed file is render output of its sibling `{source}`"),
                suggestion: Some(SUGGESTION.to_string()),
            });
        }
    }
    Ok(violations)
}

/// Frozen repro artifacts live in investigation directories.
fn is_allowed(rel: &str) -> bool {
    let Some(rest) = rel.strip_prefix("claude-notes/plans/") else {
        return false;
    };
    rest.split('/')
        .rev()
        .skip(1) // the file name itself
        .any(|dir| dir.ends_with("-investigation"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(files: &[(&str, &str)]) -> (tempfile::TempDir, Vec<String>) {
        let tmp = tempfile::TempDir::new().unwrap();
        for (rel, contents) in files {
            let path = tmp.path().join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
        let tracked = files.iter().map(|(rel, _)| rel.to_string()).collect();
        (tmp, tracked)
    }

    const STAMPED: &str = "<!DOCTYPE html>\n<html>\n<head>\n\
                           <meta name=\"generator\" content=\"quarto-rust-0.33.0\">\n";

    fn flagged(files: &[(&str, &str)]) -> Vec<(String, usize, String)> {
        let (tmp, tracked) = tree(files);
        check_files(tmp.path(), &tracked)
            .unwrap()
            .into_iter()
            .map(|v| {
                let rel = v
                    .file
                    .strip_prefix(tmp.path())
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                (rel, v.line, v.message)
            })
            .collect()
    }

    #[test]
    fn flags_q2_stamped_html_with_its_line() {
        let got = flagged(&[("site/_site/page.html", STAMPED)]);
        assert_eq!(got.len(), 1, "{got:?}");
        assert_eq!(got[0].0, "site/_site/page.html");
        assert_eq!(got[0].1, 4);
        assert!(got[0].2.contains("generator stamp"), "{}", got[0].2);
    }

    #[test]
    fn flags_unstamped_html_beside_its_source() {
        // revealjs output carries no generator stamp.
        let got = flagged(&[
            ("examples/p/slides.qmd", "---\nformat: revealjs\n---\n"),
            ("examples/p/slides.html", "<div class=\"reveal\"></div>"),
            ("notes/plan.md", "# Plan\n"),
            (
                "notes/plan.html",
                "<meta name=\"generator\" content=\"quarto-99.9.9\">",
            ),
        ]);
        let files: Vec<&str> = got.iter().map(|(f, _, _)| f.as_str()).collect();
        assert_eq!(files, ["examples/p/slides.html", "notes/plan.html"]);
        assert!(got[0].2.contains("`examples/p/slides.qmd`"), "{}", got[0].2);
    }

    #[test]
    fn allows_investigation_artifacts() {
        let got = flagged(&[
            ("claude-notes/plans/foo-investigation/repro.qmd", "x"),
            ("claude-notes/plans/foo-investigation/repro.html", STAMPED),
            (
                "claude-notes/plans/bar-investigation/out/observed.html",
                STAMPED,
            ),
        ]);
        assert!(got.is_empty(), "{got:?}");
    }

    #[test]
    fn investigation_allowance_is_scoped_to_claude_notes_plans() {
        let got = flagged(&[("examples/x-investigation/page.html", STAMPED)]);
        assert_eq!(got.len(), 1, "{got:?}");
    }

    #[test]
    fn ignores_hand_written_and_foreign_html() {
        let got = flagged(&[
            // Quarto 1 reference fixture: other generator, no sibling source.
            (
                "examples/site/q1-site/index.html",
                "<meta name=\"generator\" content=\"quarto-99.9.9\">",
            ),
            ("examples/site/index.qmd", "x"),
            ("resources/partials/header.html", "<header></header>"),
            ("notes/page.md", "x"),
        ]);
        assert!(got.is_empty(), "{got:?}");
    }

    #[test]
    fn tracked_but_deleted_files_are_skipped() {
        let (tmp, mut tracked) = tree(&[("a.qmd", "x")]);
        tracked.push("a.html".to_string());
        assert!(check_files(tmp.path(), &tracked).unwrap().is_empty());
    }
}
