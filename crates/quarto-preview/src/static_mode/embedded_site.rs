//! A read-only router over a website held in memory — the server behind
//! `q2 docs serve` (bd-x248xpyh), which serves the documentation site
//! embedded in the `q2` binary.
//!
//! This is the static server ([`super::server`]) minus everything that
//! exists for previewing: no reload client, no SSE channel, no page-view
//! reports, no pdf.js viewer, no listing of rendered pages. The site is
//! a lookup function from a `/`-separated site-relative path to bytes,
//! so the caller decides where the files live (the `q2` binary decodes
//! its embedded archive once and hands over a map).
//!
//! Plan: `claude-notes/plans/2026-10-26-q2-docs-serve.md` § Server.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};

use super::server::{NO_STORE, decode_segments, plain_not_found, redirect};

/// The site: a site-relative path (`guides/index.html`, no leading
/// slash) to the file's bytes, or `None` when there is no such file.
pub type SiteFiles = Arc<dyn Fn(&str) -> Option<&'static [u8]> + Send + Sync>;

/// Build the router: every GET/HEAD is looked up in `files`. A
/// directory URL serves its `index.html`, and a directory named
/// without its trailing slash redirects to it, so relative links in
/// the page resolve the way they do on a real web server.
pub fn build_embedded_site_router(files: SiteFiles) -> Router {
    Router::new().fallback(serve_path).with_state(files)
}

async fn serve_path(State(files): State<SiteFiles>, req: Request) -> Response {
    let is_head = match *req.method() {
        Method::GET => false,
        Method::HEAD => true,
        _ => return StatusCode::METHOD_NOT_ALLOWED.into_response(),
    };
    let raw_path = req.uri().path();
    let Some(segments) = decode_segments(raw_path) else {
        return plain_not_found(is_head);
    };
    let rel = segments.join("/");
    if !rel.is_empty()
        && let Some(bytes) = files(&rel)
    {
        return file_response(&rel, bytes, is_head);
    }
    let index = if rel.is_empty() {
        "index.html".to_string()
    } else {
        format!("{rel}/index.html")
    };
    match files(&index) {
        Some(_) if !raw_path.ends_with('/') => {
            redirect(StatusCode::MOVED_PERMANENTLY, &format!("{raw_path}/"))
        }
        Some(bytes) => file_response(&index, bytes, is_head),
        None => plain_not_found(is_head),
    }
}

fn file_response(rel: &str, bytes: &'static [u8], is_head: bool) -> Response {
    let mime = mime_guess::from_path(rel).first_or_octet_stream();
    let content_type = if mime.type_() == mime_guess::mime::TEXT {
        // Text without a charset is decoded as windows-1252 by some
        // browsers; the site is UTF-8 throughout.
        HeaderValue::from_str(&format!("{mime}; charset=utf-8"))
    } else {
        HeaderValue::from_str(mime.as_ref())
    }
    .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, bytes.len())
        .header(header::CACHE_CONTROL, NO_STORE)
        .body(if is_head {
            Body::empty()
        } else {
            Body::from(bytes)
        })
        .expect("static headers are valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;
    use std::collections::HashMap;
    use tower::ServiceExt;

    fn site() -> Router {
        let files: HashMap<&'static str, &'static [u8]> = HashMap::from([
            ("index.html", &b"<p>home</p>"[..]),
            ("guides/index.html", b"<p>guides</p>"),
            ("guides/a.html", b"<p>a</p>"),
            ("site_libs/quarto.css", b"body{}"),
            ("site_libs/quarto-search/pagefind/pagefind.js", b"//js"),
            ("search.json", b"[]"),
            ("img/logo.png", b"\x89PNG"),
            ("a b.html", b"<p>space</p>"),
        ]);
        build_embedded_site_router(Arc::new(move |rel| files.get(rel).copied()))
    }

    async fn get(path: &str) -> Response {
        site()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    async fn body(response: Response) -> Vec<u8> {
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec()
    }

    fn content_type(response: &Response) -> &str {
        response.headers()[header::CONTENT_TYPE].to_str().unwrap()
    }

    #[tokio::test]
    async fn root_serves_index_html() {
        let response = get("/").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(content_type(&response), "text/html; charset=utf-8");
        assert_eq!(body(response).await, b"<p>home</p>");
    }

    #[tokio::test]
    async fn files_are_served_with_their_content_types() {
        for (path, want) in [
            ("/guides/a.html", "text/html; charset=utf-8"),
            ("/site_libs/quarto.css", "text/css; charset=utf-8"),
            ("/search.json", "application/json"),
            ("/img/logo.png", "image/png"),
        ] {
            let response = get(path).await;
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            assert_eq!(content_type(&response), want, "{path}");
        }
        let js = get("/site_libs/quarto-search/pagefind/pagefind.js").await;
        assert!(
            content_type(&js).contains("javascript"),
            "{}",
            content_type(&js)
        );
    }

    #[tokio::test]
    async fn directory_serves_index_and_redirects_without_slash() {
        let response = get("/guides/").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body(response).await, b"<p>guides</p>");

        let response = get("/guides").await;
        assert_eq!(response.status(), StatusCode::MOVED_PERMANENTLY);
        assert_eq!(response.headers()[header::LOCATION], "/guides/");
    }

    #[tokio::test]
    async fn percent_encoded_paths_resolve() {
        let response = get("/a%20b.html").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body(response).await, b"<p>space</p>");
    }

    #[tokio::test]
    async fn misses_and_traversal_are_404() {
        for path in [
            "/nope.html",
            "/guides/zzz/",
            "/../index.html",
            "/%2e%2e/index.html",
        ] {
            assert_eq!(get(path).await.status(), StatusCode::NOT_FOUND, "{path}");
        }
    }

    #[tokio::test]
    async fn head_has_headers_but_no_body() {
        let response = site()
            .oneshot(Request::head("/guides/a.html").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_LENGTH], "8");
        assert!(body(response).await.is_empty());
    }

    #[tokio::test]
    async fn other_methods_are_refused() {
        let response = site()
            .oneshot(Request::post("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }
}
