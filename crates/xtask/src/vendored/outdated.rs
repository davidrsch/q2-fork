//! `cargo xtask vendored outdated` — ask each vendored asset's upstream what
//! is newer (bd-xm7l).
//!
//! Reads `resources/vendored.toml`, queries every `track` asset's upstream,
//! and reports one row per asset, with its `follow` assets listed beside it.
//! This is the only network-touching piece of the vendored tooling; the
//! offline checks live in the `vendored-registry` lint.
//!
//! Upstream queries shell out to tools every dev machine already has, so
//! xtask needs no HTTP client: `curl` for the npm registry, Typst Universe and
//! crates.io, `git ls-remote` for git refs. For a `git` upstream with
//! `paths`, "outdated" means *those paths changed* between the pinned ref and
//! the latest one; that needs history, so the repo is kept as a blobless bare
//! clone under `target/vendored-cache/` (never `external-sources/`).
//!
//! A failed query is reported on its row and never aborts the run.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use super::registry::{Asset, GitTrack, Policy, Registry, Upstream, VersionSource};
use crate::pull_extension_subtree::find_last_split;

const CRATES_IO_USER_AGENT: &str = "q2-xtask (https://github.com/quarto-dev/q2)";

/// CLI arguments for `vendored outdated`.
pub struct Args {
    pub json: bool,
    pub id: Option<String>,
    pub group: Option<String>,
}

/// The verdict for one asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// Upstream has nothing newer (or nothing newer under the copied paths).
    Current,
    /// Upstream moved.
    Outdated,
    /// The registry records no version; upstream's latest is shown.
    Unrecorded,
    /// Deliberately unpinned (`version = "*"`); upstream's latest is shown.
    Unpinned,
    /// The pinned ref could not be resolved upstream; compare by hand.
    Manual,
    /// The query failed.
    Error,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub id: String,
    pub title: String,
    pub group: Option<String>,
    pub current: Option<String>,
    pub latest: Option<String>,
    pub status: Status,
    /// Supporting lines: upstream commits under the copied paths, warnings.
    pub detail: Vec<String>,
    /// `follow` assets whose version this one decides.
    pub followers: Vec<String>,
    pub procedure: Option<String>,
    pub guards: Vec<String>,
    pub license: Option<String>,
    /// The registry's free-form upgrade context for this asset.
    pub notes: Option<String>,
}

/// The `--json` document: group context plus one report per asset.
#[derive(Debug, Serialize)]
struct JsonOutput<'a> {
    groups: BTreeMap<&'a str, JsonGroup<'a>>,
    assets: &'a [Report],
}

#[derive(Debug, Serialize)]
struct JsonGroup<'a> {
    title: &'a str,
    notes: Option<&'a str>,
}

