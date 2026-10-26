/*
 * tests/integration/docs_serve_cli.rs
 * Copyright (c) 2026 Posit, PBC
 *
 * `q2 docs serve` / `q2 docs open` e2e (bd-x248xpyh).
 */

//! Drives the real `q2` binary. Like `docs_llms_cli.rs`, every test
//! here must pass in BOTH embed states — a fresh clone (placeholder
//! embed: the command fails with instructions) and a staged tree (real
//! embed: the command serves the site) — using `q2 docs llms
//! --embed-info` as the state oracle.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Output, Stdio};
use std::time::Duration;

const Q2_BIN: &str = env!("CARGO_BIN_EXE_q2");

fn q2(args: &[&str]) -> Output {
    Command::new(Q2_BIN)
        .args(args)
        .output()
        .expect("spawn q2 binary")
}

fn embed_is_real() -> bool {
    let out = q2(&["docs", "llms", "--embed-info"]);
    let text = String::from_utf8_lossy(&out.stdout);
    if text.starts_with("source: real\n") {
        true
    } else if text.starts_with("source: placeholder\n") {
        false
    } else {
        panic!("--embed-info must name the embed state on line 1: {text}");
    }
}

/// A running `q2 docs <verb>` server, killed on drop.
struct Server {
    child: Child,
    /// The URL the server printed (`http://127.0.0.1:<port>/<page>`).
    url: String,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    fn start(verb: &str, extra: &[&str]) -> Server {
        let mut child = Command::new(Q2_BIN)
            .args(["docs", verb, "--no-browser", "--port", "0"])
            .args(extra)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn q2 docs serve");
        let stdout = child.stdout.take().unwrap();
        let mut url = None;
        for line in BufReader::new(stdout).lines() {
            let line = line.expect("read q2 docs serve stdout");
            if let Some(found) = line.trim().strip_prefix("→ ") {
                url = Some(found.to_string());
                break;
            }
        }
        let url = url.expect("q2 docs serve must print `→ <url>` once listening");
        Server { child, url }
    }

    fn origin(&self) -> &str {
        let after_scheme = &self.url["http://".len()..];
        let end = after_scheme.find('/').unwrap_or(after_scheme.len());
        &self.url[.."http://".len() + end]
    }

    /// GET `path`; returns (status, body). The server binds before it
    /// prints the URL, so no readiness retry is needed.
    fn get(&self, path: &str) -> (u16, String) {
        let host_port = &self.origin()["http://".len()..];
        let mut stream = TcpStream::connect(host_port).expect("connect to q2 docs serve");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: {host_port}\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        let status = response
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .expect("HTTP status line");
        let body = response
            .split_once("\r\n\r\n")
            .map_or("", |(_, b)| b)
            .to_string();
        (status, body)
    }
}

#[test]
fn serves_the_site_or_names_the_fix() {
    if !embed_is_real() {
        let out = q2(&["docs", "serve", "--no-browser", "--port", "0"]);
        assert!(!out.status.success(), "placeholder embed must fail");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("cargo xtask build-agents-docs"),
            "stderr must name the xtask: {stderr}"
        );
        return;
    }
    let server = Server::start("serve", &[]);
    assert_eq!(server.url, format!("{}/", server.origin()));

    let (status, body) = server.get("/");
    assert_eq!(status, 200);
    assert!(
        body.contains("Quarto 2"),
        "home page: {}",
        &body[..body.len().min(500)]
    );
    let (status, _) = server.get("/llms.txt");
    assert_eq!(status, 200, "the llms artifacts are part of the site too");
    let (status, _) = server.get("/site_libs/quarto-search/pagefind/pagefind.js");
    assert_eq!(status, 200, "website search must be served");
    let (status, _) = server.get("/no-such-page.html");
    assert_eq!(status, 404);
}

#[test]
fn open_is_serve_and_deep_links_a_page() {
    if !embed_is_real() {
        let out = q2(&["docs", "open", "--no-browser", "--port", "0"]);
        assert!(!out.status.success(), "placeholder embed must fail");
        return;
    }
    let server = Server::start("open", &["guides/projects/preview"]);
    assert_eq!(
        server.url,
        format!("{}/guides/projects/preview.html", server.origin())
    );
    let (status, body) = server.get("/guides/projects/preview.html");
    assert_eq!(status, 200);
    assert!(body.contains("<html"), "{}", &body[..body.len().min(500)]);
}

#[test]
fn unknown_page_fails_before_serving_with_suggestions() {
    let out = q2(&[
        "docs",
        "serve",
        "--no-browser",
        "--port",
        "0",
        "guides/projects/previe",
    ]);
    assert!(!out.status.success(), "an unknown page must fail fast");
    let stderr = String::from_utf8_lossy(&out.stderr);
    if embed_is_real() {
        assert!(
            stderr.contains("guides/projects/preview.md"),
            "the miss must suggest the near page: {stderr}"
        );
    } else {
        assert!(stderr.contains("cargo xtask build-agents-docs"), "{stderr}");
    }
}
