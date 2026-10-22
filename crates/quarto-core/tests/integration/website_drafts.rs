/*
 * tests/integration/website_drafts.rs
 * Copyright (c) 2026 Posit, PBC
 *
 * End-to-end tests for website drafts and `draft-mode`
 * (bd-2hy80x1j).
 *
 * Plan: claude-notes/plans/2026-10-21-draft-mode-gone.md
 *
 * `draft-mode` is ordinary merged metadata: it may be written as
 * `draft-mode` or `website.draft-mode`, in `_quarto.yml`, a
 * directory's `_metadata.yml`, or a page's front matter, and each
 * draft page is governed by the value its own merged metadata
 * resolves to. These tests observe linkability through the
 * llms.txt markdown companions — a draft that is linkable (effective
 * mode `visible`) gets a companion; any other draft does not.
 */

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tempfile::TempDir;

use quarto_core::format::Format;
use quarto_core::project::ProjectContext;
use quarto_core::project::orchestrator::{ProjectPipeline, ProjectRenderSummary, project_type_for};
use quarto_core::render_to_file::RenderToFileOptions;
use quarto_system_runtime::{NativeRuntime, SystemRuntime};

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

fn render_project(fixture: impl FnOnce(&Path)) -> (PathBuf, ProjectRenderSummary) {
    let temp = TempDir::new().unwrap();
    let project_dir = canonical(temp.path());
    fixture(&project_dir);

    let runtime: Arc<dyn SystemRuntime> = Arc::new(NativeRuntime::new());
    let mut project = ProjectContext::discover(&project_dir, runtime.as_ref()).unwrap();

    let options = RenderToFileOptions::default();
    let project_type = project_type_for(&project);
    let mut pipeline = ProjectPipeline::new(
        &mut project,
        project_type,
        Format::html(),
        "html",
        &options,
        runtime.clone(),
    );
    let summary = pollster::block_on(pipeline.run()).expect("pipeline should succeed");
    assert!(
        summary.pass1_failures.is_empty() && summary.pass2_failures.is_empty(),
        "unexpected failures: pass1={:?} pass2={:?}",
        summary.pass1_failures,
        summary.pass2_failures
    );
    // Leak the temp dir so the test can inspect files afterwards
    // (cleanup happens at process exit).
    std::mem::forget(temp);
    (project_dir, summary)
}

/// `_quarto.yml` for a website with llms companions on, plus `extra`
/// (already indented YAML lines appended at top level).
fn quarto_yml(extra: &str) -> String {
    format!(
        "project:\n  type: website\n  output-dir: _site\n\
         website:\n  title: \"Drafts\"\n  llms-txt: true\n{extra}"
    )
}

fn draft_page(title: &str, extra_front_matter: &str) -> String {
    format!("---\ntitle: {title}\ndraft: true\n{extra_front_matter}---\n\nDraft body.\n")
}

fn has_companion(project_dir: &Path, md: &str) -> bool {
    project_dir.join("_site").join(md).exists()
}

fn diagnostic_codes(summary: &ProjectRenderSummary) -> Vec<String> {
    summary
        .outputs
        .iter()
        .flat_map(|o| o.render_output.diagnostics.iter())
        .chain(summary.project_diagnostics.iter())
        .filter_map(|d| d.code.clone())
        .collect()
}

// ═══════════════════════════════════════════════════════════════════
// Where `draft-mode` may be written
// ═══════════════════════════════════════════════════════════════════

/// Absent `draft-mode` means `gone`: the draft is not linkable.
#[test]
fn default_mode_hides_drafts() {
    let (dir, _) = render_project(|d| {
        write(&d.join("_quarto.yml"), &quarto_yml(""));
        write(&d.join("index.qmd"), "---\ntitle: Home\n---\n\nHi.\n");
        write(&d.join("secret.qmd"), &draft_page("Secret", ""));
    });
    assert!(has_companion(&dir, "index.md"));
    assert!(!has_companion(&dir, "secret.md"));
}

/// `website.draft-mode` in `_quarto.yml` — Quarto 1's spelling.
#[test]
fn website_spelling_in_quarto_yml() {
    let (dir, _) = render_project(|d| {
        write(
            &d.join("_quarto.yml"),
            &quarto_yml("  draft-mode: visible\n"),
        );
        write(&d.join("secret.qmd"), &draft_page("Secret", ""));
    });
    assert!(has_companion(&dir, "secret.md"));
}

/// Top-level `draft-mode` in `_quarto.yml` is the same as every page
/// declaring it.
#[test]
fn top_level_spelling_in_quarto_yml() {
    let (dir, _) = render_project(|d| {
        write(
            &d.join("_quarto.yml"),
            &format!("{}draft-mode: visible\n", quarto_yml("")),
        );
        write(&d.join("secret.qmd"), &draft_page("Secret", ""));
    });
    assert!(has_companion(&dir, "secret.md"));
}

/// A directory's `_metadata.yml` scopes `draft-mode` to that
/// directory; drafts elsewhere keep the project default.
#[test]
fn metadata_yml_scopes_draft_mode_to_its_directory() {
    let (dir, _) = render_project(|d| {
        write(&d.join("_quarto.yml"), &quarto_yml(""));
        write(&d.join("posts/_metadata.yml"), "draft-mode: visible\n");
        write(&d.join("posts/wip.qmd"), &draft_page("WIP", ""));
        write(&d.join("notes/wip.qmd"), &draft_page("Other WIP", ""));
    });
    assert!(
        has_companion(&dir, "posts/wip.md"),
        "posts/ draft is visible"
    );
    assert!(
        !has_companion(&dir, "notes/wip.md"),
        "other drafts stay gone"
    );
}

/// Front matter overrides the project value for one page, in either
/// direction.
#[test]
fn front_matter_overrides_project_value() {
    let (dir, _) = render_project(|d| {
        write(
            &d.join("_quarto.yml"),
            &quarto_yml("  draft-mode: visible\n"),
        );
        write(
            &d.join("hidden.qmd"),
            &draft_page("Hidden", "website:\n  draft-mode: unlinked\n"),
        );
        write(&d.join("shown.qmd"), &draft_page("Shown", ""));
    });
    assert!(!has_companion(&dir, "hidden.md"));
    assert!(has_companion(&dir, "shown.md"));
}

// ═══════════════════════════════════════════════════════════════════
// Invalid values
// ═══════════════════════════════════════════════════════════════════

/// An unknown value warns (Q-5-49) on each draft page and falls back
/// to `gone`; non-draft pages do not warn.
#[test]
fn invalid_value_warns_on_draft_pages_and_falls_back_to_gone() {
    let (dir, summary) = render_project(|d| {
        write(
            &d.join("_quarto.yml"),
            &quarto_yml("  draft-mode: hidden\n"),
        );
        write(&d.join("index.qmd"), "---\ntitle: Home\n---\n\nHi.\n");
        write(&d.join("secret.qmd"), &draft_page("Secret", ""));
    });
    assert!(!has_companion(&dir, "secret.md"));
    let codes = diagnostic_codes(&summary);
    assert_eq!(
        codes.iter().filter(|c| *c == "Q-5-49").count(),
        1,
        "one Q-5-49, from the one draft page; got {codes:?}"
    );
}
