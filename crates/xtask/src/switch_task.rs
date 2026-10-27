//! Switch-task command — in-place sub-task transition without spinning a
//! fresh worktree.
//!
//! Companion to `cargo xtask create-worktree`. Where `create-worktree`
//! is the right answer for *parallel* / *investigation* work (you want
//! isolation, you'll pay the npm install + cargo cold-cache cost
//! deliberately), `switch-task` is the right answer for *sequential*
//! implementation work in an epic: most sub-tasks branch off the same
//! integration line, do not need a separate worktree, and benefit from
//! keeping `node_modules/` and `target/` warm across branch switches.
//!
//! Typical use, mid-epic:
//!
//!   # in some worktree (could be the main repo or any .worktrees/*)
//!   cargo xtask switch-task bd-yxqt --from feature/q2-preview-command
//!
//! That:
//!   1. fetches `origin/feature/q2-preview-command` and takes the newer
//!      of it and the local branch (see `resolve_start_point`), so it
//!      picks up siblings' merges even when the integration branch is
//!      checked out in another worktree and cannot be pulled here,
//!   2. creates a new topic branch `braid/<id>-<slug>` off that tip,
//!   3. marks the braid strand `in_progress`,
//!   4. rewrites the `<!-- BEGIN/END WORKTREE CONTEXT -->` block in
//!      `CLAUDE.local.md` so the next Claude Code session knows what
//!      it's working on.
//!
//! Omit `--from` to branch off the current HEAD without changing
//! branches first. `--no-claim` skips the braid status update.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::create_worktree::{
    Checkout, IssueMetadata, SectionKind, build_section, derive_slug, fetch_issue_metadata,
    parse_external_ref_to_github_url, strand_branch, update_claude_local_md, validate_slug,
};

/// Find the *current worktree's* root via `git rev-parse --show-toplevel`.
///
/// This is the root for any command that works on the files of the
/// checkout it was invoked from. It deliberately differs from
/// [`crate::create_worktree::main_checkout_root`], which resolves the
/// *main* checkout via `--git-common-dir`: from a linked worktree that
/// would read and write another branch's files (bd-cvk7jb11).
pub(crate) fn current_worktree_root() -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("spawning `git rev-parse --show-toplevel`")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("`git rev-parse --show-toplevel` failed:\n{stderr}");
    }
    let path = std::str::from_utf8(&output.stdout)
        .context("toplevel path is not valid UTF-8")?
        .trim();
    Ok(PathBuf::from(path))
}

/// Whether `dir` is inside the main checkout or a linked worktree: only
/// in the main checkout does git's per-worktree dir equal the common dir.
fn detect_checkout(dir: &Path) -> Result<Checkout> {
    let output = Command::new("git")
        .args([
            "rev-parse",
            "--path-format=absolute",
            "--git-dir",
            "--git-common-dir",
        ])
        .current_dir(dir)
        .output()
        .context("spawning `git rev-parse --git-dir --git-common-dir`")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("`git rev-parse --git-dir --git-common-dir` failed:\n{stderr}");
    }
    let stdout = std::str::from_utf8(&output.stdout).context("git dir path is not valid UTF-8")?;
    let mut lines = stdout.lines();
    let (Some(git_dir), Some(common_dir)) = (lines.next(), lines.next()) else {
        bail!("unexpected `git rev-parse --git-dir --git-common-dir` output:\n{stdout}");
    };
    Ok(if git_dir == common_dir {
        Checkout::Main
    } else {
        Checkout::Worktree
    })
}

/// Arguments for `cargo xtask switch-task`.
pub struct Args {
    /// Braid strand ID to switch to (e.g. `bd-yxqt`).
    pub beads_id: String,
    /// Integration branch to switch+pull to before creating the topic
    /// branch. Omit to branch off the current HEAD.
    pub from: Option<String>,
    /// Optional slug override (kebab-case). Default: derived from the
    /// strand title via `derive_slug`.
    pub slug: Option<String>,
    /// Don't mark the strand `in_progress` in braid.
    pub no_claim: bool,
}

