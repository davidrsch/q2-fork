/*
 * announcement_generate.rs
 * Copyright (c) 2026 Posit, PBC
 */

//! Announcement-banner resolution transform
//! (bd-website-announcement-w1gtpzki).
//!
//! Reads `website.announcement` (or top-level `announcement:`) from the
//! merged page metadata, hands it to
//! [`quarto_navigation::resolve_announcement`], and stores the result at
//! `navigation.announcement`. Format-agnostic: the HTML banner is emitted
//! by [`AnnouncementRenderTransform`](super::AnnouncementRenderTransform);
//! another format can render the same structure its own way.
//!
//! Reading the *merged page* metadata (not the project config, as Quarto 1
//! does) is what makes the banner opt-out-able per page: front matter or a
//! `_metadata.yml` setting `website: {announcement: false}` wins the merge.
//!
//! **The banner lives in metadata only — never inject it into
//! `ast.blocks`.** The llms.txt capture (`LlmsCaptureTransform`)
//! serializes the page *body* with empty metadata, so site chrome stays
//! out of the markdown companions. Moving the banner into the body would
//! silently add it to every `.md` companion and to `llms-full.txt`.
//!
//! Link targets inside the content are config space (project-root
//! relative), like footer text regions and the navbar title; the render
//! transform rewrites them per page.
//!
//! ## Skip conditions
//!
//! - announcement absent, `false`, or `null`;
//! - `true` or no usable content (warns `Q-13-16`);
//! - `navigation.announcement` already populated (user override).

use quarto_error_reporting::{DiagnosticMessage, DiagnosticMessageBuilder};
use quarto_navigation::{AnnouncementWarning, resolve_announcement};
use quarto_pandoc_types::config_value::ConfigValue;
use quarto_pandoc_types::pandoc::Pandoc;
use quarto_source_map::{By, SourceInfo};

use crate::Result;
use crate::render::RenderContext;
use crate::transform::{AstTransform, TransformPhase};

pub struct AnnouncementGenerateTransform;

impl AnnouncementGenerateTransform {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AnnouncementGenerateTransform {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait(?Send)]
impl AstTransform for AnnouncementGenerateTransform {
    fn name(&self) -> &str {
        "announcement-generate"
    }

    fn phase(&self) -> TransformPhase {
        TransformPhase::Navigation
    }

