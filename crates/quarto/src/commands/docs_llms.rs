//! `q2 docs llms` — serve the embedded docs-site llms.txt artifacts
//! (bd-hwop1zii).
//!
//! The `q2 docs` namespace exposes documentation embedded in the binary.
//! This module implements its `llms` subcommand (aliased as the
//! top-level `q2 agents-info`): the llms.txt index, per-page markdown
//! companions, and llms-full.txt that `q2 render docs/` produces, read
//! from the embedded docs archive ([`super::docs_embed`]). Only the
//! files the llms ledger lists are visible here.
//!
//! All lookup logic operates on an [`EmbedFiles`] parameter so tests
//! can drive it with a synthetic tree; only the thin CLI entry point
//! decodes the real embed.
//!
//! Embed states:
//! - **real**: `llms.txt` is present (staged by the xtask).
//!   `embed-info.json` records provenance (git commit + dirty flag of
//!   the staging checkout); it may be absent, in which case provenance
//!   reads as unknown.
//! - **placeholder**: no `llms.txt` (fresh clone; build.rs embedded a
//!   stub). Every content mode fails with instructions; `--embed-info`
//!   reports the state instead of failing.

use std::collections::HashSet;

use super::docs_embed::{EMBED_INFO, EmbedFiles, SITE_PREFIX};

/// How to turn a placeholder embed into a real one. Referenced by the
/// placeholder error and by `--embed-info`, and by `q2 docs serve`.
pub(crate) const REBUILD_HINT: &str =
    "run `cargo xtask build-agents-docs`, then rebuild with `cargo build --bin q2`";

/// What `q2 docs llms` (or its `q2 agents-info` alias) was asked for.
/// Built by `main.rs` from the mutually-exclusive CLI flags.
pub enum Mode {
    /// Bare invocation: the llms.txt index.
    Index,
    /// `--full`: llms-full.txt.
    Full,
    /// `--list`: href + title per page (JSON array with `--json`).
    List { json: bool },
    /// `--embed-info`: provenance of the embedded snapshot (JSON with `--json`).
    EmbedInfo { json: bool },
    /// `<href>`: one page.
    Page(String),
}

/// CLI entry point: print the requested view of the embedded docs to
/// stdout. Lookup failures (and placeholder embeds) come back as
/// errors, so the process exits nonzero with the message on stderr.
pub fn execute(mode: Mode) -> anyhow::Result<()> {
    let files = super::docs_embed::embedded_llms();
    let embed = DocsEmbed::new(&files);
    let out = match mode {
        Mode::Index => embed.index()?,
        Mode::Full => embed.full()?.to_string(),
        Mode::List { json: false } => embed
            .list()?
            .iter()
            .map(|p| format!("{}\t{}\n", p.href, p.title))
            .collect(),
        Mode::List { json: true } => embed.list_json()?,
        Mode::EmbedInfo { json: false } => embed.embed_info_text(),
        Mode::EmbedInfo { json: true } => embed.embed_info_json(),
        Mode::Page(href) => embed.page(&href)?.to_string(),
    };
    write_stdout(&out)
}

/// Write to stdout, treating a closed reader as success. This output
/// exists to be piped — `| head`, `| grep`, an agent's reader that
/// stops early — and Rust ignores SIGPIPE, so an unguarded `print!`
/// would abort with "failed printing to stdout: Broken pipe" instead
/// of ending quietly.
fn write_stdout(text: &str) -> anyhow::Result<()> {
    use std::io::Write;

    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    match lock.write_all(text.as_bytes()).and_then(|()| lock.flush()) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Provenance sidecar written by `cargo xtask build-agents-docs`
/// (real embeds) or by this crate's `build.rs` (placeholder embeds).
#[derive(Debug, Default, serde::Deserialize)]
pub struct EmbedInfo {
    #[serde(default)]
    pub placeholder: bool,
    #[serde(default)]
    pub commit: Option<String>,
    #[serde(default)]
    pub dirty: bool,
}

/// One page in `--list` output: companion href + page title.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct PageEntry {
    pub href: String,
    pub title: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DocsLlmsError {
    /// The binary carries the placeholder embed, not real docs.
    Placeholder,
    /// A real embed is missing an artifact it must contain
    /// (`llms-full.txt`); names the missing piece.
    Corrupt(String),
    /// `<href>` lookup miss, with up to five suggested hrefs.
    PageNotFound {
        query: String,
        suggestions: Vec<String>,
    },
}

impl std::fmt::Display for DocsLlmsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocsLlmsError::Placeholder => write!(
                f,
                "this q2 binary was built without the embedded documentation \
                 (placeholder embed); {REBUILD_HINT}"
            ),
            DocsLlmsError::Corrupt(what) => write!(
                f,
                "the embedded documentation is missing `{what}`; the embed is \
                 corrupt — {REBUILD_HINT}"
            ),
            DocsLlmsError::PageNotFound { query, suggestions } => {
                write!(f, "no embedded documentation page matches `{query}`")?;
                if !suggestions.is_empty() {
                    write!(f, "; did you mean one of:")?;
                    for s in suggestions {
                        write!(f, "\n  {s}")?;
                    }
                    writeln!(f)?;
                } else {
                    write!(f, ". ")?;
                }
                write!(f, "Use `q2 docs llms --list` to see every page.")
            }
        }
    }
}

