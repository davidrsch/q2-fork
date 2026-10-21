//! `cargo xtask check-noop-rebuild` — fail if building `q2` twice in a row
//! recompiles anything the second time (bd-7xlph6ki, GH #870).
//!
//! A build script that emits `cargo:rerun-if-changed=<path>` for a path that
//! does not exist makes cargo consider it stale on *every* invocation, so its
//! crate and everything above it recompile each time. That turned every
//! `cargo run --bin q2` into a ~16s rebuild of `quarto-preview` + `quarto`,
//! and `cargo xtask build-agents-docs` paid it once per example project.
//! Nothing else catches this class mechanically: tests pass, builds succeed,
//! they are just slow.
//!
//! The check runs `cargo build --bin q2` once to settle the tree, then again
//! with `--message-format=json`, and fails if the second run reports any
//! `compiler-artifact` with `"fresh": false`. `cargo xtask verify` runs it
//! after its workspace build; CI runs it in `test-suite.yml` with
//! `--profile ci`.

use anyhow::{Context, Result, bail};
use std::process::Stdio;

use crate::create_worktree::repo_root;

/// Run the check. `profile` is passed to cargo as `--profile <name>` (CI
/// builds with `ci`); `None` uses cargo's default `dev` profile.
/// `rustflags`, when set, overrides `RUSTFLAGS` for both builds; it must
/// match the build that came before, or cargo rebuilds the whole tree.
pub fn run(profile: Option<&str>, rustflags: Option<&str>) -> Result<()> {
    let root = repo_root()?;

    let mut args = vec!["build", "--bin", "q2"];
    if let Some(profile) = profile {
        args.extend(["--profile", profile]);
    }

    println!(
        "check-noop-rebuild: settling build (cargo {})",
        args.join(" ")
    );
    let mut settle = crate::util::nested_command("cargo");
    settle.args(&args).current_dir(&root);
    if let Some(flags) = rustflags {
        settle.env("RUSTFLAGS", flags);
    }
    let status = settle.status().context("spawning settling cargo build")?;
    if !status.success() {
        bail!("settling `cargo {}` failed", args.join(" "));
    }

    println!("check-noop-rebuild: rebuilding; nothing may recompile");
    args.push("--message-format=json");
    let mut rebuild = crate::util::nested_command("cargo");
    rebuild
        .args(&args)
        .current_dir(&root)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    if let Some(flags) = rustflags {
        rebuild.env("RUSTFLAGS", flags);
    }
    let output = rebuild.output().context("spawning checking cargo build")?;
    if !output.status.success() {
        bail!("checking `cargo {}` failed", args.join(" "));
    }

    let stale = stale_units(&String::from_utf8_lossy(&output.stdout));
    if !stale.is_empty() {
        bail!(
            "a second `cargo build --bin q2` recompiled {} unit(s), so some build \
             script or input is always stale:\n  {}\n\
             Find the cause with:\n  \
             CARGO_LOG=cargo::core::compiler::fingerprint=info cargo build --bin q2 \
             2>&1 | grep 'dirty:'\n\
             A `MissingFile` entry is a build script watching a path that does not \
             exist; a `changed` entry naming a broad directory (cargo scans it \
             recursively) is one watching a tree the build writes into. Use \
             `quarto_build_support::watch_dir`/`watch_file` (bd-7xlph6ki).",
            stale.len(),
            stale.join("\n  ")
        );
    }
    println!("✓ check-noop-rebuild: second build was a no-op");
    Ok(())
}

/// The `package_id`s (with target names) of every `compiler-artifact`
/// message in cargo's JSON stream that is not `fresh`. Non-JSON lines and
/// other message kinds are ignored.
fn stale_units(json_lines: &str) -> Vec<String> {
    json_lines
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|msg| msg["reason"] == "compiler-artifact" && msg["fresh"] == false)
        .map(|msg| {
            format!(
                "{} ({})",
                msg["package_id"].as_str().unwrap_or("<unknown package>"),
                msg["target"]["name"].as_str().unwrap_or("<unknown target>")
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::stale_units;

    #[test]
    fn all_fresh_artifacts_report_nothing() {
        let stream = r#"{"reason":"compiler-artifact","package_id":"a 0.1.0","target":{"name":"a"},"fresh":true}
{"reason":"build-script-executed","package_id":"b 0.1.0"}
{"reason":"build-finished","success":true}"#;
        assert!(stale_units(stream).is_empty());
    }

    #[test]
    fn non_fresh_artifacts_are_reported() {
        let stream = r#"{"reason":"compiler-artifact","package_id":"quarto-preview 0.34.0","target":{"name":"quarto_preview"},"fresh":false}
{"reason":"compiler-artifact","package_id":"serde 1.0.0","target":{"name":"serde"},"fresh":true}
{"reason":"compiler-artifact","package_id":"quarto 0.34.0","target":{"name":"q2"},"fresh":false}"#;
        assert_eq!(
            stale_units(stream),
            vec![
                "quarto-preview 0.34.0 (quarto_preview)".to_string(),
                "quarto 0.34.0 (q2)".to_string(),
            ]
        );
    }

    #[test]
    fn non_json_lines_are_ignored() {
        assert!(stale_units("   Compiling foo\nnot json\n").is_empty());
    }
}