pub fn run(args: Args) -> Result<()> {
    let root = current_worktree_root()?;

    let meta = fetch_issue_metadata(&args.beads_id)?;
    let slug = match args.slug.as_deref() {
        Some(s) => {
            validate_slug(s)?;
            s.to_string()
        }
        None => derive_slug(&meta.title)?,
    };
    let branch = strand_branch(&format!("{}-{}", args.beads_id, slug));

    let start = match args.from.as_deref() {
        Some(from) => {
            // Best-effort fetch so origin/<from> reflects the latest tip.
            // Failure is non-fatal — the user might be offline or `<from>`
            // might be a local-only branch.
            git_fetch_origin(from);
            let start = resolve_start_point(&root, from)?;
            git_switch_create_from(&branch, &start)?;
            Some(start)
        }
        None => {
            git_switch_create(&branch)?;
            None
        }
    };

    if !args.no_claim {
        claim_issue(&args.beads_id)?;
    }

    update_worktree_context(&root, &args.beads_id, &meta)?;

    print_summary(
        &args.beads_id,
        &branch,
        args.from.as_deref(),
        start.as_deref(),
    );
    Ok(())
}

/// The ref to branch `--from <from>` off: whichever of the local branch
/// and `origin/<from>` is newer.
///
/// The local `<from>` cannot simply be pulled first: it is usually checked
/// out in another worktree (`main` in the main checkout, say), so it is
/// often stale. Branching off it silently started topic branches from an
/// old tip (bd-cvk7jb11). Instead:
///
/// - no `origin/<from>` (local-only branch, or the fetch failed before it
///   ever existed) → the local branch;
/// - no local branch, or local is an ancestor of `origin/<from>` (behind or
///   equal) → `origin/<from>`;
/// - `origin/<from>` is an ancestor of local (local has unpushed merges,
///   the integration-line convention's normal state between merge and
///   push) → the local branch;
/// - diverged → error, so the user reconciles the two first.
fn resolve_start_point(dir: &Path, from: &str) -> Result<String> {
    let remote = format!("origin/{from}");
    let has_ref = |r: &str| {
        Command::new("git")
            .args([
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{r}^{{commit}}"),
            ])
            .current_dir(dir)
            .output()
            .is_ok_and(|o| o.status.success())
    };
    let is_ancestor = |a: &str, b: &str| {
        Command::new("git")
            .args(["merge-base", "--is-ancestor", a, b])
            .current_dir(dir)
            .status()
            .is_ok_and(|s| s.success())
    };
    let has_local = has_ref(&format!("refs/heads/{from}"));
    if !has_ref(&format!("refs/remotes/{remote}")) {
        return Ok(from.to_string());
    }
    if !has_local || is_ancestor(from, &remote) {
        return Ok(remote);
    }
    if is_ancestor(&remote, from) {
        return Ok(from.to_string());
    }
    bail!(
        "local `{from}` and `{remote}` have diverged; reconcile them, or pass \
         `--from {remote}` or `--from {from}` to choose one explicitly"
    )
}

fn git_fetch_origin(branch: &str) {
    eprintln!("→ git fetch origin {branch} (best-effort)");
    let _ = Command::new("git")
        .args(["fetch", "origin", branch])
        .status();
}

/// Create `<branch>` at the tip of `<start_point>` and check it out.
/// Uses `git switch -c` with a start-point, which doesn't require
/// `<start_point>` to be checked out in this worktree — so this is
/// safe when the integration branch is already checked out in another
/// worktree (the common case for the main repo + `.worktrees/*` setup).
fn git_switch_create_from(branch: &str, start_point: &str) -> Result<()> {
    eprintln!("→ git switch -c {branch} {start_point}");
    let status = Command::new("git")
        // `--no-track`: a remote-tracking start point (`origin/<from>`)
        // would otherwise make the topic branch track the integration
        // branch, and a bare `git push` would aim at it.
        .args(["switch", "--no-track", "-c", branch, start_point])
        .status()
        .context("spawning `git switch -c <branch> <start>`")?;
    if !status.success() {
        bail!(
            "`git switch -c {branch} {start_point}` failed — branch may \
             already exist; rename or delete it first"
        );
    }
    Ok(())
}