impl std::error::Error for DocsLlmsError {}

/// The embedded docs files plus the lookup/rendering logic over the
/// llms.txt artifacts among them.
pub struct DocsEmbed<'a> {
    files: &'a EmbedFiles,
    /// Site-relative paths of the llms artifacts (the ledger).
    ledger: HashSet<String>,
}

impl<'a> DocsEmbed<'a> {
    pub fn new(files: &'a EmbedFiles) -> Self {
        let ledger = super::docs_embed::ledger(files).into_iter().collect();
        DocsEmbed { files, ledger }
    }

    /// Placeholder embeds have no `llms.txt` (build.rs only archives
    /// the real tree when the xtask has produced one).
    pub fn is_placeholder(&self) -> bool {
        self.site_file("llms.txt").is_none() || self.embed_info().placeholder
    }

    fn require_real(&self) -> Result<(), DocsLlmsError> {
        if self.is_placeholder() {
            Err(DocsLlmsError::Placeholder)
        } else {
            Ok(())
        }
    }

    /// A ledger-listed site file's text. Site files the ledger does not
    /// list (the HTML site, a user-resource `.md`) are invisible here.
    fn site_file(&self, rel: &str) -> Option<&'a str> {
        if !self.ledger.contains(rel) {
            return None;
        }
        self.files
            .get(&format!("{SITE_PREFIX}{rel}"))
            .and_then(|b| std::str::from_utf8(b).ok())
    }

    fn text_file(&self, name: &str) -> Result<&'a str, DocsLlmsError> {
        self.site_file(name)
            .ok_or_else(|| DocsLlmsError::Corrupt(name.to_string()))
    }

    /// Bare `q2 docs llms`: retrieval preamble + `llms.txt` verbatim.
    pub fn index(&self) -> Result<String, DocsLlmsError> {
        self.require_real()?;
        let llms_txt = self.text_file("llms.txt")?;
        Ok(format!(
            "<!--\n\
             q2 embedded documentation index (llms.txt).\n\
             Fetch one page:  q2 docs llms <href>   (hrefs listed below)\n\
             List all pages:  q2 docs llms --list\n\
             Whole corpus:    q2 docs llms --full\n\
             -->\n\n{llms_txt}"
        ))
    }

    /// `--full`: `llms-full.txt` verbatim.
    pub fn full(&self) -> Result<&'a str, DocsLlmsError> {
        self.require_real()?;
        self.text_file("llms-full.txt")
    }

    /// Every embedded `.md` companion href, sorted, with file contents.
    fn pages(&self) -> Vec<(String, &'a str)> {
        let mut pages: Vec<(String, &'a str)> = self
            .ledger
            .iter()
            .filter(|rel| rel.ends_with(".md"))
            .filter_map(|rel| Some((rel.clone(), self.site_file(rel)?)))
            .collect();
        pages.sort_by(|a, b| a.0.cmp(&b.0));
        pages
    }

    /// `--list`: every embedded `.md` companion, sorted by href, with
    /// the page title (first `#` heading, else the file stem).
    pub fn list(&self) -> Result<Vec<PageEntry>, DocsLlmsError> {
        self.require_real()?;
        Ok(self
            .pages()
            .into_iter()
            .map(|(href, contents)| {
                let title = title_of(contents).unwrap_or_else(|| stem_of(&href).to_string());
                PageEntry { href, title }
            })
            .collect())
    }

    /// `<href>`: one page's companion. Exact `.md` hrefs (as printed in
    /// `llms.txt`) win; otherwise the query is normalized — backslashes
    /// to slashes, a leading `./` or `/` and a trailing `/` stripped,
    /// `.qmd`/`.html` mapped to `.md`, and an extensionless path tried
    /// as `<q>.md` then `<q>/index.md`.
    pub fn page(&self, query: &str) -> Result<&'a str, DocsLlmsError> {
        self.require_real()?;
        if let Some(href) = self.resolve(query) {
            return Ok(self
                .site_file(&href)
                .expect("resolve only returns present pages"));
        }
        Err(self.not_found(query))
    }

    /// The companion href (`guides/alpha.md`) a page query names, with
    /// the normalization [`Self::page`] documents; `None` on a miss.
    /// `q2 docs serve <page>` resolves its argument through this too.
    pub fn resolve(&self, query: &str) -> Option<String> {
        candidates(&normalize_query(query))
            .into_iter()
            .find(|candidate| candidate.ends_with(".md") && self.site_file(candidate).is_some())
    }

    /// The lookup-miss error for `query`, with near-miss suggestions.
    pub fn not_found(&self, query: &str) -> DocsLlmsError {
        DocsLlmsError::PageNotFound {
            query: query.to_string(),
            suggestions: self.suggestions(&normalize_query(query)),
        }
    }

    /// Near-miss help for a failed lookup: pages whose stem shares a
    /// substring with the query's stem; failing that, pages in the
    /// query's directory. At most five, sorted.
    fn suggestions(&self, normalized_query: &str) -> Vec<String> {
        let pages = self.pages();
        let q_stem = stem_of(normalized_query).to_lowercase();
        let mut hits: Vec<String> = pages
            .iter()
            .filter(|(href, _)| {
                let stem = stem_of(href).to_lowercase();
                !q_stem.is_empty() && (stem.contains(&q_stem) || q_stem.contains(&stem))
            })
            .map(|(href, _)| href.clone())
            .collect();
        if hits.is_empty()
            && let Some((parent, _)) = normalized_query.rsplit_once('/')
        {
            let prefix = format!("{parent}/");
            hits = pages
                .iter()
                .filter(|(href, _)| href.starts_with(&prefix))
                .map(|(href, _)| href.clone())
                .collect();
        }
        hits.truncate(5);
        hits
    }

    /// Parsed `embed-info.json`; defaults when absent or unreadable.
    pub fn embed_info(&self) -> EmbedInfo {
        self.files
            .get(EMBED_INFO)
            .and_then(|b| serde_json::from_slice(b).ok())
            .unwrap_or_default()
    }

    /// `--embed-info`: human-readable provenance report. Works on
    /// placeholder embeds (that is its job).
    pub fn embed_info_text(&self) -> String {
        if self.is_placeholder() {
            return format!(
                "source: placeholder\n\
                 This binary carries no embedded documentation; {REBUILD_HINT}.\n"
            );
        }
        let info = self.embed_info();
        let commit = match (info.commit.as_deref(), info.dirty) {
            (Some(c), true) => format!("{c} (dirty)"),
            (Some(c), false) => c.to_string(),
            (None, _) => "unknown".to_string(),
        };
        format!(
            "source: real\ncommit: {commit}\npages: {}\n",
            self.pages().len()
        )
    }

    /// `--list --json`: the page list as a JSON array (bd-b6cocsxw).
    /// Machine consumers (the `q2 mcp` docs tool) read this instead of
    /// the tab-separated text form.
    pub fn list_json(&self) -> Result<String, DocsLlmsError> {
        let pages = self.list()?;
        Ok(
            serde_json::to_string_pretty(&pages).expect("PageEntry serialization cannot fail")
                + "\n",
        )
    }

    /// `--embed-info --json`: the same provenance as `embed_info_text`
    /// in machine-readable form (bd-b6cocsxw). Works on placeholder
    /// embeds (that is its job, like the text form).
    pub fn embed_info_json(&self) -> String {
        let info = self.embed_info();
        format!(
            "{}\n",
            serde_json::to_string_pretty(&serde_json::json!({
                "placeholder": self.is_placeholder(),
                "commit": info.commit,
                "dirty": info.dirty,
                "pages": self.pages().len(),
            }))
            .expect("embed-info JSON serialization cannot fail")
        )
    }
}

