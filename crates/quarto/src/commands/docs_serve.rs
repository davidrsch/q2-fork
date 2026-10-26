//! `q2 docs serve` (alias `q2 docs open`) — browse the documentation
//! site embedded in the binary, offline (bd-x248xpyh).
//!
//! The human-facing counterpart to `q2 docs llms`: the same embedded
//! archive ([`super::docs_embed`]) carries the whole rendered docs site,
//! which this command decodes into memory and serves on a loopback port
//! until Ctrl-C. Nothing is fetched from the network — the staging
//! xtask refuses a site whose content needs it — and nothing is written
//! to disk.
//!
//! An optional page argument deep-links: it is resolved with the same
//! normalization and near-miss suggestions as `q2 docs llms <href>`,
//! before the server starts, so a typo fails fast.
//!
//! Plan: `claude-notes/plans/2026-10-26-q2-docs-serve.md`.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tracing::{info, warn};

use super::docs_embed::{EmbedFiles, SITE_PREFIX};
use super::docs_llms::{DocsEmbed, DocsLlmsError};
use super::preview::spawn_browser_open_when_ready;
use super::preview_static::ShutdownSignal;

/// `q2 docs serve` arguments (built by `main.rs`).
pub struct ServeArgs {
    /// Page to open, in any spelling `q2 docs llms <href>` accepts.
    pub page: Option<String>,
    /// Port to listen on. Default: an OS-assigned free port.
    pub port: Option<u16>,
    /// Interface to bind. Default: `127.0.0.1`.
    pub host: Option<String>,
    /// Skip the browser-open step.
    pub no_browser: bool,
    /// Open in this browser instead of the system default.
    pub browser: Option<String>,
}

pub fn execute(args: ServeArgs) -> Result<()> {
    // The decoded site lives for the rest of the process: the server
    // borrows from it until exit.
    let files: &'static EmbedFiles = Box::leak(Box::new(super::docs_embed::embedded_all()));
    let embed = DocsEmbed::new(files);
    if embed.is_placeholder() {
        return Err(DocsLlmsError::Placeholder.into());
    }
    let page_path = match &args.page {
        Some(query) => page_url_path(&embed, files, query)?,
        None => String::new(),
    };
    let commit = embed
        .embed_info()
        .commit
        .map(|c| c.chars().take(12).collect::<String>());

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(serve(args, files, page_path, commit))
}

/// The site-relative URL path (`guides/projects/preview.html`, or
/// `guides/` for a section index) of the page `query` names.
fn page_url_path(embed: &DocsEmbed<'_>, files: &EmbedFiles, query: &str) -> Result<String> {
    let Some(companion) = embed.resolve(query) else {
        return Err(embed.not_found(query).into());
    };
    let stem = companion
        .strip_suffix(".md")
        .expect("resolve returns .md companions");
    let html = format!("{stem}.html");
    if !files.contains_key(&format!("{SITE_PREFIX}{html}")) {
        // A companion without its page: the embed is inconsistent.
        return Err(DocsLlmsError::Corrupt(html).into());
    }
    // Section indexes read better as their directory URL.
    Ok(match html.strip_suffix("index.html") {
        Some(dir) if dir.is_empty() || dir.ends_with('/') => dir.to_string(),
        _ => html,
    })
}

