/*
 * announcement.rs
 * Copyright (c) 2026 Posit, PBC
 */

//! Announcement-banner data model and YAML resolution
//! (bd-website-announcement-w1gtpzki).
//!
//! [`resolve_announcement`] reads `website.announcement` (or a top-level
//! `announcement:`, merged by [`resolve_website_value`]) from the merged
//! `ast.meta` and returns an [`Announcement`] populated with Quarto 1's
//! defaults, or `None` when the banner is absent or disabled.
//!
//! Accepted shapes, matching Quarto 1's schema
//! (`src/resources/schema/definitions.yml`, `website.announcement`):
//!
//! - `false` / `null` — no banner. This is how a page opts out of a
//!   project-wide announcement: front matter is merged over the project
//!   config, and the highest-priority layer's scalar wins over a lower
//!   layer's map.
//! - a string — the content; every other field takes its default.
//! - a map — `content` plus `dismissable`, `icon`, `position`, `type`.
//!   q2 adds `remember-dismissal` (see [`Announcement::remember_dismissal`]).
//!
//! `true` and a map without usable `content` produce no banner and a
//! [`AnnouncementWarning::NoContent`], since either is almost certainly a
//! configuration mistake. Unknown `type`/`position` values fall back to the
//! default with a warning.
//!
//! The content stays a [`ConfigValue`] so markdown parsed by
//! `ConfigMarkdownTransform` (inlines or blocks) survives to the renderer.

use quarto_config::resolve_website_value;
use quarto_pandoc_types::ConfigMapEntry;
use quarto_pandoc_types::config_value::{
    ConfigValue, ConfigValueKind, blocks_to_plain_text, inlines_to_plain_text,
};
use quarto_source_map::{By, SourceInfo};
use sha2::{Digest, Sha256};

/// The Bootstrap alert variant (`type:`), which selects the
/// `alert-<type>` class and therefore the banner's colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnnouncementType {
    #[default]
    Primary,
    Secondary,
    Success,
    Danger,
    Warning,
    Info,
    Light,
    Dark,
}

impl AnnouncementType {
    const ALL: [AnnouncementType; 8] = [
        AnnouncementType::Primary,
        AnnouncementType::Secondary,
        AnnouncementType::Success,
        AnnouncementType::Danger,
        AnnouncementType::Warning,
        AnnouncementType::Info,
        AnnouncementType::Light,
        AnnouncementType::Dark,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            AnnouncementType::Primary => "primary",
            AnnouncementType::Secondary => "secondary",
            AnnouncementType::Success => "success",
            AnnouncementType::Danger => "danger",
            AnnouncementType::Warning => "warning",
            AnnouncementType::Info => "info",
            AnnouncementType::Light => "light",
            AnnouncementType::Dark => "dark",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.as_str() == s)
    }
}

/// Where the banner sits inside `#quarto-header`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnnouncementPosition {
    /// Before the navbar (Quarto 1's default).
    #[default]
    AboveNavbar,
    /// After the navbar and the narrow-viewport secondary nav.
    BelowNavbar,
}

impl AnnouncementPosition {
    pub fn as_str(self) -> &'static str {
        match self {
            AnnouncementPosition::AboveNavbar => "above-navbar",
            AnnouncementPosition::BelowNavbar => "below-navbar",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "above-navbar" => Some(AnnouncementPosition::AboveNavbar),
            "below-navbar" => Some(AnnouncementPosition::BelowNavbar),
            _ => None,
        }
    }
}

/// A problem found while resolving the announcement config. The caller
/// (`AnnouncementGenerateTransform` in quarto-core) turns these into
/// diagnostics; this crate stays diagnostic-free like its siblings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnouncementWarning {
    /// `announcement: true`, or a map/string with no usable content.
    NoContent,
    /// A `type:` outside the Bootstrap alert variants.
    UnknownType(String),
    /// A `position:` other than `above-navbar` / `below-navbar`.
    UnknownPosition(String),
}

