//! The documentation embedded in the binary, shared by `q2 docs llms`
//! and `q2 docs serve` (bd-hwop1zii, bd-x248xpyh).
//!
//! `cargo xtask build-agents-docs` stages the rendered docs site into
//! `agents-docs-dist/`; this crate's `build.rs` archives it as a tar.zst
//! (or a placeholder on a fresh clone) and it is embedded here with
//! `include_bytes!`. Archive layout:
//!
//! - `embed-info.json` — provenance (commit + dirty flag), or
//!   `{"placeholder":true}`;
//! - `llms-manifest.json` — the llms ledger: which site files are the
//!   llms.txt artifacts (`{"generated": ["llms.txt", "guides/a.md", …]}`);
//! - `site/…` — the rendered site, ledger-listed files first.
//!
//! The order is what makes [`decode_llms`] cheap: it stops at the first
//! entry outside the metadata and the ledger, so `q2 docs llms` never
//! decompresses the HTML site.

use std::collections::{HashMap, HashSet};
use std::io::Read;

/// The embedded archive (see `build.rs`).
static ARCHIVE: &[u8] = include_bytes!(env!("QUARTO_DOCS_EMBED_ARCHIVE"));

/// Archive path → contents.
pub type EmbedFiles = HashMap<String, Box<[u8]>>;

/// Archive path of the provenance sidecar.
pub const EMBED_INFO: &str = "embed-info.json";
/// Archive path of the llms ledger.
pub const LLMS_MANIFEST: &str = "llms-manifest.json";
/// Prefix of the rendered site's files in the archive.
pub const SITE_PREFIX: &str = "site/";

/// The embedded metadata and llms.txt artifacts only.
pub fn embedded_llms() -> EmbedFiles {
    decode_llms(ARCHIVE)
}

/// Every embedded file, the whole site included.
pub fn embedded_all() -> EmbedFiles {
    decode_all(ARCHIVE)
}

/// Decode the metadata and the ledger-listed site files, stopping at
/// the first entry that is neither.
pub fn decode_llms(archive: &[u8]) -> EmbedFiles {
    decode(archive, true)
}

/// Decode the whole archive.
pub fn decode_all(archive: &[u8]) -> EmbedFiles {
    decode(archive, false)
}

/// The llms ledger's `generated` list, as site-relative paths.
pub fn ledger(files: &EmbedFiles) -> Vec<String> {
    #[derive(serde::Deserialize)]
    struct LlmsManifest {
        generated: Vec<String>,
    }
    files
        .get(LLMS_MANIFEST)
        .and_then(|bytes| serde_json::from_slice::<LlmsManifest>(bytes).ok())
        .map(|m| m.generated)
        .unwrap_or_default()
}

/// The archive is written by this crate's own build.rs, so a decode
/// failure is a build bug, not a runtime condition — hence the
/// expects.
fn decode(archive: &[u8], llms_only: bool) -> EmbedFiles {
    let decoder =
        zstd::stream::read::Decoder::new(archive).expect("embedded docs tar.zst must initialize");
    let mut tar = tar::Archive::new(decoder);
    let mut files = EmbedFiles::new();
    // Archive paths of the ledger-listed site files, once the ledger
    // entry has gone by.
    let mut wanted: Option<HashSet<String>> = None;
    for entry in tar
        .entries()
        .expect("embedded docs tar.zst must list entries")
    {
        let mut entry = entry.expect("embedded docs tar.zst entry must parse");
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry
            .path()
            .expect("embedded docs tar.zst entry path must be valid")
            .to_string_lossy()
            .into_owned();
        let is_metadata = path == EMBED_INFO || path == LLMS_MANIFEST;
        if llms_only && !is_metadata && !wanted.as_ref().is_some_and(|w| w.contains(&path)) {
            break;
        }
        let mut bytes = Vec::with_capacity(entry.header().size().unwrap_or(0) as usize);
        entry
            .read_to_end(&mut bytes)
            .expect("embedded docs tar.zst entry must decode");
        files.insert(path.clone(), bytes.into_boxed_slice());
        if path == LLMS_MANIFEST {
            wanted = Some(
                ledger(&files)
                    .into_iter()
                    .map(|rel| format!("{SITE_PREFIX}{rel}"))
                    .collect(),
            );
        }
    }
    files
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Build a tar.zst with `entries` in the given order, as build.rs
    /// does.
    pub(crate) fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (path, bytes) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Regular);
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_mtime(0);
            header.set_cksum();
            builder.append_data(&mut header, path, *bytes).unwrap();
        }
        zstd::encode_all(builder.into_inner().unwrap().as_slice(), 3).unwrap()
    }

    const LEDGER: &[u8] = br#"{"version":1,"generated":["guides/a.md","llms.txt"]}"#;

    fn sample() -> Vec<u8> {
        archive(&[
            (EMBED_INFO, b"{\"commit\":\"abc\",\"dirty\":false}"),
            (LLMS_MANIFEST, LEDGER),
            ("site/guides/a.md", b"# A\n"),
            ("site/llms.txt", b"# Index\n"),
            ("site/guides/a.html", b"<p>A</p>"),
            ("site/index.html", b"<p>Home</p>"),
            // Out of order on purpose: after the prefix ends, a ledger
            // file is not reached — the prefix is the contract.
            ("site/late.md", b"late"),
        ])
    }

    #[test]
    fn decode_llms_stops_at_the_first_non_ledger_entry() {
        let files = decode_llms(&sample());
        let mut keys: Vec<&str> = files.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                EMBED_INFO,
                LLMS_MANIFEST,
                "site/guides/a.md",
                "site/llms.txt"
            ]
        );
    }

    #[test]
    fn decode_all_reads_everything() {
        let files = decode_all(&sample());
        assert_eq!(files.len(), 7);
        assert_eq!(&*files["site/index.html"], b"<p>Home</p>");
    }

    #[test]
    fn placeholder_archive_decodes_to_just_the_sidecar() {
        let placeholder = archive(&[(EMBED_INFO, b"{\"placeholder\":true}\n")]);
        let files = decode_llms(&placeholder);
        assert_eq!(files.len(), 1);
        assert!(ledger(&files).is_empty());
    }

    #[test]
    fn ledger_lists_site_relative_paths() {
        assert_eq!(ledger(&decode_llms(&sample())), ["guides/a.md", "llms.txt"]);
    }

    /// The real embed must keep the llms-first order build.rs promises:
    /// metadata, then every ledger-listed file, before any other site
    /// file. A regression would only cost `q2 docs llms` latency, so
    /// nothing else would notice.
    #[test]
    fn embedded_archive_is_llms_first() {
        let decoder = zstd::stream::read::Decoder::new(ARCHIVE).unwrap();
        let mut tar = tar::Archive::new(decoder);
        let paths: Vec<String> = tar
            .entries()
            .unwrap()
            .map(|e| e.unwrap().path().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(paths.first().map(String::as_str), Some(EMBED_INFO));
        let llms = decode_llms(ARCHIVE);
        if !llms.contains_key(LLMS_MANIFEST) {
            return; // placeholder embed: nothing more to order
        }
        let ledger_count = ledger(&llms).len();
        // The prefix decode got the sidecar, the ledger, and every
        // ledger-listed file — i.e. none of them came after the first
        // non-ledger file.
        assert_eq!(llms.len(), 2 + ledger_count, "llms prefix: {paths:?}");
    }
}
