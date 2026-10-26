//! Offline check for the staged docs site (bd-x248xpyh).
//!
//! `q2 docs serve` promises that the embedded documentation needs no
//! network access for any of its content. This scanner enforces that at
//! staging time: it walks the staged site and reports every subresource
//! that would be fetched from an absolute (`http:`, `https:`, or
//! protocol-relative `//`) URL, so a docs page that starts loading, say,
//! MathJax from a CDN breaks `cargo xtask build-agents-docs` instead of
//! an offline reader's page.
//!
//! What counts as a subresource:
//!
//! - HTML: `src` / `srcset` / `poster` / `data` on `<script>`, `<img>`,
//!   `<iframe>`, `<source>`, `<video>`, `<audio>`, `<track>`, `<embed>`,
//!   `<object>`; `href` on `<link>` whose `rel` loads something
//!   (stylesheet, preload, modulepreload, icon, manifest); and the CSS
//!   rules inside `<style>` blocks.
//! - CSS files: `@import` and `url(...)`.
//!
//! Navigation (`<a href>`, `<link rel=canonical>`, …) is not a
//! subresource and is never flagged. JavaScript is not scanned: URLs
//! there are built at runtime and cannot be judged statically, and the
//! JS in the site is q2's own vendored runtime (`resources/vendored.toml`),
//! not docs content.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

/// Remote subresources the docs site is allowed to reference, by URL
/// prefix. Keep this list short, and give every entry a reason.
const ALLOWED_PREFIXES: &[&str] = &[
    // The Bootstrap theme's `@import` of Source Sans Pro. Offline it
    // fails and text falls back to the system sans-serif stack, which
    // is acceptable (decided 2026-10-26, plan
    // claude-notes/plans/2026-10-26-q2-docs-serve.md § Offline guarantee).
    "https://fonts.googleapis.com/",
    "https://fonts.gstatic.com/",
];

/// One remote subresource reference: the site-relative file it is in
/// and the URL.
#[derive(Debug, PartialEq, Eq)]
pub struct RemoteReference {
    pub file: String,
    pub url: String,
}

static TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<(script|img|iframe|source|video|audio|track|embed|object|link)\b([^>]*)>")
        .unwrap()
});
static ATTR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)([a-z-]+)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#).unwrap()
});
static STYLE_BLOCK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<style\b[^>]*>(.*?)</style>").unwrap());
static CSS_URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?:@import\s+(?:url\(\s*)?|url\(\s*)["']?\s*([^"'\s();]+)"#).unwrap()
});

/// Whether fetching `url` would leave the machine.
fn is_remote(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("//")
}

fn is_allowed(url: &str) -> bool {
    ALLOWED_PREFIXES.iter().any(|p| url.trim().starts_with(p))
}

/// Remote URLs in CSS text (`@import`, `url()`).
fn css_urls(css: &str) -> Vec<String> {
    CSS_URL
        .captures_iter(css)
        .map(|c| c[1].to_string())
        .filter(|u| is_remote(u))
        .collect()
}

/// Remote subresource URLs in HTML text.
fn html_urls(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    for tag in TAG.captures_iter(html) {
        let name = tag[1].to_ascii_lowercase();
        let attrs: Vec<(String, String)> = ATTR
            .captures_iter(&tag[2])
            .map(|a| {
                let value = a
                    .get(2)
                    .or(a.get(3))
                    .or(a.get(4))
                    .map_or("", |m| m.as_str());
                (a[1].to_ascii_lowercase(), value.to_string())
            })
            .collect();
        let attr = |key: &str| {
            attrs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str())
        };
        if name == "link" {
            let rel = attr("rel").unwrap_or("").to_ascii_lowercase();
            let loads = rel.split_whitespace().any(|r| {
                matches!(
                    r,
                    "stylesheet" | "preload" | "modulepreload" | "icon" | "manifest"
                )
            });
            if loads && let Some(href) = attr("href") {
                out.push(href.to_string());
            }
            continue;
        }
        for key in ["src", "poster", "data"] {
            if let Some(v) = attr(key) {
                out.push(v.to_string());
            }
        }
        if let Some(srcset) = attr("srcset") {
            out.extend(
                srcset
                    .split(',')
                    .filter_map(|candidate| candidate.split_whitespace().next())
                    .map(str::to_string),
            );
        }
    }
    out.retain(|u| is_remote(u));
    for block in STYLE_BLOCK.captures_iter(html) {
        out.extend(css_urls(&block[1]));
    }
    out
}

