//! End-to-end CLI tests for coalescing repeated per-page diagnostics
//! (bd-mg3ckvp7).
//!
//! When a single underlying problem lives in a file shared by every
//! page — the motivating case is a broken navbar `href:` in
//! `_quarto.yml` — each per-document pipeline re-diagnoses it and the
//! render summary used to print one identical warning per page (186
//! copies on the connect-docs testbed). These tests pin the coalesced
//! behavior: one emission per distinct source span, with an
//! `Affected files:` tail listing the pages that re-reported it.
//!
//! Contract under verification (per
//! `claude-notes/plans/2026-07-31-repeated-diagnostics-coalescing.md`):
//! - A config-anchored warning (same `_quarto.yml` span reported by N
//!   pages) prints exactly once, with all N pages in the tail.
//! - A warning reported by a single page prints without any
//!   `Affected files:` tail (legacy shape preserved).
//! - Coalescing is print-only: exit codes are unchanged (warnings
//!   still exit 0 without `--strict`).
//!
//! TDD note: written before the implementation; the first test must
//! fail (3 copies, no tail) before Phase 2/3 of the plan start.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

const Q2_BIN: &str = env!("CARGO_BIN_EXE_q2");

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

/// Strip ANSI escape sequences (CSI color codes and OSC-8 hyperlinks)
/// so assertions can match the plain text of ariadne-rendered
/// snippets, which interleave color codes inside highlighted spans.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            // CSI: `ESC [ ... <alpha>`
            Some('[') => {
                for n in chars.by_ref() {
                    if n.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            // OSC: `ESC ] ... (BEL | ESC \)`
            Some(']') => {
                while let Some(n) = chars.next() {
                    if n == '\u{7}' {
                        break;
                    }
                    if n == '\u{1b}' {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Run `q2 render .` from `cwd` and return (exit-success, stderr with
/// ANSI escapes stripped).
fn run_q2_render(cwd: &Path) -> (bool, String) {
    let output = Command::new(Q2_BIN)
        .current_dir(cwd)
        .args(["render", "."])
        .output()
        .expect("spawn q2 binary");
    (
        output.status.success(),
        strip_ansi(&String::from_utf8_lossy(&output.stderr)),
    )
}

/// A website project whose navbar references a document that is not
/// in the render set. Every page's navbar-render transform reports
/// the miss at the same `_quarto.yml` span.
fn write_broken_navbar_project(dir: &Path, pages: &[&str]) {
    write_file(
        &dir.join("_quarto.yml"),
        "project:\n  type: website\n  output-dir: _site\nwebsite:\n  title: Coalesce\n  navbar:\n    left:\n      - href: missing.qmd\n        text: Missing\n",
    );
    for page in pages {
        write_file(
            &dir.join(format!("{page}.qmd")),
            &format!("---\ntitle: {page}\n---\n\nBody of {page}.\n"),
        );
    }
}

/// Three pages re-reporting the same `_quarto.yml` navbar miss must
/// produce exactly one Q-13-2 emission, tailed by all three pages.
#[test]
fn config_anchored_warning_coalesces_across_pages() {
    let dir = TempDir::new().unwrap();
    write_broken_navbar_project(dir.path(), &["index", "a", "b"]);

    let (success, stderr) = run_q2_render(dir.path());
    assert!(
        success,
        "warnings alone must not fail the render:\n{stderr}"
    );

    let q13_count = stderr.matches("Q-13-2").count();
    assert_eq!(
        q13_count, 1,
        "expected exactly one coalesced Q-13-2 emission, got {q13_count}:\n{stderr}"
    );

    let tail = stderr
        .lines()
        .find(|l| l.starts_with("Affected files:"))
        .unwrap_or_else(|| panic!("expected an `Affected files:` tail:\n{stderr}"));
    for page in ["index.qmd", "a.qmd", "b.qmd"] {
        assert!(tail.contains(page), "tail should name {page}; got: {tail}");
    }
}

/// More affected pages than the display cap: the tail lists the cap's
/// worth of names and summarizes the rest as `(and N other…)`.
#[test]
fn affected_files_tail_caps_long_lists() {
    let dir = TempDir::new().unwrap();
    write_broken_navbar_project(dir.path(), &["index", "a", "b", "c", "d"]);

    let (success, stderr) = run_q2_render(dir.path());
    assert!(
        success,
        "warnings alone must not fail the render:\n{stderr}"
    );

    assert_eq!(
        stderr.matches("Q-13-2").count(),
        1,
        "expected exactly one coalesced Q-13-2 emission:\n{stderr}"
    );
    let tail = stderr
        .lines()
        .find(|l| l.starts_with("Affected files:"))
        .unwrap_or_else(|| panic!("expected an `Affected files:` tail:\n{stderr}"));
    assert!(
        tail.contains("(and 2 others)"),
        "5 pages with a 3-name cap should summarize 2 more; got: {tail}"
    );
}

/// The coalesced config-anchored warning renders the `_quarto.yml`
/// source snippet (bd-mg3ckvp7 Phase 4 / plan D5). The warning's
/// location FileId is quarto_yaml's hash of the config path, which no
/// per-document `SourceContext` registers — so before Phase 4 the
/// block printed with no file, line, or snippet at all, leaving the
/// user with no pointer to the offending line.
#[test]
fn config_anchored_warning_shows_config_snippet() {
    let dir = TempDir::new().unwrap();
    write_broken_navbar_project(dir.path(), &["index", "a", "b"]);

    let (success, stderr) = run_q2_render(dir.path());
    assert!(
        success,
        "warnings alone must not fail the render:\n{stderr}"
    );

    assert!(
        stderr.contains("_quarto.yml"),
        "coalesced Q-13-2 should name _quarto.yml as its source:\n{stderr}"
    );
    // The YAML source line itself, which only appears if the ariadne
    // snippet rendered (the problem text says 'missing.qmd' but never
    // `href: missing.qmd`).
    assert!(
        stderr.contains("href: missing.qmd"),
        "coalesced Q-13-2 should render the offending YAML line:\n{stderr}"
    );
}

/// A per-page Q-2-9 diagnostic re-parsed out of a **multi-line block
/// scalar** (`page-footer.center: |`) renders with an ariadne snippet
/// naming `_quarto.yml` at the corrected position (task C5 / C4a).
///
/// Why this fixture and not `write_broken_navbar_project`'s plain
/// scalar: threading content provenance into the YAML re-parse bases
/// (commit `1b6d30c08`) fixed the *carets* (they used to land on the
/// wrong line) but changed the *shape* of these diagnostics' source
/// locations to `Substring { parent: Concat }` — a block scalar's
/// decoded content is discontiguous in the source once each line's
/// leading indent is stripped.
///
/// **Important limitation, recorded rather than hidden:** this
/// end-to-end test cannot by itself distinguish `bind_source_candidates`
/// using `resolve_byte_range()` (refuses `Concat`, registers nothing)
/// from using `root_file_id()` (resolves it, registers the file) —
/// both pass here, because `MetadataMergeStage`
/// (`stage/stages/metadata_merge.rs`) unconditionally pre-registers
/// `_quarto.yml`'s content into `doc.source_context` *before* the
/// block-scalar re-parse ever runs, independently of
/// `attach_config_source`/`bind_config_source`. That pre-registration
/// covers every candidate `attach_config_source` ever supplies
/// (`config_path`, profile overlays, extension manifests), so for any
/// diagnostic anchored in one of those files, `bind_source_candidates`
/// returning `None` is a harmless no-op in this call path today —
/// confirmed by running this exact fixture against the pre-fix
/// `resolve_byte_range()` binder (it still passed). The real
/// regression proof — a case where the file is *not* already
/// registered and `bind_source_candidates`'s own return value is the
/// only thing standing between the diagnostic and a snippet — is the
/// focused unit test `config_sources::tests::binds_a_concat_backed_source_info`
/// in `crates/quarto-core/src/config_sources.rs`, which fails red
/// (`None` where `Some(path)` is expected) before the fix and passes
/// after.
///
/// This test is kept anyway as the AGENTS.md-mandated end-to-end
/// check: it drives the real binary and inspects real stderr for the
/// corrected caret position *and* a rendered snippet together, and it
/// guards against a future regression in `MetadataMergeStage`'s
/// pre-registration (if that ever narrowed, this call path would
/// start depending on `bind_source_candidates` for real, and this
/// test would then catch a regression there too).
///
/// The assertion is snippet *presence*, not diagnostic presence.
/// Checking only that `Q-2-9` appears in stderr would pass whether or
/// not a snippet renders (the diagnostic itself is never suppressed)
/// and would be vacuous. Instead this asserts on the literal offending
/// source line (`<span id="y">Footer</span>`) appearing in stderr,
/// which can only come from ariadne's rendered excerpt of
/// `_quarto.yml`'s actual content — that text exists nowhere else in
/// the diagnostic's title, problem prose, or hints.
#[test]
fn block_scalar_config_diagnostic_binds_concat_backed_source() {
    let dir = TempDir::new().unwrap();
    write_file(
        &dir.path().join("_quarto.yml"),
        "project:\n  type: website\nwebsite:\n  title: \"T\"\n  page-footer:\n    center: |\n      line one\n      line two\n      <span id=\"y\">Footer</span>\n",
    );
    write_file(
        &dir.path().join("index.qmd"),
        "---\ntitle: \"Index\"\n---\n\nbody\n",
    );

    let (success, stderr) = run_q2_render(dir.path());
    assert!(
        success,
        "Q-2-9 is a warning; it must not fail the render:\n{stderr}"
    );

    let q2_9_count = stderr.matches("Q-2-9").count();
    assert_eq!(
        q2_9_count, 2,
        "expected two Q-2-9 warnings (open + close <span> tags); got {q2_9_count}:\n{stderr}"
    );

    assert!(
        stderr.contains("_quarto.yml"),
        "Q-2-9 should name _quarto.yml as its source:\n{stderr}"
    );
    // The non-vacuous signal: this exact line only appears in stderr
    // if ariadne actually read and rendered _quarto.yml's content —
    // it is not part of the diagnostic's title/problem/hint text.
    assert!(
        stderr.contains("<span id=\"y\">Footer</span>"),
        "Q-2-9 should render the offending _quarto.yml source line; \
         see this test's doc comment for why the real regression proof \
         lives in config_sources::tests::binds_a_concat_backed_source_info \
         instead:\n{stderr}"
    );
}

/// A single-page project still emits the warning once, with no
/// `Affected files:` tail — the legacy single-page shape.
#[test]
fn singleton_warning_keeps_legacy_shape() {
    let dir = TempDir::new().unwrap();
    write_broken_navbar_project(dir.path(), &["index"]);

    let (success, stderr) = run_q2_render(dir.path());
    assert!(
        success,
        "warnings alone must not fail the render:\n{stderr}"
    );

    assert_eq!(
        stderr.matches("Q-13-2").count(),
        1,
        "single page emits the warning exactly once:\n{stderr}"
    );
    assert!(
        !stderr.contains("Affected files:"),
        "singleton group must not grow a tail:\n{stderr}"
    );
}

/// A website whose `page-footer` carries one single-line scalar and
/// one **folded** scalar (a plain or quoted scalar continued on the
/// next line). Each HTML tag in either raises Q-2-9 once per page.
/// The folded scalar's content provenance is a `Concat` (quarto-yaml
/// ≥ 0.5.0), so the Q-2-9 locations re-parsed out of it are
/// `Substring { parent: Concat }`, the shape
/// posit-dev/quarto-error-reporting#10 is about.
fn write_folded_footer_project(dir: &Path, pages: &[&str]) {
    write_file(
        &dir.join("_quarto.yml"),
        "project:\n  type: website\nwebsite:\n  title: Probe\n  page-footer:\n    left: '<b>one line</b>'\n    right: '<i>first line</i>\n      <u>second line</u>'\n",
    );
    for page in pages {
        write_file(
            &dir.join(format!("{page}.qmd")),
            &format!("---\ntitle: {page}\n---\n\nbody\n"),
        );
    }
}

/// Count the `Affected files:` tails that list exactly `pages`, in
/// order, as basenames.
fn count_tails_listing(stderr: &str, pages: &[&str]) -> usize {
    stderr
        .lines()
        .filter(|l| l.starts_with("Affected files: "))
        .filter(|l| {
            let listed: Vec<String> = l["Affected files: ".len()..]
                .split(", ")
                .map(|p| {
                    Path::new(p.trim())
                        .file_name()
                        .map(|f| f.to_string_lossy().into_owned())
                        .unwrap_or_default()
                })
                .collect();
            listed == pages.iter().map(|p| format!("{p}.qmd")).collect::<Vec<_>>()
        })
        .count()
}

/// Q-2-9 warnings re-parsed out of a **folded** scalar coalesce across
/// pages exactly like those from a single-line scalar
/// (posit-dev/quarto-error-reporting#10).
///
/// Six tags (`<b>`, `</b>` on the single-line scalar; `<i>`, `</i>`,
/// `<u>`, `</u>` on the folded one) over three pages must print as six
/// groups, each tailed by all three pages. Before the fix the folded
/// scalar's four tags printed as twelve singletons (14 emissions in
/// all; reproduced with q2 0.36.0-nightly.20261024).
#[test]
fn folded_scalar_config_warnings_coalesce_across_pages() {
    let dir = TempDir::new().unwrap();
    write_folded_footer_project(dir.path(), &["a", "b", "c"]);

    let (success, stderr) = run_q2_render(dir.path());
    assert!(
        success,
        "Q-2-9 is a warning; it must not fail the render:\n{stderr}"
    );

    let q2_9_count = stderr.matches("Q-2-9").count();
    assert_eq!(
        q2_9_count, 6,
        "expected six coalesced Q-2-9 groups (two tags on the single-line \
         scalar, four on the folded one), not one per page per tag:\n{stderr}"
    );
    assert_eq!(
        count_tails_listing(&stderr, &["a", "b", "c"]),
        6,
        "every Q-2-9 group must list all three pages:\n{stderr}"
    );
    // The folded scalar's snippet still renders: this text exists only
    // in `_quarto.yml`'s content, never in the diagnostic prose.
    assert!(
        stderr.contains("<u>second line</u>"),
        "the folded scalar's second line should render in a snippet:\n{stderr}"
    );
}

/// The block-scalar fixture from
/// `block_scalar_config_diagnostic_binds_concat_backed_source`, over
/// three pages: its two Q-2-9 warnings (a `Substring { parent: Concat }`
/// each, the block scalar's decoded content being discontiguous in the
/// source) coalesce into two groups tailed by all three pages.
#[test]
fn block_scalar_config_warnings_coalesce_across_pages() {
    let dir = TempDir::new().unwrap();
    write_file(
        &dir.path().join("_quarto.yml"),
        "project:\n  type: website\nwebsite:\n  title: \"T\"\n  page-footer:\n    center: |\n      line one\n      line two\n      <span id=\"y\">Footer</span>\n",
    );
    for page in ["a", "b", "c"] {
        write_file(
            &dir.path().join(format!("{page}.qmd")),
            &format!("---\ntitle: {page}\n---\n\nbody\n"),
        );
    }

    let (success, stderr) = run_q2_render(dir.path());
    assert!(
        success,
        "Q-2-9 is a warning; it must not fail the render:\n{stderr}"
    );
    let q2_9_count = stderr.matches("Q-2-9").count();
    assert_eq!(
        q2_9_count, 2,
        "expected two coalesced Q-2-9 groups (open + close <span>):\n{stderr}"
    );
    assert_eq!(
        count_tails_listing(&stderr, &["a", "b", "c"]),
        2,
        "both Q-2-9 groups must list all three pages:\n{stderr}"
    );
}
