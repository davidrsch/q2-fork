//! Schema and loader for `resources/vendored.toml`, the registry of every
//! vendored third-party asset in the repo (bd-xm7l).
//!
//! The registry records *what* is vendored, *where*, *at which version*, and
//! *what decides that version*. How to refresh an asset stays in the README
//! next to its bytes (`procedure`). Two consumers read it:
//!
//! - the `vendored-registry` lint (`crate::lint::vendored_registry`), which
//!   checks the registry against the tree offline on every `cargo xtask lint`;
//! - `cargo xtask vendored outdated` (`super::outdated`), which asks each
//!   upstream what is newer.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

/// Path to the registry, relative to the workspace root.
pub const REGISTRY_REL: &str = "resources/vendored.toml";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registry {
    pub coverage: Coverage,
    #[serde(default, rename = "group")]
    pub groups: Vec<Group>,
    #[serde(default, rename = "asset")]
    pub assets: Vec<Asset>,
}

/// Which committed files must be claimed by some asset's `paths`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Coverage {
    /// Repo-relative directory roots. A `*` component matches exactly one
    /// path component (e.g. `crates/*/vendor`).
    pub roots: Vec<String>,
}

/// A reporting key: assets that share a group are surveyed and filed
/// together (one strand per group per survey).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub id: String,
    pub title: String,
    /// Repo-relative files or directories this asset owns.
    pub paths: Vec<String>,
    pub policy: Policy,
    #[serde(default)]
    pub group: Option<String>,
    /// Asset id whose version decides this one's (`policy = "follow"`).
    #[serde(default)]
    pub follows: Option<String>,
    /// The pinned version or upstream ref. `"*"` means deliberately
    /// unpinned (CDN `latest`). Absent means unrecorded — an inventory gap.
    #[serde(default)]
    pub version: Option<String>,
    /// Places in the tree that must agree with `version`.
    #[serde(default)]
    pub version_sources: Vec<VersionSource>,
    #[serde(default)]
    pub upstream: Option<Upstream>,
    /// A repo-relative directory that `paths[0]` is a byte-identical copy
    /// (or subset) of.
    #[serde(default)]
    pub copy_of: Option<String>,
    /// Tests that guard this asset (free-form `<crate> <test path>`).
    #[serde(default)]
    pub guards: Vec<String>,
    /// Where the refresh procedure is documented.
    #[serde(default)]
    pub procedure: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    /// `YYYY-MM-DD` of the last survey that checked this asset.
    #[serde(default)]
    pub last_reviewed: Option<String>,
    /// Free-form context for the agent doing an upgrade.
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Policy {
    /// Compare against `upstream`; report when upstream moved.
    Track,
    /// Version is dictated by the `follows` asset; reported with it.
    Follow,
    /// Deliberately never updated (test fixtures).
    Frozen,
    /// Repo-owned; listed only so coverage and discovery stop flagging it.
    Native,
    /// Managed elsewhere (Cargo `[patch]`, PINS.md); referenced, not checked.
    External,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum VersionSource {
    /// `file` contains `version` literally.
    FileContains { file: String },
    /// `regex`'s first capture group in `file` equals `version`.
    FileRegex { file: String, regex: String },
    /// The dotted `key` in JSON `file` equals `version`.
    JsonKey { file: String, key: String },
    /// Some component of `paths[0]` equals `version`.
    PathSegment,
    /// The latest `git-subtree-split` trailer for `paths[0]` equals
    /// `version`. Needs full history, so only `outdated` checks it.
    SubtreeSplit,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Upstream {
    Npm {
        package: String,
    },
    TypstUniverse {
        package: String,
    },
    CratesIo {
        #[serde(rename = "crate")]
        krate: String,
    },
    Git {
        repo: String,
        track: GitTrack,
        /// Tag = `tag_prefix` + version (e.g. `v` for `v6.3.289`).
        #[serde(default)]
        tag_prefix: Option<String>,
        /// Upstream paths this asset was copied from. When set, "outdated"
        /// means these paths changed between `version` and the latest ref,
        /// not merely that a newer ref exists.
        #[serde(default)]
        paths: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum GitTrack {
    /// The highest version-like tag (after stripping `tag_prefix`).
    Tags,
    /// The head of this branch.
    Branch(String),
}

impl Registry {
    pub fn parse(text: &str) -> Result<Registry> {
        toml::from_str(text).context("parsing the vendored registry")
    }

    pub fn load(workspace_root: &Path) -> Result<Registry> {
        let path = workspace_root.join(REGISTRY_REL);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text)
    }

    pub fn asset(&self, id: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.id == id)
    }
}

impl Asset {
    /// Whether repo-relative `file` falls under one of this asset's paths.
    pub fn claims(&self, file: &str) -> bool {
        self.paths.iter().any(|p| path_is_within(file, p))
    }
}

/// `file` equals `dir` or lies beneath it (component-wise, `/`-separated).
pub fn path_is_within(file: &str, dir: &str) -> bool {
    let dir = dir.trim_end_matches('/');
    file == dir
        || (file.len() > dir.len() && file.starts_with(dir) && file.as_bytes()[dir.len()] == b'/')
}

/// Whether repo-relative `file` lies under coverage `root`, where a `*`
/// component of `root` matches exactly one component of `file`.
pub fn under_root(file: &str, root: &str) -> bool {
    let mut file_parts = file.split('/');
    for root_part in root.trim_end_matches('/').split('/') {
        match file_parts.next() {
            Some(f) if root_part == "*" || root_part == f => {}
            _ => return false,
        }
    }
    // The root must be a strict directory prefix of the file.
    file_parts.next().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_minimal_registry() {
        let reg = Registry::parse(
            r#"
            [coverage]
            roots = ["resources"]

            [[group]]
            id = "quarto-cli"
            title = "Quarto 1"

            [[asset]]
            id = "mermaid"
            title = "mermaid"
            paths = ["resources/mermaid"]
            policy = "track"
            version = "11.12.0"
            version_sources = [
              { kind = "file-contains", file = "resources/mermaid/README.md" },
              { kind = "path-segment" },
            ]
            upstream = { kind = "npm", package = "mermaid" }
            notes = '''
            multi-line
            '''

            [[asset]]
            id = "subtree"
            title = "a subtree"
            paths = ["resources/extension-subtrees/x"]
            policy = "track"
            upstream = { kind = "git", repo = "https://example.com/x.git", track = { branch = "main" } }
            "#,
        )
        .unwrap();
        assert_eq!(reg.assets.len(), 2);
        assert_eq!(reg.groups[0].id, "quarto-cli");
        assert!(matches!(
            reg.assets[1].upstream,
            Some(Upstream::Git { track: GitTrack::Branch(ref b), .. }) if b == "main"
        ));
        assert!(
            reg.assets[0]
                .notes
                .as_deref()
                .unwrap()
                .contains("multi-line")
        );
    }

    #[test]
    fn rejects_unknown_fields() {
        let err = Registry::parse(
            r#"
            [coverage]
            roots = []
            [[asset]]
            id = "a"
            title = "a"
            paths = []
            policy = "track"
            verison = "1"
            "#,
        );
        assert!(err.is_err());
    }

    #[test]
    fn path_containment_is_component_wise() {
        assert!(path_is_within("resources/js/a.js", "resources/js"));
        assert!(path_is_within("resources/js", "resources/js/"));
        assert!(!path_is_within("resources/json/a", "resources/js"));
    }

    #[test]
    fn coverage_roots_support_single_component_wildcards() {
        assert!(under_root(
            "crates/quarto-math/vendor/a/b.rs",
            "crates/*/vendor"
        ));
        assert!(!under_root(
            "crates/quarto-math/src/vendor.rs",
            "crates/*/vendor"
        ));
        assert!(!under_root("crates/vendor", "crates/*/vendor"));
        assert!(under_root("resources/x", "resources"));
        assert!(!under_root("resources", "resources"));
    }
}
