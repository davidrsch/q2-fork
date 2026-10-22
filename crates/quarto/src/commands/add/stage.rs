//! Finding and validating the extensions in a fetched source
//! (bd-rfz4e3bw).
//!
//! By the time this runs, `quarto-source-fetch` has produced a
//! directory: a local path, or an extracted archive with its single
//! top-level wrapper already removed. This module finds the
//! `_extensions/` folder in it, reads every extension there, and
//! refuses the source — before anything is planned — when it has none,
//! or when an extension's manifest does not load.

use std::path::{Path, PathBuf};

use quarto_core::extension::{Extension, read_extensions_dir};
use quarto_system_runtime::NativeRuntime;

use crate::commands::common::plan::CommandFailure;

pub const EXTENSIONS_DIR: &str = "_extensions";

/// Find the folder whose entries are the extensions to install.
///
/// Mirrors Quarto 1's two lookups, `extensionDir` (`install.ts:609`)
/// for local folders and `findExtensionDir` (`install.ts:283`) for
/// archives, which agree on every source either can see:
///
/// 1. a path that *is* an `_extensions` folder;
/// 2. a path containing `_extensions/`;
/// 3. a path whose only subdirectory contains `_extensions/` (an
///    archive wrapped one level deeper than usual);
/// 4. otherwise the path itself, read as if it were `_extensions/`.
pub fn locate_extensions_dir(source: &Path) -> PathBuf {
    if source.file_name().is_some_and(|n| n == EXTENSIONS_DIR) {
        return source.to_path_buf();
    }
    let direct = source.join(EXTENSIONS_DIR);
    if direct.is_dir() {
        return direct;
    }
    if let Ok(entries) = std::fs::read_dir(source) {
        let dirs: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        if let [only] = dirs.as_slice() {
            let nested = only.join(EXTENSIONS_DIR);
            if nested.is_dir() {
                return nested;
            }
        }
    }
    source.to_path_buf()
}

/// Read the extensions in `extensions_dir`, sorted by id.
///
/// Every manifest must load. Quarto 1 refuses a source whose
/// `_extension.yml` fails validation; installing a broken extension
/// only moves the failure to render time, in a document that never
/// mentions the extension's name.
pub fn read_staged(
    extensions_dir: &Path,
    target_label: &str,
) -> Result<Vec<Extension>, CommandFailure> {
    let (mut extensions, diagnostics) = read_extensions_dir(extensions_dir, &NativeRuntime::new());
    if !diagnostics.is_empty() {
        let details: Vec<String> = diagnostics
            .iter()
            .map(|d| {
                d.problem
                    .as_ref()
                    .map_or_else(|| d.title.clone(), |p| p.as_str().to_string())
            })
            .collect();
        return Err(CommandFailure::new(
            format!("{target_label} contains an extension that does not load"),
            format!("{}\n\nNothing was written.", details.join("\n")),
        ));
    }
    if extensions.is_empty() {
        return Err(CommandFailure::new(
            format!("No extensions found in {target_label}"),
            format!(
                "Looked for extensions (folders with an _extension.yml) in {}. An extension \
                 source must contain an `_extensions/` folder, at its top level or one folder \
                 down.\n\nNothing was written.",
                extensions_dir.display()
            ),
        ));
    }
    extensions.sort_by(|a, b| {
        (a.id.organization.as_deref(), &a.id.name).cmp(&(b.id.organization.as_deref(), &b.id.name))
    });
    Ok(extensions)
}

/// Accept `segment` as one directory name under `_extensions/`.
///
/// Extension and organization names come from the source — a GitHub
/// org the user typed, and folder names inside a downloaded archive —
/// and are joined onto the user's project to choose a directory that
/// is **deleted** on update. Anything but a plain name is refused.
pub fn validate_segment(segment: &str, what: &str) -> Result<(), CommandFailure> {
    let reason = if segment.is_empty() {
        Some("it is empty")
    } else if segment == "." || segment == ".." {
        Some("it is a relative path component")
    } else if segment.contains(['/', '\\', ':']) || segment.contains('\0') {
        Some("it contains a path separator")
    } else if segment.starts_with('.') {
        // `_extensions/.q2-staging-*` is ours; hidden names are also
        // skipped by people's tooling in surprising ways.
        Some("it starts with `.`")
    } else {
        None
    };
    match reason {
        Some(reason) => Err(CommandFailure::new(
            format!("Refusing to install an extension with {what} {segment:?}"),
            format!(
                "The {what} cannot be used as a folder name: {reason}.\n\nNothing was written."
            ),
        )),
        None => Ok(()),
    }
}