/// Scan one file's contents; `rel` picks the scanner by extension.
fn scan_file(rel: &str, text: &str) -> Vec<String> {
    let ext = rel.rsplit_once('.').map_or("", |(_, e)| e);
    if ext.eq_ignore_ascii_case("html") {
        html_urls(text)
    } else if ext.eq_ignore_ascii_case("css") {
        css_urls(text)
    } else {
        Vec::new()
    }
}

/// Every non-allowlisted remote subresource reference under `site`,
/// sorted by file then URL.
pub fn scan_site(site: &Path) -> anyhow::Result<Vec<RemoteReference>> {
    let mut found = Vec::new();
    for entry in walkdir::WalkDir::new(site) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(site)?
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        if !(rel.ends_with(".html") || rel.ends_with(".css")) {
            continue;
        }
        // Lossy: a stray non-UTF-8 byte must not hide a URL elsewhere
        // in the file.
        let text = String::from_utf8_lossy(&std::fs::read(entry.path())?).into_owned();
        for url in scan_file(&rel, &text) {
            if !is_allowed(&url) {
                found.push(RemoteReference {
                    file: rel.clone(),
                    url,
                });
            }
        }
    }
    found.sort_by(|a, b| (&a.file, &a.url).cmp(&(&b.file, &b.url)));
    found.dedup();
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_remote_script_stylesheet_image_iframe() {
        let html = r#"<html><head>
            <script src="https://cdn.jsdelivr.net/npm/mathjax@3/es5/tex.js"></script>
            <link rel="stylesheet" href="//unpkg.com/x.css">
            </head><body>
            <img SRC='http://example.com/a.png'>
            <iframe src=https://www.youtube.com/embed/xyz></iframe>
            <img srcset="local.png 1x, https://img.example.com/b.png 2x">
            </body></html>"#;
        assert_eq!(
            html_urls(html),
            [
                "https://cdn.jsdelivr.net/npm/mathjax@3/es5/tex.js",
                "//unpkg.com/x.css",
                "http://example.com/a.png",
                "https://www.youtube.com/embed/xyz",
                "https://img.example.com/b.png",
            ]
        );
    }

    #[test]
    fn ignores_local_resources_and_navigation() {
        let html = r#"<script src="../site_libs/quarto.js"></script>
            <link rel="stylesheet" href="/site_libs/bootstrap.css">
            <link rel="canonical" href="https://quarto.org/docs/">
            <a href="https://github.com/quarto-dev/q2">GitHub</a>
            <img src="images/figure.png">
            <pre><code>&lt;script src="https://cdn.example.com/x.js"&gt;</code></pre>"#;
        assert!(html_urls(html).is_empty(), "{:?}", html_urls(html));
    }

    #[test]
    fn scans_style_blocks_but_not_prose() {
        let html = r#"<style>@import url("https://cdn.example.com/f.css");</style>
            <p>Use url(https://example.com/in-prose) in your CSS.</p>"#;
        assert_eq!(html_urls(html), ["https://cdn.example.com/f.css"]);
    }

    #[test]
    fn css_import_and_url_forms() {
        let css = r#"@import url('https://fonts.googleapis.com/css2?family=X');
            @import "https://cdn.example.com/a.css";
            .x { background: url(https://cdn.example.com/b.png); }
            .y { background: url("../img/local.png"); }
            .z { src: url(data:font/woff2;base64,AAAA); }"#;
        assert_eq!(
            css_urls(css),
            [
                "https://fonts.googleapis.com/css2?family=X",
                "https://cdn.example.com/a.css",
                "https://cdn.example.com/b.png",
            ]
        );
    }

    #[test]
    fn scan_site_applies_the_allowlist_and_reports_relative_paths() {
        let tmp = tempfile::TempDir::new().unwrap();
        let site = tmp.path();
        std::fs::create_dir_all(site.join("site_libs")).unwrap();
        std::fs::create_dir_all(site.join("guides")).unwrap();
        std::fs::write(
            site.join("site_libs").join("theme.css"),
            "@import url('https://fonts.googleapis.com/css2?family=Source+Sans+Pro');",
        )
        .unwrap();
        std::fs::write(
            site.join("guides").join("math.html"),
            r#"<script src="https://cdn.jsdelivr.net/npm/mathjax@3/tex.js"></script>"#,
        )
        .unwrap();
        // JavaScript is out of scope, even with a URL in it.
        std::fs::write(site.join("app.js"), "load('https://cdn.example.com/x.js')").unwrap();

        assert_eq!(
            scan_site(site).unwrap(),
            [RemoteReference {
                file: "guides/math.html".to_string(),
                url: "https://cdn.jsdelivr.net/npm/mathjax@3/tex.js".to_string(),
            }]
        );
    }
}