async fn serve(
    args: ServeArgs,
    files: &'static EmbedFiles,
    page_path: String,
    commit: Option<String>,
) -> Result<()> {
    let site: quarto_preview::static_mode::SiteFiles =
        Arc::new(move |rel| files.get(&format!("{SITE_PREFIX}{rel}")).map(|b| &**b));
    let router = quarto_preview::static_mode::build_embedded_site_router(site);

    // Installed before the port opens, so a Ctrl-C the moment a client
    // can connect is caught (same ordering as `q2 preview --static`).
    let mut shutdown = ShutdownSignal::install()?;

    let host = args.host.unwrap_or_else(|| "127.0.0.1".to_string());
    let requested_port = args.port.unwrap_or(0);
    let listener = match tokio::net::TcpListener::bind((host.as_str(), requested_port)).await {
        Ok(listener) => listener,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => anyhow::bail!(
            "port {requested_port} on {host} is already in use; pass --port 0 to let the OS \
             pick a free port, or omit --port for the default behaviour"
        ),
        Err(e) => return Err(e).with_context(|| format!("binding {host}:{requested_port}")),
    };
    let port = listener
        .local_addr()
        .context("local_addr of the bound listener")?
        .port();
    let url = format!("http://{host}:{port}/{page_path}");

    info!(%url, "starting q2 docs serve");
    println!();
    println!("  q2 docs serve");
    println!("  → {url}");
    match &commit {
        Some(c) => println!("  Quarto 2 documentation (commit {c}), served from this binary"),
        None => println!("  Quarto 2 documentation, served from this binary"),
    }
    println!("  Ctrl-C to stop");
    println!();
    if !args.no_browser {
        spawn_browser_open_when_ready(host.clone(), port, url, args.browser);
    }

    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(quarto_preview::static_mode::serve(
        router,
        listener,
        async move {
            let _ = stop_rx.await;
        },
    ));
    shutdown.wait().await;
    println!("Received Ctrl-C, shutting down");
    let _ = stop_tx.send(());
    match tokio::time::timeout(Duration::from_secs(5), server).await {
        Ok(Ok(Ok(()))) => {}
        Ok(Ok(Err(e))) => warn!(error = %e, "server error during shutdown"),
        Ok(Err(e)) => warn!(error = %e, "server task failed"),
        Err(_) => warn!("server did not finish shutting down within 5s; exiting anyway"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::docs_embed::LLMS_MANIFEST;

    fn files(ledger: &[&str], site: &[&str]) -> EmbedFiles {
        let mut files = EmbedFiles::new();
        let manifest = serde_json::json!({ "version": 1, "generated": ledger });
        files.insert(
            LLMS_MANIFEST.to_string(),
            manifest.to_string().into_bytes().into(),
        );
        for rel in site {
            files.insert(format!("{SITE_PREFIX}{rel}"), b"# Page\n"[..].into());
        }
        files
    }

    fn sample() -> EmbedFiles {
        files(
            &[
                "llms.txt",
                "index.md",
                "guides/index.md",
                "guides/projects/preview.md",
                "guides/orphan.md",
            ],
            &[
                "llms.txt",
                "index.md",
                "index.html",
                "guides/index.md",
                "guides/index.html",
                "guides/projects/preview.md",
                "guides/projects/preview.html",
                "guides/orphan.md",
            ],
        )
    }

    #[test]
    fn page_spellings_map_to_the_html_page() {
        let files = sample();
        let embed = DocsEmbed::new(&files);
        for query in [
            "guides/projects/preview",
            "guides/projects/preview.md",
            "guides/projects/preview.qmd",
            "/guides/projects/preview.html",
        ] {
            assert_eq!(
                page_url_path(&embed, &files, query).unwrap(),
                "guides/projects/preview.html",
                "{query}"
            );
        }
    }

    #[test]
    fn section_indexes_map_to_their_directory_url() {
        let files = sample();
        let embed = DocsEmbed::new(&files);
        assert_eq!(page_url_path(&embed, &files, "guides").unwrap(), "guides/");
        assert_eq!(page_url_path(&embed, &files, "index.md").unwrap(), "");
    }

    #[test]
    fn unknown_page_fails_with_suggestions() {
        let files = sample();
        let embed = DocsEmbed::new(&files);
        let err = page_url_path(&embed, &files, "guides/projects/previw").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("no embedded documentation page matches"),
            "{msg}"
        );
        assert!(msg.contains("guides/projects/preview.md"), "{msg}");
    }

    #[test]
    fn companion_without_html_page_is_corrupt() {
        let files = sample();
        let embed = DocsEmbed::new(&files);
        let err = page_url_path(&embed, &files, "guides/orphan").unwrap_err();
        assert!(err.to_string().contains("guides/orphan.html"), "{err}");
    }
}