fn git_switch_create(branch: &str) -> Result<()> {
    eprintln!("→ git switch -c {branch}");
    let status = Command::new("git")
        .args(["switch", "-c", branch])
        .status()
        .context("spawning `git switch -c`")?;
    if !status.success() {
        bail!(
            "`git switch -c {branch}` failed — branch may already exist; \
             rename or delete it first"
        );
    }
    Ok(())
}

fn claim_issue(id: &str) -> Result<()> {
    eprintln!("→ braid update {id} --status in_progress");
    let status = Command::new("braid")
        .args(["update", id, "--status", "in_progress"])
        .status()
        .context("spawning `braid update`")?;
    if !status.success() {
        bail!("`braid update {id} --status in_progress` failed");
    }
    Ok(())
}

fn update_worktree_context(root: &Path, id: &str, meta: &IssueMetadata) -> Result<()> {
    let github_url = parse_external_ref_to_github_url(meta.external_ref.as_deref());
    let kind = SectionKind::Braid {
        id: id.to_string(),
        title: meta.title.clone(),
        github_url,
    };
    let section = build_section(&kind, detect_checkout(root)?);
    let path = root.join("CLAUDE.local.md");
    update_claude_local_md(&path, &section)?;
    eprintln!("→ refreshed worktree context in {}", path.display());
    Ok(())
}