/// The type column of the confirmation table, e.g. `(format,filters)`.
///
/// Q1's `typeStr` (`install.ts:419-450`) covers formats, shortcodes,
/// and filters. q2 extensions also contribute project types and
/// engines, which a reader deciding whether to trust an extension
/// would want to see, so those are listed after Q1's three.
pub fn type_summary(extension: &Extension) -> String {
    let c = &extension.contributes;
    let mut kinds: Vec<&str> = Vec::new();
    let mut push = |count: usize, one: &'static str, many: &'static str| {
        if count > 0 {
            kinds.push(if count == 1 { one } else { many });
        }
    };
    push(c.formats.len(), "format", "formats");
    push(c.shortcodes.len(), "shortcode", "shortcodes");
    push(c.filters.len(), "filter", "filters");
    push(usize::from(c.project.is_some()), "project", "project");
    push(c.engines.len(), "engine", "engines");
    if kinds.is_empty() {
        String::new()
    } else {
        format!("({})", kinds.join(","))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tree(files: &[(&str, &str)]) -> TempDir {
        let dir = TempDir::new().unwrap();
        for (name, content) in files {
            let path = dir.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }
        dir
    }

    const MANIFEST: &str = "title: Ext\nversion: 1.0.0\ncontributes:\n  shortcodes:\n    - s.lua\n";

    #[test]
    fn the_extensions_folder_is_found_at_each_supported_depth() {
        let top = tree(&[("_extensions/a/_extension.yml", MANIFEST)]);
        assert_eq!(
            locate_extensions_dir(top.path()),
            top.path().join("_extensions")
        );

        let itself = top.path().join("_extensions");
        assert_eq!(locate_extensions_dir(&itself), itself);

        let wrapped = tree(&[("repo-main/_extensions/a/_extension.yml", MANIFEST)]);
        assert_eq!(
            locate_extensions_dir(wrapped.path()),
            wrapped.path().join("repo-main").join("_extensions")
        );

        let bare = tree(&[("a/_extension.yml", MANIFEST)]);
        assert_eq!(locate_extensions_dir(bare.path()), bare.path());
    }

    #[test]
    fn staged_extensions_are_read_in_id_order() {
        let src = tree(&[
            ("_extensions/zeta/_extension.yml", MANIFEST),
            ("_extensions/org/alpha/_extension.yml", MANIFEST),
            ("_extensions/beta/_extension.yml", MANIFEST),
        ]);
        let exts = read_staged(&src.path().join("_extensions"), "src").unwrap();
        let ids: Vec<String> = exts.iter().map(|e| e.id.to_string()).collect();
        assert_eq!(ids, ["beta", "zeta", "org/alpha"]);
    }

    #[test]
    fn a_source_without_extensions_is_refused() {
        let src = tree(&[("README.md", "hi")]);
        let err = read_staged(src.path(), "org/repo").unwrap_err();
        let text = err.0.to_text(None);
        assert!(text.contains("No extensions found in org/repo"), "{text}");
        assert!(text.contains("Nothing was written"), "{text}");
    }

    #[test]
    fn a_manifest_that_does_not_load_is_refused() {
        let src = tree(&[("_extensions/bad/_extension.yml", "title: no contributes\n")]);
        let err = read_staged(&src.path().join("_extensions"), "src").unwrap_err();
        let text = err.0.to_text(None);
        assert!(text.contains("does not load"), "{text}");
        assert!(
            text.contains("contributes"),
            "the loader's reason is shown: {text}"
        );
    }

    #[test]
    fn only_plain_names_are_valid_segments() {
        for ok in ["fontawesome", "posit-dev", "a.b", "a_b"] {
            assert!(validate_segment(ok, "name").is_ok(), "{ok}");
        }
        for bad in ["", ".", "..", "a/b", "a\\b", "c:", ".hidden"] {
            assert!(validate_segment(bad, "name").is_err(), "{bad:?}");
        }
    }
}
