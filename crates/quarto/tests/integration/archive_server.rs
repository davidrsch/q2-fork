//! Archive fixtures for the remote-source tests of `q2 add` and
//! `q2 use brand`: `.tar.gz` bytes, and a localhost server that serves
//! them and records every request path.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

/// `.tar.gz` bytes for `files`, each under `prefix` (`""` for none).
pub fn tar_gz(prefix: &str, files: &[(&str, &str)]) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let encoder = flate2::write::GzEncoder::new(&mut buf, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        for (name, content) in files {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Regular);
            header.set_mode(0o644);
            header.set_size(content.len() as u64);
            header.set_path(format!("{prefix}{name}")).unwrap();
            header.set_cksum();
            builder.append(&header, content.as_bytes()).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap();
    }
    buf
}

/// Serves `body` at `/archive.tar.gz`, 404 elsewhere, recording paths.
pub struct TestServer {
    base_url: String,
    pub requests: Arc<Mutex<Vec<String>>>,
    _thread: std::thread::JoinHandle<()>,
}

impl TestServer {
    pub fn start(body: Vec<u8>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind localhost");
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&requests);
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut buf = [0u8; 2048];
                let Ok(n) = stream.read(&mut buf) else {
                    continue;
                };
                let request = String::from_utf8_lossy(&buf[..n]).into_owned();
                let path = request
                    .lines()
                    .next()
                    .and_then(|l| l.split_whitespace().nth(1))
                    .unwrap_or("/")
                    .to_string();
                seen.lock().unwrap().push(path.clone());
                if path == "/archive.tar.gz" {
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/gzip\r\n\
                         Content-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(&body);
                } else {
                    let _ = stream.write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\n\
                          Connection: close\r\n\r\nnot found",
                    );
                }
                let _ = stream.flush();
            }
        });
        Self {
            base_url,
            requests,
            _thread: thread,
        }
    }

    pub fn url(&self) -> String {
        format!("{}/archive.tar.gz", self.base_url)
    }
}