/// Fully resolved announcement banner.
#[derive(Debug, Clone, PartialEq)]
pub struct Announcement {
    /// Stable dismissal key, a content hash (see [`content_id`]). Emitted
    /// as `data-announcement-id`; `quarto-nav.js` stores dismissals
    /// under `quarto-announce-<id>`, so editing the content re-shows the
    /// banner to readers who dismissed the old one.
    pub id: String,
    /// The banner body: a string, `PandocInlines`, or `PandocBlocks`.
    pub content: ConfigValue,
    /// Whether the reader can close the banner. Default `true`.
    pub dismissable: bool,
    /// Whether a dismissal is remembered in `localStorage` (Quarto 1
    /// behavior, the default) or applies to the current page view only.
    /// A q2 addition; meaningless when `dismissable` is `false`.
    pub remember_dismissal: bool,
    /// `type:` — the Bootstrap alert variant.
    pub kind: AnnouncementType,
    pub position: AnnouncementPosition,
    /// Bootstrap icon name, without the `bi-` prefix.
    pub icon: Option<String>,
}

impl Announcement {
    /// Parse an announcement from its authored shape (string or map) or
    /// from the serialized form [`Announcement::to_config_value`]
    /// produces. A stored `id` is kept; otherwise it is computed from
    /// the content.
    ///
    /// Booleans and nulls are not handled here — [`resolve_announcement`]
    /// strips them first — and are treated as having no content.
    pub fn from_config_value(cv: &ConfigValue) -> (Option<Announcement>, Vec<AnnouncementWarning>) {
        let mut warnings = Vec::new();

        let content = if cv.as_map_entries().is_some() {
            cv.get("content").cloned()
        } else {
            Some(cv.clone())
        };
        let Some(content) = content.filter(has_content) else {
            warnings.push(AnnouncementWarning::NoContent);
            return (None, warnings);
        };

        let field_text = |key: &str| {
            cv.get(key)
                .and_then(|v| v.as_plain_text())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };

        let kind = match field_text("type") {
            None => AnnouncementType::default(),
            Some(s) => AnnouncementType::parse(&s).unwrap_or_else(|| {
                warnings.push(AnnouncementWarning::UnknownType(s));
                AnnouncementType::default()
            }),
        };
        let position = match field_text("position") {
            None => AnnouncementPosition::default(),
            Some(s) => AnnouncementPosition::parse(&s).unwrap_or_else(|| {
                warnings.push(AnnouncementWarning::UnknownPosition(s));
                AnnouncementPosition::default()
            }),
        };

        // Q1: `dismissable: announcementRaw.dismissable !== false`.
        let flag = |key: &str| cv.get(key).and_then(|v| v.as_bool()) != Some(false);

        let announcement = Announcement {
            id: field_text("id").unwrap_or_else(|| content_id(&content)),
            content,
            dismissable: flag("dismissable"),
            remember_dismissal: flag("remember-dismissal"),
            kind,
            position,
            icon: field_text("icon"),
        };
        (Some(announcement), warnings)
    }

    /// Serialize to a map suitable for storage at
    /// `navigation.announcement`. Every field is written, so the stored
    /// form is self-describing for a downstream filter.
    pub fn to_config_value(&self) -> ConfigValue {
        let info = SourceInfo::generated(By::programmatic_config());
        let entry = |key: &str, value: ConfigValue| ConfigMapEntry {
            key: key.to_string(),
            key_source: info.clone(),
            value,
        };
        let mut entries = vec![
            entry("id", ConfigValue::new_string(&self.id, info.clone())),
            entry("content", self.content.clone()),
            entry(
                "dismissable",
                ConfigValue::new_bool(self.dismissable, info.clone()),
            ),
            entry(
                "remember-dismissal",
                ConfigValue::new_bool(self.remember_dismissal, info.clone()),
            ),
            entry(
                "type",
                ConfigValue::new_string(self.kind.as_str(), info.clone()),
            ),
            entry(
                "position",
                ConfigValue::new_string(self.position.as_str(), info.clone()),
            ),
        ];
        if let Some(ref icon) = self.icon {
            entries.push(entry("icon", ConfigValue::new_string(icon, info.clone())));
        }
        ConfigValue::new_map(entries, info)
    }
}

