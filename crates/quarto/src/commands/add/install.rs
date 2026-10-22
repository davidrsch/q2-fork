//! Writing an [`AddPlan`] into the project (bd-rfz4e3bw).
//!
//! The one place `q2 add` changes the user's files, and the only
//! writer in `q2 create`/`use`/`add` that **replaces** existing files.
//! It is deliberately separate from `common::writer`, whose contract —
//! existing files are never overwritten — `q2 create` and `q2 use`
//! rely on.
//!
//! Per extension, following Q1's `completeInstallation`
//! (`install.ts:556`):
//!
//! 1. copy the staged extension into a staging directory *inside*
//!    `<root>/_extensions/` (same volume, so the final step is a
//!    rename, not a copy that can fail halfway);
//! 2. write `.quarto-source.yml` into the staged copy;
//! 3. remove the installed copy, if any — after checking that the path
//!    really is a child of `<root>/_extensions/`;
//! 4. rename the staged copy into place.
//!
//! The staging directory is removed on every exit path.

use std::path::{Path, PathBuf};

use crate::commands::common::plan::CommandFailure;

use super::record::{RECORD_FILENAME, RECORD_VERSION, SourceRecord, hash_tree, list_files};
use super::resolve::{AddPlan, PlannedExtension};
use super::stage::EXTENSIONS_DIR;

fn io_failure(what: &str, path: &Path, e: std::io::Error) -> CommandFailure {
    CommandFailure::new(
        format!("Failed to {what}"),
        format!("{}: {e}", path.display()),
    )
}

/// Install every extension in `plan` that `writes`. A dry-run plan
/// writes nothing.
pub fn install(plan: &AddPlan) -> Result<(), CommandFailure> {
    if plan.dry_run || !plan.writes_anything() {
        return Ok(());
    }
    let extensions_dir = plan.root.join(EXTENSIONS_DIR);
    std::fs::create_dir_all(&extensions_dir)
        .map_err(|e| io_failure("create directory", &extensions_dir, e))?;

    // `tempdir_in` names it `.q2-staging-<random>` and deletes it on
    // drop, so an early `?` return still cleans up.
    let staging = tempfile::Builder::new()
        .prefix(".q2-staging-")
        .tempdir_in(&extensions_dir)
        .map_err(|e| io_failure("create a staging directory in", &extensions_dir, e))?;

    for (i, ext) in plan.extensions.iter().filter(|e| e.writes).enumerate() {
        let staged = staging.path().join(i.to_string());
        copy_tree(&ext.source_dir, &staged)?;
        write_record(plan, &staged)?;
        replace(&extensions_dir, ext, &plan.root, &staged)?;
    }
    Ok(())
}

/// Copy a staged extension, leaving out any `.quarto-source.yml` the
/// source shipped: the record describes *this* install, and one
/// carried in from elsewhere would vouch for files it never saw.
fn copy_tree(from: &Path, to: &Path) -> Result<(), CommandFailure> {
    for rel in list_files(from)? {
        if rel == Path::new(RECORD_FILENAME) {
            continue;
        }
        let dest = to.join(&rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| io_failure("create directory", parent, e))?;
        }
        let src = from.join(&rel);
        std::fs::copy(&src, &dest)
            .map_err(|e| io_failure(&format!("copy {} to", src.display()), &dest, e))?;
    }
    Ok(())
}

fn write_record(plan: &AddPlan, staged: &Path) -> Result<(), CommandFailure> {
    let record = SourceRecord {
        version: RECORD_VERSION,
        source: plan.provenance.source.clone(),
        kind: plan.provenance.kind,
        git_ref: plan.provenance.git_ref.clone(),
        files: hash_tree(staged)?,
    };
    let path = staged.join(RECORD_FILENAME);
    std::fs::write(&path, record.to_yaml()?).map_err(|e| io_failure("write", &path, e))
}

/// Swap the staged copy in for whatever is at the destination.
fn replace(
    extensions_dir: &Path,
    ext: &PlannedExtension,
    root: &Path,
    staged: &Path,
) -> Result<(), CommandFailure> {
    let dest = root.join(&ext.dest);
    let parent = dest
        .parent()
        .ok_or_else(|| internal(&dest, "has no parent directory"))?;
    std::fs::create_dir_all(parent).map_err(|e| io_failure("create directory", parent, e))?;
    assert_inside(extensions_dir, &dest)?;

    // `symlink_metadata` so a symlinked install is seen as the link:
    // `remove_dir_all` removes a link without following it, and that
    // is all we want to remove.
    if std::fs::symlink_metadata(&dest).is_ok() {
        remove_installed(&dest)?;
    }
    std::fs::rename(staged, &dest).map_err(|e| io_failure("move the extension into", &dest, e))
}

/// Check that `dest` is a strict descendant of `extensions_dir`, after
/// resolving symlinks in its parent (both sides canonicalized the same
/// way, so a Windows `\\?\` prefix appears on both or neither). The
/// destination's components were
/// validated as plain names during resolution; this is the last line of
/// defence before a recursive delete, so it checks again from the
/// filesystem's point of view rather than trusting the string.
fn assert_inside(extensions_dir: &Path, dest: &Path) -> Result<(), CommandFailure> {
    let canonical_root = std::fs::canonicalize(extensions_dir)
        .map_err(|e| io_failure("resolve", extensions_dir, e))?;
    let parent = dest
        .parent()
        .ok_or_else(|| internal(dest, "has no parent directory"))?;
    let canonical_parent =
        std::fs::canonicalize(parent).map_err(|e| io_failure("resolve", parent, e))?;
    let name_ok = dest
        .file_name()
        .is_some_and(|n| n != "." && n != ".." && !n.is_empty());
    if name_ok && canonical_parent.starts_with(&canonical_root) {
        Ok(())
    } else {
        Err(internal(
            dest,
            &format!("is not inside {}", extensions_dir.display()),
        ))
    }
}

fn remove_installed(dest: &Path) -> Result<(), CommandFailure> {
    match std::fs::remove_dir_all(dest) {
        Ok(()) => Ok(()),
        Err(first) => {
            // Windows refuses to delete read-only files, and extension
            // archives sometimes carry that bit. Clear it and retry once.
            clear_readonly(dest);
            std::fs::remove_dir_all(dest)
                .map_err(|_| io_failure("remove the installed extension at", dest, first))
        }
    }
}

#[allow(clippy::permissions_set_readonly_false)]
fn clear_readonly(dir: &Path) {
    let Ok(files) = list_files(dir) else {
        return;
    };
    for rel in files {
        let path: PathBuf = dir.join(rel);
        if let Ok(meta) = std::fs::metadata(&path) {
            let mut perms = meta.permissions();
            if perms.readonly() {
                perms.set_readonly(false);
                let _ = std::fs::set_permissions(&path, perms);
            }
        }
    }
}

fn internal(path: &Path, problem: &str) -> CommandFailure {
    CommandFailure::new(
        "Refusing to install",
        format!("{} {problem}. Nothing was removed.", path.display()),
    )
}
