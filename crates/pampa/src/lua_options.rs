/*
 * lua_options.rs
 * Copyright (c) 2026 Posit, PBC
 */

//! What the host hands a Lua environment beyond the document: the source
//! dialects its `require` may compile ([`crate::dialect`]) and the module
//! roots it may search ([`ModuleRoots`]). One struct, so the next
//! host-provided setting is a field here rather than a parameter on every
//! `_with` entry point (filters, shortcode handlers, lmd cells).
//!
//! Feature-independent (like `dialect.rs`) so `unified_filter` can carry it
//! whether or not the `lua-filter` feature is on.

use std::path::PathBuf;
use std::sync::Arc;

use crate::dialect::SourceDialect;

/// A directory `require("<name>/module")` and `quarto.include` may resolve
/// into by name: an extension's directory, addressed by the extension's
/// name (bd-76ogq4ew, phase 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionRoot {
    /// The name the first path segment of a module name is matched against
    /// (`cards` for `_extensions/cards/`).
    pub name: String,
    /// The extension's directory, absolute.
    pub dir: PathBuf,
}

/// Where the scoped `require` (and `quarto.include`) look after the
/// requiring file's own directory and the running script's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModuleRoots {
    /// The project root: `require("/components/cards")` resolves against
    /// it (the leading-`/` convention `components.src` uses). For a
    /// single-file render the host passes the document's directory.
    pub project: Option<PathBuf>,
    /// Extension directories, by extension name.
    pub extensions: Vec<ExtensionRoot>,
}

impl ModuleRoots {
    /// The extension root named `name`, if any. Later entries win, matching
    /// extension discovery's "user extensions override built-ins" order.
    pub fn extension(&self, name: &str) -> Option<&ExtensionRoot> {
        self.extensions.iter().rev().find(|e| e.name == name)
    }
}

/// Host-provided settings for a Lua environment.
#[derive(Clone, Default)]
pub struct LuaOptions {
    /// Source dialects `require` compiles to Lua, by file extension.
    pub dialects: Vec<Arc<dyn SourceDialect>>,
    /// Module roots beyond the script directories.
    pub module_roots: ModuleRoots,
}

impl LuaOptions {
    /// Dialects only, no extra roots.
    pub fn with_dialects(dialects: Vec<Arc<dyn SourceDialect>>) -> Self {
        Self {
            dialects,
            module_roots: ModuleRoots::default(),
        }
    }
}

impl std::fmt::Debug for LuaOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LuaOptions")
            .field(
                "dialects",
                &self
                    .dialects
                    .iter()
                    .map(|d| d.name().to_string())
                    .collect::<Vec<_>>(),
            )
            .field("module_roots", &self.module_roots)
            .finish()
    }
}