/// Resolve the announcement from merged page metadata.
///
/// Reads `website.announcement` and top-level `announcement`, merged by
/// [`resolve_website_value`] (top-level wins). Returns `(None, [])` when
/// the key is absent, `false`, or `null`; `(None, [NoContent])` for
/// `true`.
pub fn resolve_announcement(
    meta: &ConfigValue,
) -> (Option<Announcement>, Vec<AnnouncementWarning>) {
    let Some(cv) = resolve_website_value(meta, "announcement") else {
        return (None, Vec::new());
    };
    if cv.is_null() {
        return (None, Vec::new());
    }
    match cv.as_bool() {
        Some(false) => (None, Vec::new()),
        Some(true) => (None, vec![AnnouncementWarning::NoContent]),
        None => Announcement::from_config_value(&cv),
    }
}

/// The dismissal id: the first 16 hex digits of the SHA-256 of the
/// content's plain text.
///
/// Plain text, not rendered HTML: href rewriting makes the HTML differ
/// with page depth, and a dismissal on one page must stick on the others.
/// The text is also what a reader recognizes as "the same announcement".
pub fn content_id(content: &ConfigValue) -> String {
    let digest = Sha256::digest(content_plain_text(content).as_bytes());
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

fn content_plain_text(cv: &ConfigValue) -> String {
    match &cv.value {
        ConfigValueKind::PandocInlines(inlines) => inlines_to_plain_text(inlines),
        ConfigValueKind::PandocBlocks(blocks) => blocks_to_plain_text(blocks),
        _ => cv.as_plain_text().unwrap_or_default(),
    }
}

/// Whether a content value has something to show. Inlines and blocks
/// count when non-empty (an image-only banner has no plain text but is
/// still content); strings count when not blank.
fn has_content(cv: &ConfigValue) -> bool {
    match &cv.value {
        ConfigValueKind::PandocInlines(inlines) => !inlines.is_empty(),
        ConfigValueKind::PandocBlocks(blocks) => !blocks.is_empty(),
        ConfigValueKind::Scalar { .. } => cv.as_plain_text().is_some_and(|s| !s.trim().is_empty()),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(entries: Vec<(&str, ConfigValue)>) -> ConfigValue {
        let info = SourceInfo::for_test();
        ConfigValue::new_map(
            entries
                .into_iter()
                .map(|(k, v)| ConfigMapEntry {
                    key: k.to_string(),
                    key_source: info.clone(),
                    value: v,
                })
                .collect(),
            info,
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

    #[test]
    fn absent_is_none_without_warning() {
        let (a, w) = resolve_announcement(&map(vec![]));
        assert!(a.is_none());
        assert!(w.is_empty());
    }

    #[test]
    fn false_and_null_are_silent_opt_outs() {
        for value in [b(false), ConfigValue::null(SourceInfo::for_test())] {
            let (a, w) = resolve_announcement(&website(value));
            assert!(a.is_none());
            assert!(w.is_empty());
        }
    }

    #[test]
    fn true_warns_no_content() {
        let (a, w) = resolve_announcement(&website(b(true)));
        assert!(a.is_none());
        assert_eq!(w, vec![AnnouncementWarning::NoContent]);
    }

    #[test]
    fn string_form_takes_q1_defaults() {
        let (a, w) = resolve_announcement(&website(s("Hello")));
        let a = a.expect("announcement");
        assert!(w.is_empty());
        assert_eq!(a.content.as_plain_text().as_deref(), Some("Hello"));
        assert!(a.dismissable);
        assert!(a.remember_dismissal);
        assert_eq!(a.kind, AnnouncementType::Primary);
        assert_eq!(a.position, AnnouncementPosition::AboveNavbar);
        assert_eq!(a.icon, None);
        assert_eq!(a.id.len(), 16);
    }

    #[test]
    fn map_form_reads_every_field() {
        let cv = map(vec![
            ("content", s("Hi")),
            ("dismissable", b(false)),
            ("remember-dismissal", b(false)),
            ("type", s("warning")),
            ("position", s("below-navbar")),
            ("icon", s("info-circle")),
        ]);
        let (a, w) = resolve_announcement(&website(cv));
        let a = a.expect("announcement");
        assert!(w.is_empty());
        assert!(!a.dismissable);
        assert!(!a.remember_dismissal);
        assert_eq!(a.kind, AnnouncementType::Warning);
        assert_eq!(a.position, AnnouncementPosition::BelowNavbar);
        assert_eq!(a.icon.as_deref(), Some("info-circle"));
    }

    #[test]
    fn top_level_spelling_is_accepted() {
        let meta = map(vec![("announcement", s("Top"))]);
        let (a, _) = resolve_announcement(&meta);
        assert_eq!(
            a.expect("announcement").content.as_plain_text().as_deref(),
            Some("Top")
        );
    }

    #[test]
    fn top_level_false_beats_website_map() {
        let meta = map(vec![
            ("announcement", b(false)),
            (
                "website",
                map(vec![("announcement", map(vec![("content", s("Hi"))]))]),
            ),
        ]);
        let (a, w) = resolve_announcement(&meta);
        assert!(a.is_none());
        assert!(w.is_empty());
    }

    #[test]
    fn missing_or_blank_content_warns() {
        for cv in [
            map(vec![("type", s("info"))]),
            map(vec![("content", s("   "))]),
            s(""),
        ] {
            let (a, w) = resolve_announcement(&website(cv));
            assert!(a.is_none());
            assert_eq!(w, vec![AnnouncementWarning::NoContent]);
        }
    }

    #[test]
    fn unknown_type_and_position_fall_back_with_warnings() {
        let cv = map(vec![
            ("content", s("Hi")),
            ("type", s("loud")),
            ("position", s("sideways")),
        ]);
        let (a, w) = resolve_announcement(&website(cv));
        let a = a.expect("announcement still renders");
        assert_eq!(a.kind, AnnouncementType::Primary);
        assert_eq!(a.position, AnnouncementPosition::AboveNavbar);
        assert_eq!(
            w,
            vec![
                AnnouncementWarning::UnknownType("loud".to_string()),
                AnnouncementWarning::UnknownPosition("sideways".to_string()),
            ]
        );
    }

    #[test]
    fn id_depends_only_on_content_text() {
        let a = Announcement::from_config_value(&s("Same")).0.unwrap();
        let b_ = Announcement::from_config_value(&map(vec![
            ("content", s("Same")),
            ("type", s("danger")),
        ]))
        .0
        .unwrap();
        let c = Announcement::from_config_value(&s("Different")).0.unwrap();
        assert_eq!(a.id, b_.id);
        assert_ne!(a.id, c.id);
    }

    #[test]
    fn round_trips_through_config_value() {
        let cv = map(vec![
            ("content", s("Hi")),
            ("dismissable", b(false)),
            ("remember-dismissal", b(false)),
            ("type", s("dark")),
            ("position", s("below-navbar")),
            ("icon", s("megaphone")),
        ]);
        let original = Announcement::from_config_value(&cv).0.unwrap();
        let (again, w) = Announcement::from_config_value(&original.to_config_value());
        assert!(w.is_empty());
        assert_eq!(again.unwrap(), original);
    }

    #[test]
    fn stored_id_is_preserved() {
        let cv = map(vec![("content", s("Hi")), ("id", s("custom-id"))]);
        let a = Announcement::from_config_value(&cv).0.unwrap();
        assert_eq!(a.id, "custom-id");
    }
}