pub fn run(args: Args) -> Result<()> {
    let root = crate::lint::find_workspace_root()?;
    let registry = Registry::load(&root)?;

    let selected: Vec<&Asset> = registry
        .assets
        .iter()
        .filter(|a| a.policy == Policy::Track)
        .filter(|a| args.id.as_ref().is_none_or(|id| &a.id == id))
        .filter(|a| {
            args.group
                .as_ref()
                .is_none_or(|g| a.group.as_ref() == Some(g))
        })
        .collect();
    if selected.is_empty() {
        bail!("no `track` assets match the filters");
    }

    let cache_dir = root.join("target").join("vendored-cache");
    prepare_caches(&cache_dir, &selected);

    let mut reports: Vec<Report> = std::thread::scope(|s| {
        let handles: Vec<_> = selected
            .iter()
            .map(|asset| {
                let root = &root;
                let cache_dir = &cache_dir;
                s.spawn(move || survey(root, cache_dir, asset))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("survey thread panicked"))
            .collect()
    });

    let followers = followers_by_leader(&registry);
    for r in &mut reports {
        r.followers = followers.get(r.id.as_str()).cloned().unwrap_or_default();
    }
    reports.sort_by(|a, b| (&a.group, &a.id).cmp(&(&b.group, &b.id)));

    if args.json {
        let groups = registry
            .groups
            .iter()
            .map(|g| {
                let group = JsonGroup {
                    title: &g.title,
                    notes: g.notes.as_deref(),
                };
                (g.id.as_str(), group)
            })
            .collect();
        let output = JsonOutput {
            groups,
            assets: &reports,
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_table(&registry, &reports);
        print_untracked_summary(&registry);
    }
    Ok(())
}

/// Assets that `follow` (directly or transitively) each leader.
fn followers_by_leader(registry: &Registry) -> HashMap<&str, Vec<String>> {
    let mut map: HashMap<&str, Vec<String>> = HashMap::new();
    for asset in registry
        .assets
        .iter()
        .filter(|a| a.policy == Policy::Follow)
    {
        let mut leader = asset;
        let mut hops = 0;
        while let Some(next) = leader.follows.as_deref().and_then(|t| registry.asset(t)) {
            leader = next;
            hops += 1;
            if hops > registry.assets.len() {
                break; // cycle; the lint reports it
            }
        }
        map.entry(leader.id.as_str())
            .or_default()
            .push(asset.id.clone());
    }
    map
}

/// Clone or fetch each distinct git repo that needs history, one thread per
/// repo. Failures surface later as per-asset errors.
fn prepare_caches(cache_dir: &Path, assets: &[&Asset]) {
    let mut repos: Vec<&str> = assets
        .iter()
        .filter_map(|a| match &a.upstream {
            Some(Upstream::Git { repo, paths, .. }) if !paths.is_empty() => Some(repo.as_str()),
            _ => None,
        })
        .collect();
    repos.sort_unstable();
    repos.dedup();
    std::thread::scope(|s| {
        for repo in repos {
            s.spawn(move || {
                if let Err(e) = ensure_cache(cache_dir, repo) {
                    eprintln!("warning: could not prepare history cache for {repo}: {e:#}");
                }
            });
        }
    });
}

fn cache_path(cache_dir: &Path, repo: &str) -> PathBuf {
    let slug: String = repo
        .trim_end_matches(".git")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    cache_dir.join(format!("{slug}.git"))
}

fn ensure_cache(cache_dir: &Path, repo: &str) -> Result<()> {
    let path = cache_path(cache_dir, repo);
    if path.exists() {
        git(
            &path,
            &[
                "fetch",
                "--quiet",
                "--force",
                "--prune",
                repo,
                "+refs/heads/*:refs/heads/*",
                "+refs/tags/*:refs/tags/*",
            ],
        )?;
    } else {
        std::fs::create_dir_all(cache_dir)?;
        let status = Command::new("git")
            .args(["clone", "--quiet", "--bare", "--filter=blob:none", repo])
            .arg(&path)
            .status()
            .context("running git clone")?;
        if !status.success() {
            bail!("git clone {repo} failed");
        }
    }
    Ok(())
}

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .with_context(|| format!("running git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn curl(url: &str) -> Result<String> {
    let output = Command::new("curl")
        .args(["-fsSL", "--max-time", "60", "-A", CRATES_IO_USER_AGENT, url])
        .output()
        .context("running curl")?;
    if !output.status.success() {
        bail!(
            "GET {url} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn survey(root: &Path, cache_dir: &Path, asset: &Asset) -> Report {
    let mut report = Report {
        id: asset.id.clone(),
        title: asset.title.clone(),
        group: asset.group.clone(),
        current: asset.version.clone(),
        latest: None,
        status: Status::Error,
        detail: Vec::new(),
        followers: Vec::new(),
        procedure: asset.procedure.clone(),
        guards: asset.guards.clone(),
        license: asset.license.clone(),
        notes: asset.notes.clone(),
    };
    if let Err(e) = survey_into(root, cache_dir, asset, &mut report) {
        report.status = Status::Error;
        report.detail.push(format!("{e:#}"));
    }
    report
}

fn survey_into(root: &Path, cache_dir: &Path, asset: &Asset, report: &mut Report) -> Result<()> {
    let Some(upstream) = &asset.upstream else {
        bail!("no upstream recorded");
    };

    // Subtree pins live in git history; check the registry agrees with it.
    if asset
        .version_sources
        .iter()
        .any(|s| matches!(s, VersionSource::SubtreeSplit))
        && let Some(prefix) = asset.paths.first()
    {
        match find_last_split(root, prefix)? {
            Some(split) if Some(&split) == asset.version.as_ref() => {}
            Some(split) => report.detail.push(format!(
                "registry drift: last git-subtree-split for {prefix} is {split}, registry says {}",
                asset.version.as_deref().unwrap_or("nothing")
            )),
            None => report.detail.push(format!(
                "no git-subtree-split trailer found for {prefix} (shallow clone?)"
            )),
        }
    }

    match upstream {
        Upstream::Npm { package } => {
            let body = curl(&format!("https://registry.npmjs.org/{package}/latest"))?;
            report.latest = Some(parse_npm_latest(&body)?);
        }
        Upstream::TypstUniverse { package } => {
            let body = curl("https://packages.typst.org/preview/index.json")?;
            report.latest = parse_typst_index(&body, package)?;
        }
        Upstream::CratesIo { krate } => {
            let body = curl(&format!("https://crates.io/api/v1/crates/{krate}"))?;
            report.latest = Some(parse_crates_io(&body)?);
        }
        Upstream::Git {
            repo,
            track,
            tag_prefix,
            paths,
        } => {
            let prefix = tag_prefix.as_deref().unwrap_or("");
            let latest = match track {
                GitTrack::Tags => {
                    let out = git(root, &["ls-remote", "--tags", "--refs", repo])?;
                    latest_tag(&out, prefix)
                        .with_context(|| format!("no version-like tags in {repo}"))?
                }
                GitTrack::Branch(branch) => {
                    let out = git(root, &["ls-remote", repo, &format!("refs/heads/{branch}")])?;
                    out.split_whitespace()
                        .next()
                        .with_context(|| format!("branch {branch} not found in {repo}"))?
                        .to_string()
                }
            };
            report.latest = Some(latest.clone());
            if !paths.is_empty() {
                return survey_paths(cache_dir, repo, asset, &latest, paths, report);
            }
            report.status = match asset.version.as_deref() {
                None => Status::Unrecorded,
                Some("*") => Status::Unpinned,
                Some(v) => match track {
                    GitTrack::Tags => {
                        let ours = v.strip_prefix(prefix).unwrap_or(v);
                        let theirs = latest.strip_prefix(prefix).unwrap_or(&latest);
                        version_status(ours, theirs)
                    }
                    GitTrack::Branch(_) if latest.starts_with(v) || v.starts_with(&latest) => {
                        Status::Current
                    }
                    GitTrack::Branch(_) => Status::Outdated,
                },
            };
            return Ok(());
        }
    }

    let latest = report.latest.clone();
    report.status = match (asset.version.as_deref(), latest.as_deref()) {
        (None, _) => Status::Unrecorded,
        (Some("*"), _) => Status::Unpinned,
        (Some(_), None) => bail!("upstream has no release"),
        (Some(ours), Some(theirs)) => version_status(ours, theirs),
    };
    Ok(())
}

/// Path-scoped git survey: did `paths` change between the pinned ref and
/// `latest`?
fn survey_paths(
    cache_dir: &Path,
    repo: &str,
    asset: &Asset,
    latest: &str,
    paths: &[String],
    report: &mut Report,
) -> Result<()> {
    let cache = cache_path(cache_dir, repo);
    if !cache.exists() {
        bail!("no history cache for {repo}");
    }
    let path_args = |mut args: Vec<String>| {
        args.push("--".to_string());
        args.extend(paths.iter().cloned());
        args
    };
    let run = |args: Vec<String>| {
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        git(&cache, &refs)
    };

    let resolvable = asset.version.as_deref().filter(|v| {
        *v != "*"
            && git(
                &cache,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("{v}^{{commit}}"),
                ],
            )
            .is_ok()
    });
    let Some(ours) = resolvable else {
        let last = run(path_args(vec![
            "log".into(),
            "-1".into(),
            "--format=%h %ad %s".into(),
            "--date=short".into(),
            latest.into(),
        ]))?;
        report.status = if asset.version.is_none() {
            Status::Unrecorded
        } else {
            Status::Manual
        };
        report.detail.push(format!(
            "last upstream change under the copied paths: {}",
            last.trim()
        ));
        return Ok(());
    };

    let log = run(path_args(vec![
        "log".into(),
        "--format=%h %ad %s".into(),
        "--date=short".into(),
        format!("{ours}..{latest}"),
    ]))?;
    let commits: Vec<&str> = log.lines().filter(|l| !l.is_empty()).collect();
    report.status = if commits.is_empty() {
        Status::Current
    } else {
        Status::Outdated
    };
    if !commits.is_empty() {
        report.detail.push(format!(
            "{} upstream commit(s) touch the copied paths since {ours}:",
            commits.len()
        ));
        report
            .detail
            .extend(commits.iter().take(15).map(|c| format!("  {c}")));
        if commits.len() > 15 {
            report
                .detail
                .push(format!("  … and {} more", commits.len() - 15));
        }
    }
    Ok(())
}

fn parse_npm_latest(body: &str) -> Result<String> {
    let json: serde_json::Value = serde_json::from_str(body)?;
    json["version"]
        .as_str()
        .map(str::to_string)
        .context("npm response has no `version`")
}

fn parse_crates_io(body: &str) -> Result<String> {
    let json: serde_json::Value = serde_json::from_str(body)?;
    json["crate"]["max_stable_version"]
        .as_str()
        .map(str::to_string)
        .context("crates.io response has no `crate.max_stable_version`")
}

fn parse_typst_index(body: &str, package: &str) -> Result<Option<String>> {
    let json: serde_json::Value = serde_json::from_str(body)?;
    let entries = json.as_array().context("Typst index is not an array")?;
    Ok(entries
        .iter()
        .filter(|e| e["name"].as_str() == Some(package))
        .filter_map(|e| e["version"].as_str())
        .max_by(|a, b| compare_versions(a, b))
        .map(str::to_string))
}

/// The highest version-like tag in `git ls-remote --tags --refs` output,
/// returned with its prefix. Pre-release tags (`-rc1`, …) are skipped.
fn latest_tag(ls_remote: &str, prefix: &str) -> Option<String> {
    ls_remote
        .lines()
        .filter_map(|l| l.split_whitespace().nth(1)?.strip_prefix("refs/tags/"))
        .filter(|t| t.strip_prefix(prefix).is_some_and(is_release_version))
        .max_by(|a, b| compare_versions(&a[prefix.len()..], &b[prefix.len()..]))
        .map(str::to_string)
}

fn is_release_version(s: &str) -> bool {
    !s.is_empty()
        && s.split('.')
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// Compare dotted numeric versions component-wise (`3.11` > `3.9`); missing
/// components count as 0. Non-numeric components compare as 0.
fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> Vec<u64> {
        s.split(['.', '-', '+'])
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    let (pa, pb) = (parse(a), parse(b));
    for i in 0..pa.len().max(pb.len()) {
        let ord = pa.get(i).unwrap_or(&0).cmp(pb.get(i).unwrap_or(&0));
        if ord.is_ne() {
            return ord;
        }
    }
    std::cmp::Ordering::Equal
}

/// `ours` may be a floating major (`3`), which is current for any `3.x`.
fn version_status(ours: &str, theirs: &str) -> Status {
    let ours_parts = ours.split('.').count();
    let theirs_prefix: Vec<&str> = theirs.split('.').take(ours_parts).collect();
    if compare_versions(ours, &theirs_prefix.join(".")).is_ge() {
        Status::Current
    } else {
        Status::Outdated
    }
}

fn print_table(registry: &Registry, reports: &[Report]) {
    let mut by_group: BTreeMap<&str, Vec<&Report>> = BTreeMap::new();
    for r in reports {
        by_group
            .entry(r.group.as_deref().unwrap_or("(ungrouped)"))
            .or_default()
            .push(r);
    }
    let short = |s: &Option<String>| match s.as_deref() {
        None => "(unrecorded)".to_string(),
        Some(v) if v.len() == 40 && v.bytes().all(|b| b.is_ascii_hexdigit()) => v[..10].to_string(),
        Some(v) => v.to_string(),
    };
    for (group, rows) in by_group {
        let title = registry
            .groups
            .iter()
            .find(|g| g.id == group)
            .map_or(String::new(), |g| format!(": {}", g.title));
        println!("\n== {group}{title}");
        for r in rows {
            let status = serde_json::to_value(&r.status)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default();
            println!(
                "  {:<30} {:<12} {:<14} -> {:<14}",
                r.id,
                status,
                short(&r.current),
                short(&r.latest)
            );
            if !r.followers.is_empty() {
                println!("      followers: {}", r.followers.join(", "));
            }
            for d in &r.detail {
                println!("      {d}");
            }
        }
    }
}

fn print_untracked_summary(registry: &Registry) {
    let count = |p: Policy| registry.assets.iter().filter(|a| a.policy == p).count();
    println!(
        "\nNot queried: {} follow (reported with their leader), {} frozen, {} external, {} native.",
        count(Policy::Follow),
        count(Policy::Frozen),
        count(Policy::External),
        count(Policy::Native)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npm_and_crates_io_responses_parse() {
        assert_eq!(
            parse_npm_latest(r#"{"name":"mermaid","version":"12.2.0"}"#).unwrap(),
            "12.2.0"
        );
        assert_eq!(
            parse_crates_io(
                r#"{"crate":{"max_stable_version":"0.4.2","max_version":"0.5.0-rc.1"}}"#
            )
            .unwrap(),
            "0.4.2"
        );
        assert!(parse_npm_latest("{}").is_err());
    }

    #[test]
    fn typst_index_picks_the_highest_version_of_the_named_package() {
        let body = r#"[
            {"name":"orange-book","version":"0.7.1"},
            {"name":"orange-book","version":"0.10.0"},
            {"name":"orange-book","version":"0.9.2"},
            {"name":"showybox","version":"2.0.5"}
        ]"#;
        assert_eq!(
            parse_typst_index(body, "orange-book").unwrap().as_deref(),
            Some("0.10.0")
        );
        assert_eq!(parse_typst_index(body, "nope").unwrap(), None);
    }

    #[test]
    fn latest_tag_respects_prefix_and_skips_prereleases() {
        let out = "\
aaa\trefs/tags/v1.9.10
bbb\trefs/tags/v1.11.3
ccc\trefs/tags/v1.12.0-rc1
ddd\trefs/tags/v1.11.12
eee\trefs/tags/1.99.0
fff\trefs/tags/nightly
";
        assert_eq!(latest_tag(out, "v").as_deref(), Some("v1.11.12"));
        assert_eq!(latest_tag(out, "").as_deref(), Some("1.99.0"));
    }

    #[test]
    fn version_comparison_is_numeric_and_supports_floating_majors() {
        assert_eq!(version_status("3.9", "3.11"), Status::Outdated);
        assert_eq!(version_status("3.11", "3.11"), Status::Current);
        assert_eq!(version_status("3", "3.2.2"), Status::Current);
        assert_eq!(version_status("3", "4.0.0"), Status::Outdated);
        assert_eq!(version_status("0.18.9", "0.18.10"), Status::Outdated);
        assert_eq!(version_status("2.0.0", "1.9.9"), Status::Current);
    }

    #[test]
    fn followers_resolve_to_their_root_leader() {
        let reg = Registry::parse(
            r#"
            [coverage]
            roots = []
            [[asset]]
            id = "leader"
            title = "l"
            paths = ["x"]
            policy = "track"
            upstream = { kind = "npm", package = "x" }
            [[asset]]
            id = "mid"
            title = "m"
            paths = ["y"]
            policy = "follow"
            follows = "leader"
            [[asset]]
            id = "leaf"
            title = "f"
            paths = ["z"]
            policy = "follow"
            follows = "mid"
            "#,
        )
        .unwrap();
        let map = followers_by_leader(&reg);
        let mut f = map["leader"].clone();
        f.sort();
        assert_eq!(f, ["leaf", "mid"]);
    }

    #[test]
    fn cache_paths_are_filesystem_safe() {
        let p = cache_path(
            Path::new("/c"),
            "https://github.com/quarto-dev/quarto-cli.git",
        );
        assert_eq!(
            p,
            Path::new("/c").join("https___github_com_quarto_dev_quarto_cli.git")
        );
    }
}
