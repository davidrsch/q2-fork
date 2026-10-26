//! `cargo xtask build-agents-docs` — stage the rendered docs site for
//! embedding into the `q2` binary (bd-hwop1zii, bd-x248xpyh).
//!
//! The staged tree feeds both `q2 docs llms` (the llms.txt artifacts)
//! and `q2 docs serve` (the whole HTML site). Steps:
//!
//! 1. Stage the embedded example projects into `docs/examples/` (the
//!    `stage-doc-examples` xtask). `docs/_quarto.yml` declares that
//!    gitignored tree as a project resource and `q2 render` fails hard on
//!    a missing declared resource, so on a fresh checkout (the release
//!    workflow) the docs render cannot even start without this.
//! 2. Clear `docs/_site` and render `docs/` in place with `q2` (built
//!    once up front and shared with step 1 — see `util::build_q2`),
//!    exactly like CI's docs build. `q2 render` does not remove output
//!    whose source is gone, so without the clear a dev checkout would
//!    embed stale pages. The render's llms-txt pass writes
//!    `docs/_site/llms.txt`, `docs/_site/llms-full.txt`, one `.md`
//!    companion per page, and the ledger
//!    `docs/.quarto/llms-manifest.json`.
//! 3. Copy the whole `_site` into a fresh `agents-docs-dist/site/`, and
//!    the ledger to `agents-docs-dist/llms-manifest.json`: `q2 docs llms`
//!    serves only the ledger-listed files (a user-authored resource `.md`
//!    in `_site/` is not an llms companion).
//! 4. Check the staged site needs no network access for its content
//!    (`docs_offline_check`); a remote subresource fails the staging.
//! 5. Write `agents-docs-dist/embed-info.json` recording the staging
//!    checkout's git commit + dirty flag, for `q2 docs llms
//!    --embed-info`.
//!
//! The next `cargo build --bin q2` archives and embeds the staged tree
//! via the `quarto` crate's `build.rs`. Like the preview SPA and the MCP
//! bundle, a plain rebuild does NOT re-render the docs — rerun this
//! xtask to refresh the embed.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// `release` picks the profile of the `q2` that renders (`build-all
/// --release` passes it so the release binary it just built is reused).
pub fn run(release: bool) -> Result<()> {
    let root = find_project_root()?;

    // Capture provenance BEFORE rendering: the render writes into
    // `docs/_site` and `docs/.quarto` and can leave generated siblings
    // (e.g. `*_files/`) in the source tree, so a post-render `git
    // status` would report a clean checkout as dirty. What we want to
    // record is the state of the sources the docs were built from.
    let info = embed_info(&root)?;

    // `docs/_quarto.yml` declares the gitignored `docs/examples/` tree as
    // a project resource, and `q2 render` refuses to run when a declared
    // resource is missing. Stage it first so this works on a fresh
    // checkout (the release workflow's `web-payloads` job), not only on a
    // dev machine that happened to run `stage-doc-examples` at some point.
    // Ordered after `embed_info` on purpose: staging renders the example
    // projects in place, which would otherwise make the provenance read
    // as dirty.
    // Build q2 once; every render below runs the binary directly instead of
    // a nested `cargo run` per render (bd-7xlph6ki). A stale or
    // placeholder-embed binary is fine — the renders don't consume the
    // embed.
    let q2 = crate::util::build_q2(&root, release)?;

    println!("━━━ Staging docs/examples/ (embedded example projects) ━━━");
    crate::stage_doc_examples::stage_all(&root, &q2)?;

    println!("━━━ Rendering docs/ ━━━");
    let site_dir = root.join("docs").join("_site");
    if site_dir.exists() {
        std::fs::remove_dir_all(&site_dir)
            .with_context(|| format!("clearing stale {}", site_dir.display()))?;
    }
    render_docs(&root, &q2)?;

    println!("━━━ Staging agents-docs-dist/ ━━━");
    let manifest_path = root.join("docs").join(".quarto").join("llms-manifest.json");
    let manifest_json = std::fs::read_to_string(&manifest_path).with_context(|| {
        format!(
            "reading {} — did the docs render run its llms-txt pass? \
             (docs/_quarto.yml must set `website.llms-txt: true`)",
            manifest_path.display()
        )
    })?;
    let dist_dir = root.join("agents-docs-dist");
    let staged = stage_site(&manifest_json, &site_dir, &dist_dir)?;

    println!("━━━ Checking the staged site works offline ━━━");
    let remote = crate::docs_offline_check::scan_site(&dist_dir.join("site"))?;
    if !remote.is_empty() {
        let list: String = remote
            .iter()
            .map(|r| format!("\n  {}: {}", r.file, r.url))
            .collect();
        bail!(
            "the docs site loads {} remote subresource(s), so `q2 docs serve` \
             would need network access to show it:{list}\n\
             Vendor the resource into the site, or — only if the page is fine \
             without it — add it to ALLOWED_PREFIXES in \
             crates/xtask/src/docs_offline_check.rs with a reason.",
            remote.len()
        );
    }

    std::fs::write(dist_dir.join("embed-info.json"), &info)
        .with_context(|| format!("writing {}/embed-info.json", dist_dir.display()))?;

    println!(
        "✓ agents-docs-dist/ staged: {staged} site files, embed-info {}",
        info.trim()
    );
    println!("  Rebuild to embed: cargo build --bin q2");
    Ok(())
}

