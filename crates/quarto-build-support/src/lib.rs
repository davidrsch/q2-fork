//! Helpers shared by the workspace's build scripts (bd-7xlph6ki).
//!
//! Several build scripts embed a generated tree (an SPA `dist/`, a JS
//! bundle, the staged docs) that may or may not exist, and ask cargo to
//! rerun them when it changes. Cargo treats a `cargo:rerun-if-changed`
//! path that does not exist as stale on *every* invocation, so watching
//! a missing tree directly makes the crate, and everything above it,
//! recompile on every `cargo build`/`cargo run` (GH #870). Use
//! [`watch_path`] instead of emitting `rerun-if-changed` by hand.
//!
//! Build scripts that rewrite an output read by `include_bytes!`
//! should use [`write_if_changed`], so a rerun that produces identical
//! bytes keeps the output's mtime and its dependents stay fresh.

use std::path::{Component, Path, PathBuf};

/// Emit `cargo:rerun-if-changed` lines for `path`; see [`watch_list`]
/// for exactly which paths are watched.
pub fn watch_path(path: &Path) {
    for watched in watch_list(path) {
        println!("cargo:rerun-if-changed={}", watched.display());
    }
}

/// The paths [`watch_path`] watches for `path`, which is first
/// normalized lexically (`a/b/../c` becomes `a/c`):
///
/// - an existing file: the file itself;
/// - an existing directory: the directory and every file and directory
///   under it, recursively. The directory's own mtime catches additions
///   and removals; the per-entry watches catch in-place rewrites, which a
///   directory-mtime watch misses;
/// - a missing path: its nearest existing ancestor directory, whose mtime
///   changes when the missing child (or anything on the way to it) is
///   created. Never the missing path itself, which cargo would treat as
///   always stale.
pub fn watch_list(path: &Path) -> Vec<PathBuf> {
    let path = normalize(path);
    if path.is_file() {
        return vec![path];
    }
    if path.is_dir() {
        let mut out = vec![path.clone()];
        collect_recursive(&path, &mut out);
        return out;
    }
    path.ancestors()
        .skip(1)
        .find(|ancestor| ancestor.is_dir())
        .map(|ancestor| vec![ancestor.to_path_buf()])
        .unwrap_or_default()
}

/// Like [`watch_path`], but an existing directory is watched by its own
/// entry only, not recursively: its mtime catches files appearing or
/// vanishing, but not in-place rewrites. For build scripts that pick one
/// file out of a directory and watch that file themselves.
pub fn watch_entry(path: &Path) {
    for watched in watch_entry_list(path) {
        println!("cargo:rerun-if-changed={}", watched.display());
    }
}

/// The paths [`watch_entry`] watches for `path`: the (normalized) path
/// itself when it exists, else its nearest existing ancestor directory.
pub fn watch_entry_list(path: &Path) -> Vec<PathBuf> {
    let path = normalize(path);
    if path.exists() {
        return vec![path];
    }
    watch_list(&path)
}

fn collect_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        out.push(path.clone());
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            collect_recursive(&path, out);
        }
    }
}

/// Resolve `.` and `..` components lexically, without touching the
/// filesystem. Build scripts build paths like
/// `<manifest_dir>/../../hub-client/dist`; walking ancestors of such a
/// path would otherwise strip a `..` and land on the wrong directory.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                } else {
                    out.push(component);
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// Write `contents` to `path` unless it already holds exactly those
/// bytes. Returns whether the file was written. Leaving an identical
/// file untouched keeps its mtime, so an `include_bytes!` of it does not
/// force its crate to recompile.
pub fn write_if_changed(path: &Path, contents: &[u8]) -> std::io::Result<bool> {
    if std::fs::read(path).is_ok_and(|existing| existing == contents) {
        return Ok(false);
    }
    std::fs::write(path, contents)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_path_watches_nearest_existing_ancestor() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("hub-client").join("dist").join("assets");
        assert_eq!(watch_list(&missing), vec![tmp.path().to_path_buf()]);

        std::fs::create_dir(tmp.path().join("hub-client")).unwrap();
        assert_eq!(watch_list(&missing), vec![tmp.path().join("hub-client")]);
    }

    #[test]
    fn missing_path_is_never_watched() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("agents-docs-dist");
        assert!(!watch_list(&missing).contains(&missing));
    }

    #[test]
    fn existing_file_watches_itself() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("index.mjs");
        std::fs::write(&file, "x").unwrap();
        assert_eq!(watch_list(&file), vec![file]);
    }

    #[test]
    fn existing_dir_watches_every_entry_recursively() {
        let tmp = tempfile::tempdir().unwrap();
        let dist = tmp.path().join("dist");
        std::fs::create_dir_all(dist.join("assets")).unwrap();
        std::fs::write(dist.join("index.html"), "x").unwrap();
        std::fs::write(dist.join("assets").join("app.js"), "x").unwrap();
        assert_eq!(
            watch_list(&dist),
            vec![
                dist.clone(),
                dist.join("assets"),
                dist.join("assets").join("app.js"),
                dist.join("index.html"),
            ]
        );
    }

    #[test]
    fn parent_dir_components_are_resolved_before_walking_up() {
        let tmp = tempfile::tempdir().unwrap();
        let crate_dir = tmp.path().join("crates").join("quarto-preview");
        std::fs::create_dir_all(&crate_dir).unwrap();
        // `<crate>/../../hub-client/dist` with `hub-client/` missing: the
        // nearest existing ancestor is the workspace root, not `crates/`
        // (which a naive walk that strips the trailing `..` would reach).
        let missing = crate_dir
            .join("..")
            .join("..")
            .join("hub-client")
            .join("dist");
        assert_eq!(watch_list(&missing), vec![tmp.path().to_path_buf()]);
    }

    #[test]
    fn watch_entry_does_not_recurse_into_existing_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let assets = tmp.path().join("dist").join("assets");
        assert_eq!(watch_entry_list(&assets), vec![tmp.path().to_path_buf()]);

        std::fs::create_dir_all(&assets).unwrap();
        std::fs::write(assets.join("app.wasm"), "x").unwrap();
        assert_eq!(watch_entry_list(&assets), vec![assets]);
    }

    #[test]
    fn write_if_changed_leaves_identical_files_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("archive.tar.zst");
        assert!(write_if_changed(&file, b"one").unwrap());
        assert!(!write_if_changed(&file, b"one").unwrap());
        assert!(write_if_changed(&file, b"two").unwrap());
        assert_eq!(std::fs::read(&file).unwrap(), b"two");
    }
}