/// The page title is the first non-blank line iff it is an ATX `#`
/// heading (which is how the llms companion writer renders titles).
fn title_of(contents: &str) -> Option<String> {
    let first = contents.lines().find(|l| !l.trim().is_empty())?;
    let title = first.strip_prefix("# ")?.trim();
    (!title.is_empty()).then(|| title.to_string())
}

/// Final path segment without its extension (`guides/alpha.md` →
/// `alpha`).
fn stem_of(href: &str) -> &str {
    let base = href.rsplit('/').next().unwrap_or(href);
    base.rsplit_once('.').map_or(base, |(stem, _)| stem)
}

fn normalize_query(query: &str) -> String {
    let mut q = query.trim().replace('\\', "/");
    loop {
        if let Some(rest) = q.strip_prefix("./") {
            q = rest.to_string();
        } else if let Some(rest) = q.strip_prefix('/') {
            q = rest.to_string();
        } else if let Some(rest) = q.strip_suffix('/') {
            q = rest.to_string();
        } else {
            break;
        }
    }
    q
}

/// Lookup candidates for a normalized query, most-specific first.
fn candidates(q: &str) -> Vec<String> {
    if q.is_empty() {
        return Vec::new();
    }
    if q.ends_with(".md") {
        return vec![q.to_string()];
    }
    for source_ext in [".qmd", ".html"] {
        if let Some(base) = q.strip_suffix(source_ext) {
            return vec![format!("{base}.md")];
        }
    }
    // Extensionless: a page, or a directory standing for its index.
    vec![format!("{q}.md"), format!("{q}/index.md")]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::docs_embed::LLMS_MANIFEST;

    // A synthetic real embed:
    //   llms.txt, llms-full.txt, embed-info.json
    //   index.md, guides/alpha.md, guides/noheading.md, guides/sub/index.md
    // plus site files the ledger does not list (HTML, a resource .md),
    // which must be invisible to every mode.
    const LLMS_TXT: &str = "# Test Site\n\n> A test corpus\n\n## Guides\n\n\
                            - [Alpha](guides/alpha.md)\n\
                            - [Sub](guides/sub/index.md)\n";
    const LLMS_FULL: &str = "# Test Site — full corpus\n\nAlpha body. Sub body.\n";

    /// An embed of `site` (site-relative path, contents) where the
    /// ledger lists `ledger`, with `embed_info` as the sidecar.
    fn files(embed_info: Option<&str>, ledger: &[&str], site: &[(&str, &str)]) -> EmbedFiles {
        let mut files = EmbedFiles::new();
        if let Some(info) = embed_info {
            files.insert(EMBED_INFO.to_string(), info.as_bytes().into());
        }
        if !ledger.is_empty() {
            let manifest = serde_json::json!({ "version": 1, "generated": ledger });
            files.insert(
                LLMS_MANIFEST.to_string(),
                manifest.to_string().into_bytes().into(),
            );
        }
        for (rel, contents) in site {
            files.insert(format!("{SITE_PREFIX}{rel}"), contents.as_bytes().into());
        }
        files
    }

    fn real_files() -> EmbedFiles {
        files(
            Some("{\"commit\":\"abc1234\",\"dirty\":false}"),
            &[
                "index.md",
                "llms-full.txt",
                "llms.txt",
                "guides/alpha.md",
                "guides/noheading.md",
                "guides/sub/index.md",
            ],
            &[
                ("index.md", "\n\n# Home\n\nWelcome.\n"),
                ("llms-full.txt", LLMS_FULL),
                ("llms.txt", LLMS_TXT),
                ("guides/alpha.md", "# Alpha Page\n\nAlpha body.\n"),
                ("guides/noheading.md", "no heading here\n"),
                ("guides/sub/index.md", "# Sub Index\n\nSub body.\n"),
                // Not in the ledger:
                ("index.html", "<p>Home</p>"),
                ("guides/alpha.html", "<p>Alpha</p>"),
                ("guides/resource.md", "# A user resource\n"),
            ],
        )
    }

    fn placeholder_files() -> EmbedFiles {
        files(Some("{\"placeholder\":true}"), &[], &[])
    }

    // Real embed with a dirty commit and no llms-full.txt (corrupt).
    fn dirty_files() -> EmbedFiles {
        files(
            Some("{\"commit\":\"beef999\",\"dirty\":true}"),
            &["llms.txt"],
            &[("llms.txt", "# Dirty\n")],
        )
    }

    // Real embed with no embed-info.json at all.
    fn noinfo_files() -> EmbedFiles {
        files(None, &["llms.txt"], &[("llms.txt", "# NoInfo\n")])
    }

    /// Test fixtures are leaked so `DocsEmbed<'static>` can borrow them,
    /// keeping the assertions below as terse as they were with static
    /// `include_dir` trees.
    fn leak(files: EmbedFiles) -> &'static EmbedFiles {
        Box::leak(Box::new(files))
    }

    fn real() -> DocsEmbed<'static> {
        DocsEmbed::new(leak(real_files()))
    }
    fn placeholder() -> DocsEmbed<'static> {
        DocsEmbed::new(leak(placeholder_files()))
    }

    // ---- placeholder detection -------------------------------------

    #[test]
    fn placeholder_detected_by_missing_llms_txt() {
        assert!(!real().is_placeholder());
        assert!(placeholder().is_placeholder());
    }

    #[test]
    fn placeholder_blocks_content_modes_and_names_the_xtask() {
        let e = placeholder();
        for err in [
            e.index().unwrap_err(),
            e.full().map(|_| ()).unwrap_err(),
            e.list().map(|_| ()).unwrap_err(),
            e.page("index.md").map(|_| ()).unwrap_err(),
        ] {
            assert_eq!(err, DocsLlmsError::Placeholder);
            let msg = err.to_string();
            assert!(
                msg.contains("cargo xtask build-agents-docs"),
                "placeholder error must name the xtask: {msg}"
            );
            assert!(
                msg.contains("cargo build --bin q2"),
                "placeholder error must name the rebuild step: {msg}"
            );
        }
    }

    // ---- index / full ----------------------------------------------

    #[test]
    fn index_is_preamble_then_llms_txt_verbatim() {
        let out = real().index().unwrap();
        assert!(
            out.starts_with("<!--"),
            "preamble must be an HTML comment so the payload stays valid \
             markdown: {out}"
        );
        assert!(out.contains("q2 docs llms <href>"), "{out}");
        assert!(out.contains("q2 docs llms --list"), "{out}");
        assert!(out.contains("q2 docs llms --full"), "{out}");
        let end_of_comment = out.find("-->").expect("preamble comment must close");
        assert_eq!(
            &out[end_of_comment..],
            format!("-->\n\n{LLMS_TXT}"),
            "llms.txt must follow the preamble verbatim"
        );
    }

    #[test]
    fn full_is_llms_full_txt_verbatim() {
        assert_eq!(real().full().unwrap(), LLMS_FULL);
    }

    #[test]
    fn full_missing_from_real_embed_is_corrupt() {
        let e = DocsEmbed::new(leak(dirty_files()));
        assert_eq!(
            e.full().map(|_| ()).unwrap_err(),
            DocsLlmsError::Corrupt("llms-full.txt".to_string())
        );
    }

    // ---- list -------------------------------------------------------

    #[test]
    fn list_returns_all_md_pages_sorted_with_titles() {
        let pages = real().list().unwrap();
        let expect = [
            ("guides/alpha.md", "Alpha Page"),
            ("guides/noheading.md", "noheading"),
            ("guides/sub/index.md", "Sub Index"),
            ("index.md", "Home"),
        ];
        let got: Vec<(&str, &str)> = pages
            .iter()
            .map(|p| (p.href.as_str(), p.title.as_str()))
            .collect();
        assert_eq!(got, expect);
    }

    #[test]
    fn list_json_matches_the_text_list() {
        let e = real();
        let json: Vec<serde_json::Value> =
            serde_json::from_str(&e.list_json().unwrap()).expect("list_json parses");
        let text = e.list().unwrap();
        assert_eq!(json.len(), text.len());
        for (j, t) in json.iter().zip(text.iter()) {
            assert_eq!(j["href"].as_str().unwrap(), t.href);
            assert_eq!(j["title"].as_str().unwrap(), t.title);
        }
    }

    #[test]
    fn embed_info_json_reports_real_and_placeholder_states() {
        let real_info: serde_json::Value =
            serde_json::from_str(&real().embed_info_json()).expect("embed_info_json parses");
        assert_eq!(real_info["placeholder"], false);
        assert_eq!(real_info["commit"], "abc1234");
        assert_eq!(real_info["dirty"], false);
        assert_eq!(real_info["pages"], 4);

        let placeholder_info: serde_json::Value =
            serde_json::from_str(&placeholder().embed_info_json()).expect("parses");
        assert_eq!(placeholder_info["placeholder"], true);
        assert_eq!(placeholder_info["pages"], 0);
    }

    // ---- page lookup + normalization -------------------------------

    #[test]
    fn page_lookup_normalization_table() {
        let e = real();
        let alpha = "# Alpha Page\n\nAlpha body.\n";
        let sub = "# Sub Index\n\nSub body.\n";
        for (query, want) in [
            ("guides/alpha.md", alpha), // exact
            ("./guides/alpha.md", alpha),
            ("/guides/alpha.md", alpha),
            ("guides/alpha.qmd", alpha),
            ("guides/alpha.html", alpha),
            ("guides/alpha", alpha),
            ("guides\\alpha.md", alpha), // windows-pasted path
            ("guides/sub", sub),         // dir -> index.md
            ("guides/sub/", sub),        // trailing slash
            ("guides/sub/index.qmd", sub),
        ] {
            assert_eq!(e.page(query).unwrap(), want, "query: {query}");
        }
    }

    #[test]
    fn page_miss_suggests_by_stem_substring() {
        let err = real().page("guides/alph").unwrap_err();
        let DocsLlmsError::PageNotFound { query, suggestions } = &err else {
            panic!("expected PageNotFound, got {err:?}");
        };
        assert_eq!(query, "guides/alph");
        assert_eq!(suggestions, &["guides/alpha.md".to_string()]);
        let msg = err.to_string();
        assert!(msg.contains("guides/alpha.md"), "{msg}");
        assert!(msg.contains("--list"), "{msg}");
    }

    #[test]
    fn page_miss_suggests_same_directory_pages() {
        let err = real().page("guides/zzz.md").unwrap_err();
        let DocsLlmsError::PageNotFound { suggestions, .. } = &err else {
            panic!("expected PageNotFound, got {err:?}");
        };
        assert_eq!(
            suggestions,
            &[
                "guides/alpha.md".to_string(),
                "guides/noheading.md".to_string(),
                "guides/sub/index.md".to_string(),
            ]
        );
    }

    #[test]
    fn page_miss_with_no_near_matches_points_at_list() {
        let err = real().page("zzz/yyy.md").unwrap_err();
        let DocsLlmsError::PageNotFound { suggestions, .. } = &err else {
            panic!("expected PageNotFound, got {err:?}");
        };
        assert!(suggestions.is_empty());
        assert!(err.to_string().contains("--list"));
    }

    // ---- embed-info -------------------------------------------------

    #[test]
    fn embed_info_parses_sidecar() {
        let info = real().embed_info();
        assert!(!info.placeholder);
        assert_eq!(info.commit.as_deref(), Some("abc1234"));
        assert!(!info.dirty);
    }

    #[test]
    fn embed_info_text_real() {
        assert_eq!(
            real().embed_info_text(),
            "source: real\ncommit: abc1234\npages: 4\n"
        );
    }

    #[test]
    fn embed_info_text_dirty_commit_is_flagged() {
        assert_eq!(
            DocsEmbed::new(leak(dirty_files())).embed_info_text(),
            "source: real\ncommit: beef999 (dirty)\npages: 0\n"
        );
    }

    #[test]
    fn embed_info_text_missing_sidecar_reads_unknown() {
        assert_eq!(
            DocsEmbed::new(leak(noinfo_files())).embed_info_text(),
            "source: real\ncommit: unknown\npages: 0\n"
        );
    }

    #[test]
    fn embed_info_text_placeholder_names_the_fix() {
        let text = placeholder().embed_info_text();
        assert!(text.starts_with("source: placeholder\n"), "{text}");
        assert!(text.contains("cargo xtask build-agents-docs"), "{text}");
    }
}