/// Render the docs website in place (`docs/_site`), the same render CI
/// runs, with the already-built `q2` binary.
fn render_docs(root: &Path, q2: &Path) -> Result<()> {
    let status = std::process::Command::new(q2)
        .current_dir(root)
        .args(["render", "docs"])
        .status()
        .with_context(|| format!("spawning `{} render docs`", q2.display()))?;
    if !status.success() {
        bail!("`q2 render docs` failed (exit {:?})", status.code());
    }
    Ok(())
}

/// Stage the rendered site for embedding: `site_dir` is copied whole
/// into a fresh `dist_dir/site/`, and the llms ledger (`manifest_json`,
/// verbatim) to `dist_dir/llms-manifest.json`. Returns the number of
/// site files staged. The dist is rebuilt from scratch so files removed
/// from the docs can never linger in the embed. Dotfiles (`.DS_Store`
/// and friends) are skipped: they are never part of the site.
fn stage_site(manifest_json: &str, site_dir: &Path, dist_dir: &Path) -> Result<usize> {
    #[derive(serde::Deserialize)]
    struct LlmsManifest {
        generated: Vec<String>,
    }

    let manifest: LlmsManifest =
        serde_json::from_str(manifest_json).context("parsing llms-manifest.json")?;
    if manifest.generated.is_empty() {
        bail!("llms-manifest.json lists no generated artifacts");
    }
    if !manifest.generated.iter().any(|rel| rel == "llms.txt") {
        bail!("llms-manifest.json does not list llms.txt; refusing to stage");
    }
    for rel in &manifest.generated {
        // Ledger paths are output-dir-relative with forward slashes.
        let src = site_dir.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        if !src.is_file() {
            bail!(
                "the llms ledger lists {} but the render did not produce it",
                src.display()
            );
        }
    }

    if dist_dir.exists() {
        std::fs::remove_dir_all(dist_dir)
            .with_context(|| format!("clearing stale {}", dist_dir.display()))?;
    }
    let dist_site = dist_dir.join("site");
    std::fs::create_dir_all(&dist_site)
        .with_context(|| format!("creating {}", dist_site.display()))?;

    let mut staged = 0;
    let walk = walkdir::WalkDir::new(site_dir)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !e.file_name().to_string_lossy().starts_with('.'));
    for entry in walk {
        let entry = entry.with_context(|| format!("walking {}", site_dir.display()))?;
        let rel = entry.path().strip_prefix(site_dir)?;
        let dest = dist_site.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&dest)
                .with_context(|| format!("creating {}", dest.display()))?;
        } else if entry.file_type().is_file() {
            std::fs::copy(entry.path(), &dest)
                .with_context(|| format!("copying {}", entry.path().display()))?;
            staged += 1;
        }
    }

    std::fs::write(dist_dir.join("llms-manifest.json"), manifest_json)
        .with_context(|| format!("writing {}/llms-manifest.json", dist_dir.display()))?;
    Ok(staged)
}