    async fn transform(&self, ast: &mut Pandoc, ctx: &mut RenderContext) -> Result<()> {
        if ast.meta.contains_path(&["navigation", "announcement"]) {
            return Ok(());
        }

        let (announcement, warnings) = resolve_announcement(&ast.meta);

        for warning in warnings {
            ctx.diagnostics.push(warning_diagnostic(&ast.meta, warning));
        }

        if let Some(announcement) = announcement {
            ast.meta.insert_path(
                &["navigation", "announcement"],
                announcement.to_config_value(),
            );
        }

        Ok(())
    }
}

/// The authored announcement value whose source location a diagnostic
/// should point at: the top-level spelling when present (it wins the
/// merge), else `website.announcement`.
fn authored_value(meta: &ConfigValue) -> Option<&ConfigValue> {
    meta.get("announcement")
        .or_else(|| meta.get_path(&["website", "announcement"]))
}

/// Location of `field` inside the authored value, falling back to the
/// value itself, then to a generated location.
fn location(meta: &ConfigValue, field: Option<&str>) -> SourceInfo {
    let value = authored_value(meta);
    field
        .and_then(|f| value.and_then(|v| v.get(f)))
        .or(value)
        .map_or_else(
            || SourceInfo::generated(By::programmatic_config()),
            |v| v.source_info.clone(),
        )
}

fn warning_diagnostic(meta: &ConfigValue, warning: AnnouncementWarning) -> DiagnosticMessage {
    match warning {
        AnnouncementWarning::NoContent => {
            DiagnosticMessageBuilder::warning("Announcement has no content")
                .with_code("Q-13-16")
                .problem(
                    "`website.announcement` is set, but has no text to show, so no banner \
                     is rendered.",
                )
                .add_hint(
                    "Give the text as a string or as `content:`; to turn the banner off, \
                     set `announcement: false`.",
                )
                .with_location(location(meta, Some("content")))
                .build()
        }
        AnnouncementWarning::UnknownType(value) => invalid_option(
            meta,
            "type",
            &value,
            "`primary`, `secondary`, `success`, `danger`, `warning`, `info`, `light`, `dark`",
            "primary",
        ),
        AnnouncementWarning::UnknownPosition(value) => invalid_option(
            meta,
            "position",
            &value,
            "`above-navbar`, `below-navbar`",
            "above-navbar",
        ),
    }
}

fn invalid_option(
    meta: &ConfigValue,
    field: &str,
    value: &str,
    allowed: &str,
    default: &str,
) -> DiagnosticMessage {
    DiagnosticMessageBuilder::warning("Invalid announcement option")
        .with_code("Q-13-17")
        .problem(format!(
            "`{value}` is not a valid announcement `{field}`; `{default}` is used instead."
        ))
        .add_hint(format!("The valid values are {allowed}."))
        .with_location(location(meta, Some(field)))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::Format;
    use crate::project::{DocumentInfo, ProjectConfig, ProjectContext};
    use crate::render::BinaryDependencies;
    use quarto_config::MergedConfig;
    use quarto_pandoc_types::ConfigMapEntry;
    use std::path::PathBuf;

    fn make_test_project() -> ProjectContext {
        ProjectContext {
            dir: PathBuf::from("/project"),
            config: ProjectConfig::default(),
            is_single_file: true,
            files: vec![DocumentInfo::from_path("/project/doc.qmd")],
            output_dir: PathBuf::from("/project"),
            ..Default::default()
        }
    }

    fn map(entries: Vec<(&str, ConfigValue)>) -> ConfigValue {
        ConfigValue::new_map(
            entries
                .into_iter()
                .map(|(k, v)| ConfigMapEntry {
                    key: k.to_string(),
                    key_source: SourceInfo::for_test(),
                    value: v,
                })
                .collect(),
            SourceInfo::for_test(),
        )
    }

    fn s(x: &str) -> ConfigValue {
        ConfigValue::new_string(x, SourceInfo::for_test())
    }

    fn b(x: bool) -> ConfigValue {
        ConfigValue::new_bool(x, SourceInfo::for_test())
    }

    fn website(announcement: ConfigValue) -> ConfigValue {
        map(vec![("website", map(vec![("announcement", announcement)]))])
    }

    async fn run(meta: ConfigValue) -> (ConfigValue, Vec<DiagnosticMessage>) {
        let mut ast = Pandoc {
            meta,
            blocks: vec![],
        };
        let project = make_test_project();
        let doc = DocumentInfo::from_path("/project/doc.qmd");
        let format = Format::html();
        let binaries = BinaryDependencies::new();
        let mut ctx = RenderContext::new(&project, &doc, &format, &binaries);
        AnnouncementGenerateTransform::new()
            .transform(&mut ast, &mut ctx)
            .await
            .unwrap();
        (ast.meta, ctx.diagnostics)
    }

    /// Merge layers the way project config and front matter merge
    /// (first = lowest priority).
    fn merged(layers: Vec<ConfigValue>) -> ConfigValue {
        let refs: Vec<&ConfigValue> = layers.iter().collect();
        MergedConfig::new(refs).materialize().unwrap()
    }

    #[tokio::test]
    async fn absent_stores_nothing() {
        let (meta, diags) = run(map(vec![])).await;
        assert!(!meta.contains_path(&["navigation", "announcement"]));
        assert!(diags.is_empty());
    }

    #[tokio::test]
    async fn string_form_is_stored() {
        let (meta, diags) = run(website(s("Hello"))).await;
        let stored = meta.get_path(&["navigation", "announcement"]).unwrap();
        assert_eq!(
            stored
                .get("content")
                .and_then(|v| v.as_plain_text())
                .as_deref(),
            Some("Hello")
        );
        assert_eq!(
            stored
                .get("type")
                .and_then(|v| v.as_plain_text())
                .as_deref(),
            Some("primary")
        );
        assert!(diags.is_empty());
    }

    #[tokio::test]
    async fn true_warns_q_13_16_and_stores_nothing() {
        let (meta, diags) = run(website(b(true))).await;
        assert!(!meta.contains_path(&["navigation", "announcement"]));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code.as_deref(), Some("Q-13-16"));
    }

    #[tokio::test]
    async fn unknown_type_warns_q_13_17_but_still_stores() {
        let (meta, diags) = run(website(map(vec![
            ("content", s("Hi")),
            ("type", s("loud")),
        ])))
        .await;
        assert!(meta.contains_path(&["navigation", "announcement"]));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code.as_deref(), Some("Q-13-17"));
    }

    #[tokio::test]
    async fn existing_navigation_announcement_is_kept() {
        let mut meta = website(s("Fresh"));
        meta.insert_path(
            &["navigation", "announcement"],
            map(vec![("content", s("Pre"))]),
        );
        let (meta, _) = run(meta).await;
        let stored = meta.get_path(&["navigation", "announcement"]).unwrap();
        assert_eq!(
            stored
                .get("content")
                .and_then(|v| v.as_plain_text())
                .as_deref(),
            Some("Pre")
        );
    }

    /// The per-page opt-out: project config sets the banner, front
    /// matter sets `website.announcement: false`, and the merged page
    /// metadata carries no banner — silently.
    #[tokio::test]
    async fn front_matter_false_overrides_project_map() {
        let project = website(map(vec![
            ("content", s("Unreviewed")),
            ("type", s("warning")),
        ]));
        let front_matter = website(b(false));
        let (meta, diags) = run(merged(vec![project, front_matter])).await;
        assert!(!meta.contains_path(&["navigation", "announcement"]));
        assert!(diags.is_empty());
    }

    /// A front-matter *map* merges field-wise over the project's.
    #[tokio::test]
    async fn front_matter_map_merges_field_wise() {
        let project = website(map(vec![
            ("content", s("Unreviewed")),
            ("type", s("warning")),
        ]));
        let front_matter = website(map(vec![("type", s("danger"))]));
        let (meta, _) = run(merged(vec![project, front_matter])).await;
        let stored = meta.get_path(&["navigation", "announcement"]).unwrap();
        assert_eq!(
            stored
                .get("content")
                .and_then(|v| v.as_plain_text())
                .as_deref(),
            Some("Unreviewed")
        );
        assert_eq!(
            stored
                .get("type")
                .and_then(|v| v.as_plain_text())
                .as_deref(),
            Some("danger")
        );
    }
}