fn print_summary(id: &str, branch: &str, from: Option<&str>, start: Option<&str>) {
    println!();
    println!("✓ Switched to {id} on branch {branch}");
    if let Some(start) = start {
        println!("  (branched off {start})");
    }
    println!("  braid strand marked in_progress");
    println!();
    println!("Next: implement, commit, then promote with:");
    println!("  git switch {}", from.unwrap_or("<epic-branch>"));
    println!("  git merge --no-ff {branch}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(dir)
            .status()
            .expect("spawn git");
        assert!(status.success(), "git {args:?} failed in {}", dir.display());
    }

    #[test]
    fn detect_checkout_distinguishes_main_from_linked_worktree() {
        let tmp = TempDir::new().unwrap();
        let main = tmp.path().join("main");
        std::fs::create_dir(&main).unwrap();
        git(&main, &["init", "-q"]);
        git(&main, &["commit", "-q", "--allow-empty", "-m", "init"]);
        git(&main, &["worktree", "add", "-q", "--detach", "../linked"]);
        let linked = tmp.path().join("linked");

        for (root, expected) in [(&main, Checkout::Main), (&linked, Checkout::Worktree)] {
            assert_eq!(
                detect_checkout(root).unwrap(),
                expected,
                "{}",
                root.display()
            );
            let sub = root.join("sub");
            std::fs::create_dir(&sub).unwrap();
            assert_eq!(
                detect_checkout(&sub).unwrap(),
                expected,
                "{}",
                sub.display()
            );
        }
    }

    /// `git rev-parse <rev>` in `dir`.
    fn rev(dir: &Path, rev: &str) -> String {
        let out = Command::new("git")
            .args(["rev-parse", rev])
            .current_dir(dir)
            .output()
            .expect("spawn git");
        assert!(out.status.success(), "git rev-parse {rev} failed");
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    /// A repo whose `main` and fake `origin/main` (a plain ref under
    /// refs/remotes, no actual remote needed) start at the same commit.
    fn repo_with_origin_main() -> TempDir {
        let tmp = TempDir::new().unwrap();
        let d = tmp.path();
        git(d, &["init", "-q", "-b", "main"]);
        git(d, &["commit", "-q", "--allow-empty", "-m", "base"]);
        git(d, &["update-ref", "refs/remotes/origin/main", "main"]);
        tmp
    }

    /// Commit on top of `rev` without moving any branch; returns the sha.
    fn commit_on(d: &Path, rev: &str, msg: &str) -> String {
        let out = Command::new("git")
            .args([
                "commit-tree",
                &format!("{rev}^{{tree}}"),
                "-p",
                rev,
                "-m",
                msg,
            ])
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .current_dir(d)
            .output()
            .expect("spawn git");
        assert!(out.status.success(), "commit-tree failed: {out:?}");
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    #[test]
    fn resolve_start_point_prefers_origin_when_local_is_behind() {
        // The bd-cvk7jb11 case: `main` is checked out (and stale) in
        // another worktree, origin has moved on.
        let tmp = repo_with_origin_main();
        let d = tmp.path();
        let newer = commit_on(d, "origin/main", "sibling merge");
        git(d, &["update-ref", "refs/remotes/origin/main", &newer]);
        assert_eq!(resolve_start_point(d, "main").unwrap(), "origin/main");
    }

    #[test]
    fn resolve_start_point_prefers_origin_when_equal() {
        let tmp = repo_with_origin_main();
        assert_eq!(
            resolve_start_point(tmp.path(), "main").unwrap(),
            "origin/main"
        );
    }

    #[test]
    fn resolve_start_point_keeps_local_when_ahead() {
        // Merged a sub-task into the integration branch, not pushed yet.
        let tmp = repo_with_origin_main();
        let d = tmp.path();
        git(
            d,
            &["commit", "-q", "--allow-empty", "-m", "unpushed merge"],
        );
        assert_eq!(resolve_start_point(d, "main").unwrap(), "main");
    }

    #[test]
    fn resolve_start_point_refuses_diverged() {
        let tmp = repo_with_origin_main();
        let d = tmp.path();
        let theirs = commit_on(d, "origin/main", "remote only");
        git(d, &["update-ref", "refs/remotes/origin/main", &theirs]);
        git(d, &["commit", "-q", "--allow-empty", "-m", "local only"]);
        let err = resolve_start_point(d, "main").unwrap_err().to_string();
        assert!(err.contains("diverged"), "{err}");
    }

    #[test]
    fn resolve_start_point_without_remote_or_local_branch() {
        let tmp = repo_with_origin_main();
        let d = tmp.path();
        // Local-only branch: no origin/<from>.
        git(d, &["branch", "local-only"]);
        assert_eq!(resolve_start_point(d, "local-only").unwrap(), "local-only");
        // Remote-only branch: no local <from>.
        git(
            d,
            &["update-ref", "refs/remotes/origin/remote-only", "main"],
        );
        assert_eq!(
            resolve_start_point(d, "remote-only").unwrap(),
            "origin/remote-only"
        );
        assert_eq!(rev(d, "origin/remote-only"), rev(d, "main"));
    }

    #[test]
    fn current_worktree_root_is_the_linked_worktree_not_the_main_checkout() {
        // bd-cvk7jb11: stage-doc-examples, check-noop-rebuild,
        // gen-math-spec and braid-snapshot resolved their root with
        // main_checkout_root(), so from a linked worktree they read and
        // wrote the main checkout's files. They now use
        // current_worktree_root(); this pins the difference.
        let tmp = TempDir::new().unwrap();
        let main = tmp.path().join("main");
        std::fs::create_dir(&main).unwrap();
        git(&main, &["init", "-q"]);
        git(&main, &["commit", "-q", "--allow-empty", "-m", "init"]);
        git(&main, &["worktree", "add", "-q", "--detach", "../linked"]);
        let linked = tmp.path().join("linked");

        // Each nextest test runs in its own process, so chdir here is
        // safe (no cross-test race).
        let prev_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&linked).unwrap();
        let here = current_worktree_root().unwrap();
        let main_root = crate::create_worktree::main_checkout_root().unwrap();
        std::env::set_current_dir(prev_dir).unwrap();

        // Canonicalize: macOS /tmp is a symlink.
        let canon = |p: &Path| p.canonicalize().unwrap();
        assert_eq!(canon(&here), canon(&linked));
        assert_eq!(canon(&main_root), canon(&main));
    }
}