/// Provenance sidecar for `q2 docs llms --embed-info`: the staging
/// checkout's HEAD commit and whether the working tree was dirty.
fn embed_info(root: &Path) -> Result<String> {
    let commit = git_stdout(root, &["rev-parse", "HEAD"])?;
    let dirty = !git_stdout(root, &["status", "--porcelain"])?.is_empty();
    Ok(format!(
        "{}\n",
        serde_json::json!({ "commit": commit, "dirty": dirty })
    ))
}

fn git_stdout(root: &Path, args: &[&str]) -> Result<String> {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .with_context(|| format!("running git {args:?}"))?;
    if !out.status.success() {
        bail!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn find_project_root() -> Result<PathBuf> {
    let mut dir = std::env::current_dir().context("Failed to get current directory")?;
    loop {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists() {
            let content = std::fs::read_to_string(&cargo_toml)?;
            if content.contains("[workspace]") {
                return Ok(dir);
            }
        }
        if !dir.pop() {
            bail!("Could not find workspace root (Cargo.toml with [workspace])");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::stage_site;

    fn manifest(entries: &[&str]) -> String {
        serde_json::json!({ "version": 1, "generated": entries }).to_string()
    }

    #[test]
    fn stages_whole_site_and_ledger_clearing_stale() {
        let tmp = tempfile::TempDir::new().unwrap();
        let site = tmp.path().join("_site");
        let dist = tmp.path().join("dist");
        std::fs::create_dir_all(site.join("guides")).unwrap();
        std::fs::create_dir_all(site.join("site_libs")).unwrap();
        std::fs::write(site.join("llms.txt"), "# Index\n").unwrap();
        std::fs::write(site.join("llms-full.txt"), "full\n").unwrap();
        std::fs::write(site.join("index.html"), "<html></html>").unwrap();
        std::fs::write(site.join("guides").join("a.md"), "# A\n").unwrap();
        std::fs::write(site.join("guides").join("a.html"), "<p>A</p>").unwrap();
        std::fs::write(site.join("site_libs").join("q.js"), "x").unwrap();
        // A user-resource .md is part of the site (staged), just not
        // ledger-listed; dotfiles are not part of the site at all.
        std::fs::write(site.join("guides").join("stray.md"), "stray\n").unwrap();
        std::fs::write(site.join(".DS_Store"), "junk").unwrap();
        // A stale file from a previous staging must be cleared.
        std::fs::create_dir_all(&dist).unwrap();
        std::fs::write(dist.join("removed-page.md"), "old\n").unwrap();

        let ledger = manifest(&["guides/a.md", "llms.txt", "llms-full.txt"]);
        let n = stage_site(&ledger, &site, &dist).unwrap();

        assert_eq!(n, 7);
        let staged = dist.join("site");
        assert_eq!(
            std::fs::read_to_string(staged.join("llms.txt")).unwrap(),
            "# Index\n"
        );
        assert!(staged.join("guides").join("a.html").is_file());
        assert!(staged.join("guides").join("stray.md").is_file());
        assert!(staged.join("site_libs").join("q.js").is_file());
        assert!(!staged.join(".DS_Store").exists(), "dotfiles are skipped");
        assert_eq!(
            std::fs::read_to_string(dist.join("llms-manifest.json")).unwrap(),
            ledger,
            "the ledger is staged verbatim"
        );
        assert!(
            !dist.join("removed-page.md").exists(),
            "stale staged files must be cleared"
        );
    }

    #[test]
    fn refuses_manifest_without_llms_txt() {
        let tmp = tempfile::TempDir::new().unwrap();
        let err = stage_site(
            &manifest(&["guides/a.md"]),
            &tmp.path().join("_site"),
            &tmp.path().join("dist"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("llms.txt"), "{err}");
    }

    #[test]
    fn missing_listed_file_is_an_error_naming_the_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let site = tmp.path().join("_site");
        std::fs::create_dir_all(&site).unwrap();
        std::fs::write(site.join("llms.txt"), "# Index\n").unwrap();
        let err = stage_site(
            &manifest(&["llms.txt", "ghost.md"]),
            &site,
            &tmp.path().join("dist"),
        )
        .unwrap_err();
        assert!(format!("{err:#}").contains("ghost.md"), "{err:#}");
    }
}
