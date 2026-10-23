/*
 * lua/readwrite.rs
 * Copyright (c) 2025 Posit, PBC
 *
 * Lua bindings for pandoc.read and pandoc.write functions.
 *
 * This module provides Pandoc-compatible API for reading and writing documents
 * in Lua filters. Options are represented as plain Lua tables with metatables
 * for type identification.
 */

use mlua::{Error, Lua, LuaSerdeExt, Result, Table, Value};

use super::pandoc_doc::{peek_pandoc_doc as lua_pandoc_to_rust, push_pandoc_doc};
use crate::options::{
    ParsedFormat, SUPPORTED_READER_FORMATS, SUPPORTED_WRITER_FORMATS, default_reader_options,
    default_writer_options, is_supported_reader_format, is_supported_writer_format,
    merge_with_defaults, normalize_reader_format, normalize_writer_format, parse_format_string,
};

/// The text one `pandoc.read` call parsed, as a virtual file (GH #884).
///
/// Every span in the call's result is rooted at `file_id`. The id is a 53-bit
/// hash minted at call time (the way `MetadataMergeStage` registers config files
/// under `quarto_yaml::file_id_for_filename`), so the Lua side needs no
/// knowledge of the host document's `SourceContext`. A host that drains these
/// with [`take_read_sources`] and registers them ([`register_read_sources`])
/// gets diagnostics that show the string itself under `label`. A host that
/// doesn't leaves the id unknown to its context, so such a diagnostic has no
/// location, rather than one at the string's offsets in the host document
/// (which is what a private context's FileId(0) used to produce).
#[derive(Debug, Clone, PartialEq)]
pub struct ReadSource {
    pub file_id: quarto_source_map::FileId,
    /// Names the call site, e.g. `doc.qmd[pandoc.read, line 14]` (the same
    /// shape as a notebook cell's `notebook.ipynb[cell 2, markdown]`).
    pub label: String,
    pub content: String,
}

/// Lua app data: the files `pandoc.read` minted in this state, in call order,
/// until a host drains them.
#[derive(Default)]
struct ReadSources {
    /// Calls so far, hashed into each id so that two reads of the same text
    /// from the same line (a loop) still get distinct files.
    calls: u64,
    pending: Vec<ReadSource>,
}

/// Drain the files `pandoc.read` minted in `lua` since the last drain.
pub fn take_read_sources(lua: &Lua) -> Vec<ReadSource> {
    lua.app_data_mut::<ReadSources>()
        .map(|mut sources| std::mem::take(&mut sources.pending))
        .unwrap_or_default()
}

/// Register drained `pandoc.read` files in a host context. Ids already
/// present are skipped: pipeline re-entry, or the astronomically unlikely
/// collision of a hash id with a sequential one (`add_file_with_id` would
/// panic on a duplicate).
pub fn register_read_sources(
    context: &mut quarto_source_map::SourceContext,
    sources: &[ReadSource],
) {
    for source in sources {
        if context.get_file(source.file_id).is_none() {
            // lint:allow(add-file-with-id) — hash-minted id; see `ReadSource`
            context.add_file_with_id(
                source.file_id,
                source.label.clone(),
                Some(source.content.clone()),
            );
        }
    }
}

/// Mint the virtual file for one `pandoc.read` call of `content` and
/// record it in `lua`'s pending sources.
fn mint_read_source(lua: &Lua, content: String) -> quarto_source_map::FileId {
    use std::hash::{Hash, Hasher};

    let (chunk, line) = super::diagnostics::get_caller_location(lua);
    // Lua's chunk-name convention is `@path` for a file and `=name` for a
    // literal name; lmd names its cells `@<doc>`, while filters pass their
    // path bare. A multi-line name is an unnamed chunk's own source text.
    let chunk = match chunk.strip_prefix(['@', '=']) {
        Some(name) => name.to_string(),
        None if chunk.contains('\n') => "<string>".to_string(),
        None => chunk,
    };
    let label = format!("{chunk}[pandoc.read, line {line}]");

    if lua.app_data_ref::<ReadSources>().is_none() {
        lua.set_app_data(ReadSources::default());
    }
    let mut sources = lua
        .app_data_mut::<ReadSources>()
        .expect("ReadSources was just installed");
    sources.calls += 1;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    ("pandoc.read", &label, &content, sources.calls).hash(&mut hasher);
    // 53 bits: the id must survive Lua (a node's location passes through a
    // Lua table, whose integers are i64, in `quarto.warn(msg, node)`) and
    // JavaScript (JSON consumers' numbers are exact only to 2^53).
    let file_id = quarto_source_map::FileId((hasher.finish() & ((1 << 53) - 1)) as usize);
    sources.pending.push(ReadSource {
        file_id,
        label,
        content,
    });
    file_id
}

/// Register pandoc.read, pandoc.write, and option constructors on the pandoc table.
pub fn register_pandoc_readwrite(lua: &Lua, pandoc: &Table) -> Result<()> {
    // Register format lists
    register_format_lists(lua, pandoc)?;

    // Register option constructors
    register_option_constructors(lua, pandoc)?;

    // Register pandoc.read
    pandoc.set(
        "read",
        lua.create_function(|lua, args: mlua::MultiValue| pandoc_read(lua, args))?,
    )?;

    // Register pandoc.write
    pandoc.set(
        "write",
        lua.create_function(|lua, args: mlua::MultiValue| pandoc_write(lua, args))?,
    )?;

    Ok(())
}

/// Register pandoc.readers and pandoc.writers format lists.
fn register_format_lists(lua: &Lua, pandoc: &Table) -> Result<()> {
    // pandoc.readers - set of supported reader format names
    let readers = lua.create_table()?;
    for format in SUPPORTED_READER_FORMATS {
        readers.set(*format, true)?;
    }
    pandoc.set("readers", readers)?;

    // pandoc.writers - set of supported writer format names
    let writers = lua.create_table()?;
    for format in SUPPORTED_WRITER_FORMATS {
        writers.set(*format, true)?;
    }
    pandoc.set("writers", writers)?;

    Ok(())
}

/// Register pandoc.ReaderOptions and pandoc.WriterOptions constructors.
fn register_option_constructors(lua: &Lua, pandoc: &Table) -> Result<()> {
    // pandoc.ReaderOptions(opts?) - creates a ReaderOptions table
    pandoc.set(
        "ReaderOptions",
        lua.create_function(|lua, opts: Option<Table>| create_reader_options_table(lua, opts))?,
    )?;

    // pandoc.WriterOptions(opts?) - creates a WriterOptions table
    pandoc.set(
        "WriterOptions",
        lua.create_function(|lua, opts: Option<Table>| create_writer_options_table(lua, opts))?,
    )?;

    Ok(())
}

/// Create a ReaderOptions table with defaults and optional overrides.
pub fn create_reader_options_table(lua: &Lua, opts: Option<Table>) -> Result<Table> {
    // Get default options as serde_json::Value
    let defaults = default_reader_options();

    // Merge with user-provided options if any
    let merged = if let Some(user_table) = opts {
        let user_json: serde_json::Value = lua.from_value(Value::Table(user_table))?;
        merge_with_defaults(defaults, &user_json)
    } else {
        defaults
    };

    // Convert to Lua table
    let lua_val = lua.to_value(&merged)?;
    let table = lua_val
        .as_table()
        .ok_or_else(|| Error::runtime("Failed to create ReaderOptions table"))?
        .clone();

    // Set metatable for type identification
    let mt = lua.create_table()?;
    mt.set("__name", "ReaderOptions")?;
    table.set_metatable(Some(mt))?;

    Ok(table)
}

/// Create a WriterOptions table with defaults and optional overrides.
pub fn create_writer_options_table(lua: &Lua, opts: Option<Table>) -> Result<Table> {
    // Get default options as serde_json::Value
    let defaults = default_writer_options();

    // Merge with user-provided options if any
    let merged = if let Some(user_table) = opts {
        let user_json: serde_json::Value = lua.from_value(Value::Table(user_table))?;
        merge_with_defaults(defaults, &user_json)
    } else {
        defaults
    };

    // Convert to Lua table
    let lua_val = lua.to_value(&merged)?;
    let table = lua_val
        .as_table()
        .ok_or_else(|| Error::runtime("Failed to create WriterOptions table"))?
        .clone();

    // Set metatable for type identification
    let mt = lua.create_table()?;
    mt.set("__name", "WriterOptions")?;
    table.set_metatable(Some(mt))?;

    Ok(table)
}

/// Parse a format specification from a Lua value.
///
/// The format can be:
/// - A string like "markdown" or "markdown+smart-citations"
/// - A table with `format` and optional `extensions` fields
fn parse_format_spec_from_lua(_lua: &Lua, value: Value) -> Result<ParsedFormat> {
    match value {
        Value::String(s) => {
            let spec = s.to_str()?;
            Ok(parse_format_string(&spec))
        }
        Value::Table(t) => {
            // Table form: { format = "markdown", extensions = {...} }
            let base_format: String = t.get("format").unwrap_or_else(|_| "markdown".to_string());

            let mut enable = Vec::new();
            let mut disable = Vec::new();

            if let Ok(ext_table) = t.get::<Table>("extensions") {
                // Extensions can be a list or key-value table
                for pair in ext_table.pairs::<Value, Value>() {
                    let (key, val) = pair?;
                    match key {
                        Value::Integer(_) => {
                            // List form: extensions = {"smart", "footnotes"}
                            if let Value::String(ext_name) = val {
                                enable.push(ext_name.to_str()?.to_string());
                            }
                        }
                        Value::String(ext_name) => {
                            // Key-value form: extensions = { smart = true, citations = false }
                            let ext = ext_name.to_str()?.to_string();
                            match val {
                                Value::Boolean(true) => enable.push(ext),
                                Value::Boolean(false) => disable.push(ext),
                                Value::String(s) if s.to_str()? == "enable" => enable.push(ext),
                                Value::String(s) if s.to_str()? == "disable" => disable.push(ext),
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
            }

            Ok(ParsedFormat {
                base_format,
                extensions: crate::options::ExtensionsDiff { enable, disable },
            })
        }
        Value::Nil => Ok(ParsedFormat {
            base_format: "markdown".to_string(),
            extensions: crate::options::ExtensionsDiff::default(),
        }),
        _ => Err(Error::runtime(
            "format must be a string or table with 'format' field",
        )),
    }
}

// The document <-> Lua conversions live in pandoc_doc.rs (shared with the
// pandoc.Pandoc constructor and the doc metatable); `rust_pandoc_to_lua_table`
// below is retained as the local name at the read call sites.
use push_pandoc_doc as rust_pandoc_to_lua_table;

/// Implementation of pandoc.read(content, format?, reader_options?, read_env?)
///
/// Reads content in the specified format and returns a Pandoc document.
fn pandoc_read(lua: &Lua, args: mlua::MultiValue) -> Result<Value> {
    let mut args_iter = args.into_iter();

    // First argument: content (required)
    let content_val = args_iter
        .next()
        .ok_or_else(|| Error::runtime("pandoc.read requires at least one argument (content)"))?;
    let content = match content_val {
        Value::String(s) => s.as_bytes().to_vec(),
        _ => return Err(Error::runtime("pandoc.read: content must be a string")),
    };

    // Second argument: format (optional, defaults to "markdown")
    let format_val = args_iter.next().unwrap_or(Value::Nil);
    let parsed_format = parse_format_spec_from_lua(lua, format_val)?;

    // Normalize and validate format
    let base_format = normalize_reader_format(&parsed_format.base_format);
    if !is_supported_reader_format(base_format) {
        return Err(Error::runtime(format!(
            "Unsupported reader format: {}. Supported formats: {:?}",
            parsed_format.base_format, SUPPORTED_READER_FORMATS
        )));
    }

    // Third argument: reader_options (optional)
    let reader_options_val = args_iter.next().unwrap_or(Value::Nil);

    // Build options JSON for internal use
    let mut opts_json = serde_json::json!({
        "format": base_format,
        "extensions": {
            "enable": parsed_format.extensions.enable,
            "disable": parsed_format.extensions.disable,
        }
    });

    // Merge reader_options (excluding extensions field per Pandoc behavior)
    if let Value::Table(opts_table) = reader_options_val {
        let user_opts: serde_json::Value = lua.from_value(Value::Table(opts_table))?;
        if let serde_json::Value::Object(user_map) = user_opts
            && let serde_json::Value::Object(ref mut opts_map) = opts_json
        {
            for (key, value) in user_map {
                if key != "extensions" {
                    opts_map.insert(key, value);
                }
            }
        }
    }

    // Fourth argument: read_env (optional, not implemented yet)
    // let _read_env = args_iter.next();

    // Dispatch to the appropriate reader
    match base_format {
        "qmd" => {
            // Use the QMD reader.
            //
            // The reader's output stream is its verbose trace (what
            // `pampa -v` shows), not a diagnostic channel; every pipeline
            // caller sinks it. Pointing it at stderr here is what leaked
            // one "[TOP-LEVEL MISSING NODE]" line per pipe-table delimiter
            // dash into every render that called pandoc.read (GH #847,
            // bd-okfxemnj). The reader's real diagnostics come back as the
            // third tuple element; forwarding those is bd-fwswbxnl.
            let mut trace = crate::utils::output::VerboseOutput::Sink(std::io::sink());
            // Root every span in this call's own virtual file (GH #884), not
            // the reader's private FileId(0), which the host document would
            // read as itself. The reader parses a newline-terminated copy, so
            // register exactly that text.
            let mut text = String::from_utf8_lossy(&content).into_owned();
            if !text.ends_with('\n') {
                text.push('\n');
            }
            let file_id = mint_read_source(lua, text.clone());
            match crate::readers::qmd::read_as_file(
                text.as_bytes(),
                "<pandoc.read>",
                file_id,
                &mut trace,
                true, // prune_errors
            ) {
                Ok((pandoc, _context, warnings)) => {
                    // The reader's warnings are located in this call's text,
                    // so the host can show them there (bd-fwswbxnl).
                    super::diagnostics::push_diagnostics(lua, warnings);
                    rust_pandoc_to_lua_table(lua, &pandoc)
                }
                Err(diagnostics) => {
                    // The Lua error names the problems; the structured
                    // diagnostics, located in the text, go to the host too,
                    // for when the error is caught (`pcall`) or the host
                    // reports the failure alongside them (an lmd cell).
                    let messages: Vec<String> =
                        diagnostics.iter().map(|d| d.title.clone()).collect();
                    super::diagnostics::push_diagnostics(lua, diagnostics);
                    Err(Error::runtime(format!(
                        "pandoc.read failed: {}",
                        messages.join("; ")
                    )))
                }
            }
        }
        "commonmark" => {
            // CommonMark through comrak (GH #885): for strings that were never
            // written as qmd (data-driven prose), where the qmd reader's
            // reserved braces, bare brackets, quote balancing and `$` math
            // would reject or rewrite ordinary text. Spans root in this
            // call's own file, like the qmd arm's. Precision limits are those
            // of `comrak-to-pandoc`: a `Code` span covers its backticks while
            // its text does not, link and image URLs carry no provenance, and
            // a known multi-character entity directly followed by `&` or `\`
            // can misattribute within a token. Extensions are ignored, as for
            // every format here.
            let text = String::from_utf8_lossy(&content).into_owned();
            let file_id = mint_read_source(lua, text.clone());
            let pandoc = crate::readers::commonmark::convert(&text, file_id);
            // The structural desugaring set the other arms get; for
            // CommonMark input that is in practice the implicit figure.
            let mut collector = crate::utils::diagnostic_collector::DiagnosticCollector::new();
            let pandoc = crate::pandoc::desugar::desugar_document(pandoc, &mut collector);
            super::diagnostics::push_diagnostics(lua, collector.into_diagnostics());
            rust_pandoc_to_lua_table(lua, &pandoc)
        }
        "json" => {
            // Lua-side `pandoc.read(json, "json")` — the JSON comes from the
            // filter author and likely lacks q2's `s:` references. Route
            // through the completing reader with `By::unknown()`: nodes
            // without `s:` get stamped with a non-atomic placeholder so
            // they remain editable downstream (plan 7f Phase 4).
            let mut cursor = std::io::Cursor::new(&content);
            match crate::readers::json::read_completing_source_info(
                &mut cursor,
                quarto_source_map::By::unknown(),
            ) {
                Ok((pandoc, _context)) => {
                    // The structural desugaring set (list-table, definition-list,
                    // implicit figure), as for the qmd arm above (where the reader
                    // applies it) and the CLI's `--from json`. Its diagnostics
                    // (e.g. a malformed list-table's Q-2-35; the div is left as
                    // is) go to the host (bd-fwswbxnl).
                    let mut collector =
                        crate::utils::diagnostic_collector::DiagnosticCollector::new();
                    let pandoc = crate::pandoc::desugar::desugar_document(pandoc, &mut collector);
                    super::diagnostics::push_diagnostics(lua, collector.into_diagnostics());
                    rust_pandoc_to_lua_table(lua, &pandoc)
                }
                Err(e) => Err(Error::runtime(format!("pandoc.read (json) failed: {}", e))),
            }
        }
        _ => Err(Error::runtime(format!(
            "Unsupported reader format: {}",
            base_format
        ))),
    }
}

/// Implementation of pandoc.write(doc, format?, writer_options?)
///
/// Writes a Pandoc document to a string in the specified format.
fn pandoc_write(lua: &Lua, args: mlua::MultiValue) -> Result<mlua::String> {
    let mut args_iter = args.into_iter();

    // First argument: doc (required)
    let doc_val = args_iter
        .next()
        .ok_or_else(|| Error::runtime("pandoc.write requires at least one argument (doc)"))?;

    // Convert Lua Pandoc to Rust
    let pandoc = lua_pandoc_to_rust(lua, doc_val)?;

    // Second argument: format (optional, defaults to "html")
    let format_val = args_iter.next().unwrap_or(Value::Nil);
    let parsed_format = if format_val == Value::Nil {
        ParsedFormat {
            base_format: "html".to_string(),
            extensions: crate::options::ExtensionsDiff::default(),
        }
    } else {
        parse_format_spec_from_lua(lua, format_val)?
    };

    // Normalize and validate format
    let base_format = normalize_writer_format(&parsed_format.base_format);
    if !is_supported_writer_format(base_format) {
        return Err(Error::runtime(format!(
            "Unsupported writer format: {}. Supported formats: {:?}",
            parsed_format.base_format, SUPPORTED_WRITER_FORMATS
        )));
    }

    // Third argument: writer_options (optional)
    let writer_options_val = args_iter.next().unwrap_or(Value::Nil);

    // Build options JSON for internal use (unused for now, but stored for future)
    let mut _opts_json = serde_json::json!({
        "format": base_format,
        "extensions": {
            "enable": parsed_format.extensions.enable,
            "disable": parsed_format.extensions.disable,
        }
    });

    // Merge writer_options (excluding extensions field)
    if let Value::Table(opts_table) = writer_options_val {
        let user_opts: serde_json::Value = lua.from_value(Value::Table(opts_table))?;
        if let serde_json::Value::Object(user_map) = user_opts
            && let serde_json::Value::Object(ref mut opts_map) = _opts_json
        {
            for (key, value) in user_map {
                if key != "extensions" {
                    opts_map.insert(key, value);
                }
            }
        }
    }

    // Dispatch to the appropriate writer
    let output = match base_format {
        "html" => {
            // Need an ASTContext for HTML writer (for source tracking support)
            let context = crate::pandoc::ast_context::ASTContext::anonymous();
            let mut buf = Vec::new();
            crate::writers::html::write(&pandoc, &context, &mut buf)
                .map_err(|e| Error::runtime(format!("pandoc.write (html) failed: {}", e)))?;
            String::from_utf8(buf)
                .map_err(|e| Error::runtime(format!("Invalid UTF-8 in output: {}", e)))?
        }
        "json" => {
            // Need an ASTContext for JSON writer
            let context = crate::pandoc::ast_context::ASTContext::anonymous();
            let mut buf = Vec::new();
            crate::writers::json::write(&pandoc, &context, &mut buf).map_err(|e| {
                let messages: Vec<String> = e.iter().map(|d| d.title.clone()).collect();
                Error::runtime(format!(
                    "pandoc.write (json) failed: {}",
                    messages.join("; ")
                ))
            })?;
            String::from_utf8(buf)
                .map_err(|e| Error::runtime(format!("Invalid UTF-8 in output: {}", e)))?
        }
        "native" => {
            let context = crate::pandoc::ast_context::ASTContext::anonymous();
            let mut buf = Vec::new();
            crate::writers::native::write(&pandoc, &context, &mut buf).map_err(|e| {
                let messages: Vec<String> = e.iter().map(|d| d.title.clone()).collect();
                Error::runtime(format!(
                    "pandoc.write (native) failed: {}",
                    messages.join("; ")
                ))
            })?;
            String::from_utf8(buf)
                .map_err(|e| Error::runtime(format!("Invalid UTF-8 in output: {}", e)))?
        }
        "qmd" => {
            let mut buf = Vec::new();
            crate::writers::qmd::write(&pandoc, &mut buf).map_err(|e| {
                let messages: Vec<String> = e.iter().map(|d| d.title.clone()).collect();
                Error::runtime(format!(
                    "pandoc.write (qmd) failed: {}",
                    messages.join("; ")
                ))
            })?;
            String::from_utf8(buf)
                .map_err(|e| Error::runtime(format!("Invalid UTF-8 in output: {}", e)))?
        }
        "plain" => {
            // plaintext writer works on blocks, so we write them directly
            let mut buf = Vec::new();
            let mut ctx = crate::writers::plaintext::PlainTextWriterContext::new();
            crate::writers::plaintext::write_blocks(&pandoc.blocks, &mut buf, &mut ctx)
                .map_err(|e| Error::runtime(format!("pandoc.write (plain) failed: {}", e)))?;
            String::from_utf8(buf)
                .map_err(|e| Error::runtime(format!("Invalid UTF-8 in output: {}", e)))?
        }
        _ => {
            return Err(Error::runtime(format!(
                "Unsupported writer format: {}",
                base_format
            )));
        }
    };

    lua.create_string(&output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_format_string_simple() {
        let parsed = parse_format_string("markdown");
        assert_eq!(parsed.base_format, "markdown");
        assert!(parsed.extensions.enable.is_empty());
        assert!(parsed.extensions.disable.is_empty());
    }

    #[test]
    fn test_parse_format_string_with_extensions() {
        let parsed = parse_format_string("markdown+smart-citations");
        assert_eq!(parsed.base_format, "markdown");
        assert_eq!(parsed.extensions.enable, vec!["smart"]);
        assert_eq!(parsed.extensions.disable, vec!["citations"]);
    }

    #[test]
    fn test_reader_options_default() {
        let lua = Lua::new();
        let opts = create_reader_options_table(&lua, None).unwrap();

        // Check default values
        assert_eq!(opts.get::<i64>("columns").unwrap(), 80);
        assert_eq!(opts.get::<i64>("tab_stop").unwrap(), 4);
        assert!(!opts.get::<bool>("standalone").unwrap());

        // Check metatable
        let mt = opts.metatable().unwrap();
        assert_eq!(mt.get::<String>("__name").unwrap(), "ReaderOptions");
    }

    #[test]
    fn test_writer_options_default() {
        let lua = Lua::new();
        let opts = create_writer_options_table(&lua, None).unwrap();

        // Check default values
        assert_eq!(opts.get::<i64>("columns").unwrap(), 72);
        assert_eq!(opts.get::<i64>("dpi").unwrap(), 96);
        assert!(!opts.get::<bool>("number_sections").unwrap());

        // Check metatable
        let mt = opts.metatable().unwrap();
        assert_eq!(mt.get::<String>("__name").unwrap(), "WriterOptions");
    }

    #[test]
    fn test_reader_options_with_overrides() {
        let lua = Lua::new();
        let user_opts = lua.create_table().unwrap();
        user_opts.set("columns", 120).unwrap();
        user_opts.set("standalone", true).unwrap();

        let opts = create_reader_options_table(&lua, Some(user_opts)).unwrap();

        assert_eq!(opts.get::<i64>("columns").unwrap(), 120);
        assert!(opts.get::<bool>("standalone").unwrap());
        // Other defaults should still be present
        assert_eq!(opts.get::<i64>("tab_stop").unwrap(), 4);
    }

    #[test]
    fn test_register_format_lists() {
        let lua = Lua::new();
        let pandoc = lua.create_table().unwrap();
        register_format_lists(&lua, &pandoc).unwrap();

        // Check readers
        let readers: Table = pandoc.get("readers").unwrap();
        assert!(readers.get::<bool>("qmd").unwrap());
        assert!(readers.get::<bool>("markdown").unwrap());
        assert!(readers.get::<bool>("json").unwrap());

        // Check writers
        let writers: Table = pandoc.get("writers").unwrap();
        assert!(writers.get::<bool>("html").unwrap());
        assert!(writers.get::<bool>("json").unwrap());
        assert!(writers.get::<bool>("native").unwrap());
        assert!(writers.get::<bool>("qmd").unwrap());
        assert!(writers.get::<bool>("plain").unwrap());
    }

    #[test]
    fn test_pandoc_read_markdown() {
        let lua = Lua::new();
        let pandoc = lua.create_table().unwrap();
        register_pandoc_readwrite(&lua, &pandoc).unwrap();
        lua.globals().set("pandoc", pandoc).unwrap();

        // Read simple markdown
        let result: Value = lua
            .load(r#"return pandoc.read("Hello *world*", "markdown")"#)
            .eval()
            .unwrap();

        // Check it's a table with blocks
        let doc = result.as_table().unwrap();
        let blocks: Table = doc.get("blocks").unwrap();
        assert!(blocks.raw_len() > 0);
    }

    #[test]
    fn test_pandoc_write_html() {
        let lua = Lua::new();

        // We need the full pandoc namespace for read/write
        super::super::constructors::register_pandoc_namespace(
            &lua,
            std::sync::Arc::new(super::super::runtime::NativeRuntime::new()),
            super::super::mediabag::create_shared_mediabag(),
        )
        .unwrap();

        // Create a simple document via pandoc.read and write it as HTML
        let result: String = lua
            .load(
                r#"
                local doc = pandoc.read("Hello", "markdown")
                return pandoc.write(doc, "html")
            "#,
            )
            .eval()
            .unwrap();

        assert!(result.contains("<p>"));
        assert!(result.contains("Hello"));
    }

    #[test]
    fn test_pandoc_read_write_roundtrip() {
        let lua = Lua::new();

        // Register full namespace
        super::super::constructors::register_pandoc_namespace(
            &lua,
            std::sync::Arc::new(super::super::runtime::NativeRuntime::new()),
            super::super::mediabag::create_shared_mediabag(),
        )
        .unwrap();

        // Read markdown, write as JSON, read back, write as markdown
        let result: String = lua
            .load(
                "
                local doc = pandoc.read('# Hello\\n\\nWorld', 'markdown')
                local json = pandoc.write(doc, 'json')
                local doc2 = pandoc.read(json, 'json')
                return pandoc.write(doc2, 'qmd')
            ",
            )
            .eval()
            .unwrap();

        assert!(result.contains("Hello"));
        assert!(result.contains("World"));
    }

    /// bd-7gqr148v: the json arm applies the structural desugaring set, like
    /// the qmd arm (through the reader) and the CLI's `--from json`.
    #[test]
    fn pandoc_read_json_applies_the_structural_desugaring_set() {
        let lua = Lua::new();
        super::super::constructors::register_pandoc_namespace(
            &lua,
            std::sync::Arc::new(super::super::runtime::NativeRuntime::new()),
            super::super::mediabag::create_shared_mediabag(),
        )
        .unwrap();

        let kinds: Vec<String> = lua
            .load(
                r#"
                local function cell(s) return { pandoc.Plain({ pandoc.Str(s) }) } end
                local rows = pandoc.BulletList({
                  { pandoc.BulletList({ cell("k"), cell("v") }) },
                  { pandoc.BulletList({ cell("a"), cell("1") }) },
                })
                local doc = pandoc.Pandoc({
                  pandoc.Div({ rows }, pandoc.Attr("", {"list-table"})),
                  pandoc.Div({ pandoc.BulletList({ { pandoc.Plain({ pandoc.Str("t") }),
                                                     pandoc.BulletList({ cell("d") }) } }) },
                             pandoc.Attr("", {"definition-list"})),
                  pandoc.Para({ pandoc.Image({ pandoc.Str("alt") }, "x.png") }),
                })
                local back = pandoc.read(pandoc.write(doc, "json"), "json")
                local out = {}
                for _, b in ipairs(back.blocks) do table.insert(out, b.t) end
                return out
            "#,
            )
            .eval()
            .unwrap();

        assert_eq!(kinds, vec!["Table", "DefinitionList", "Figure"]);
    }

    /// bd-3h6iasmz: a list built with default attributes is written with
    /// pandoc's names for them, so pandoc accepts the JSON and the native
    /// output, and pampa reads its own JSON back.
    #[test]
    fn default_list_attributes_are_written_with_pandoc_names() {
        let lua = Lua::new();
        super::super::constructors::register_pandoc_namespace(
            &lua,
            std::sync::Arc::new(super::super::runtime::NativeRuntime::new()),
            super::super::mediabag::create_shared_mediabag(),
        )
        .unwrap();

        let (json, native, back_style, back_delim): (String, String, String, String) = lua
            .load(
                r#"
                local doc = pandoc.Pandoc({
                  pandoc.OrderedList({ { pandoc.Plain({ pandoc.Str("a") }) } }),
                })
                local json = pandoc.write(doc, "json")
                local back = pandoc.read(json, "json").blocks[1].listAttributes
                return json, pandoc.write(doc, "native"), back.style, back.delimiter
            "#,
            )
            .eval()
            .unwrap();

        assert!(json.contains(r#"{"t":"DefaultStyle"}"#), "{json}");
        assert!(json.contains(r#"{"t":"DefaultDelim"}"#), "{json}");
        assert!(!json.contains(r#""Default""#), "{json}");
        assert!(native.contains("DefaultStyle"), "{native}");
        assert!(native.contains("DefaultDelim"), "{native}");
        assert_eq!(
            (back_style.as_str(), back_delim.as_str()),
            ("DefaultStyle", "DefaultDelim")
        );
    }

    #[test]
    fn test_pandoc_write_native() {
        let lua = Lua::new();

        super::super::constructors::register_pandoc_namespace(
            &lua,
            std::sync::Arc::new(super::super::runtime::NativeRuntime::new()),
            super::super::mediabag::create_shared_mediabag(),
        )
        .unwrap();

        let result: String = lua
            .load(
                r#"
                local doc = pandoc.read("Test", "markdown")
                return pandoc.write(doc, "native")
            "#,
            )
            .eval()
            .unwrap();

        // Native format outputs blocks in Haskell-like syntax
        assert!(result.contains("Para"));
        assert!(result.contains("Str"));
        assert!(result.contains("Test"));
    }

    #[test]
    fn test_unsupported_reader_format() {
        let lua = Lua::new();
        let pandoc = lua.create_table().unwrap();
        register_pandoc_readwrite(&lua, &pandoc).unwrap();
        lua.globals().set("pandoc", pandoc).unwrap();

        let result: Result<Value> = lua.load(r#"return pandoc.read("test", "docx")"#).eval();

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Unsupported reader format"));
    }

    #[test]
    fn test_unsupported_writer_format() {
        let lua = Lua::new();

        super::super::constructors::register_pandoc_namespace(
            &lua,
            std::sync::Arc::new(super::super::runtime::NativeRuntime::new()),
            super::super::mediabag::create_shared_mediabag(),
        )
        .unwrap();

        let result: Result<Value> = lua
            .load(
                r#"
                local doc = pandoc.read("test", "markdown")
                return pandoc.write(doc, "docx")
            "#,
            )
            .eval();

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Unsupported writer format"));
    }

    #[test]
    fn test_parse_format_spec_from_lua_table_form() {
        let lua = Lua::new();

        // Test table form with extensions
        let format_table = lua.create_table().unwrap();
        format_table.set("format", "markdown").unwrap();

        let extensions = lua.create_table().unwrap();
        extensions.set(1, "smart").unwrap();
        extensions.set(2, "footnotes").unwrap();
        format_table.set("extensions", extensions).unwrap();

        let parsed = parse_format_spec_from_lua(&lua, Value::Table(format_table)).unwrap();
        assert_eq!(parsed.base_format, "markdown");
        assert!(parsed.extensions.enable.contains(&"smart".to_string()));
    }

    #[test]
    fn test_parse_format_spec_from_lua_table_key_value_extensions() {
        let lua = Lua::new();

        // Test table form with key-value extensions
        let format_table = lua.create_table().unwrap();
        format_table.set("format", "markdown").unwrap();

        let extensions = lua.create_table().unwrap();
        extensions.set("smart", true).unwrap();
        extensions.set("citations", false).unwrap();
        format_table.set("extensions", extensions).unwrap();

        let parsed = parse_format_spec_from_lua(&lua, Value::Table(format_table)).unwrap();
        assert!(parsed.extensions.enable.contains(&"smart".to_string()));
        assert!(parsed.extensions.disable.contains(&"citations".to_string()));
    }

    #[test]
    fn test_parse_format_spec_from_lua_table_string_extensions() {
        let lua = Lua::new();

        // Test table form with string extensions ("enable"/"disable")
        let format_table = lua.create_table().unwrap();
        format_table.set("format", "markdown").unwrap();

        let extensions = lua.create_table().unwrap();
        extensions.set("smart", "enable").unwrap();
        extensions.set("citations", "disable").unwrap();
        format_table.set("extensions", extensions).unwrap();

        let parsed = parse_format_spec_from_lua(&lua, Value::Table(format_table)).unwrap();
        assert!(parsed.extensions.enable.contains(&"smart".to_string()));
        assert!(parsed.extensions.disable.contains(&"citations".to_string()));
    }

    #[test]
    fn test_parse_format_spec_from_lua_nil() {
        let lua = Lua::new();

        let parsed = parse_format_spec_from_lua(&lua, Value::Nil).unwrap();
        assert_eq!(parsed.base_format, "markdown");
        assert!(parsed.extensions.enable.is_empty());
    }

    #[test]
    fn test_parse_format_spec_from_lua_invalid() {
        let lua = Lua::new();

        let result = parse_format_spec_from_lua(&lua, Value::Integer(42));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("format must be a string or table"));
    }

    #[test]
    fn test_writer_options_with_overrides() {
        let lua = Lua::new();
        let user_opts = lua.create_table().unwrap();
        user_opts.set("columns", 100).unwrap();
        user_opts.set("number_sections", true).unwrap();

        let opts = create_writer_options_table(&lua, Some(user_opts)).unwrap();

        assert_eq!(opts.get::<i64>("columns").unwrap(), 100);
        assert!(opts.get::<bool>("number_sections").unwrap());
        // Other defaults should still be present
        assert_eq!(opts.get::<i64>("dpi").unwrap(), 96);
    }

    #[test]
    fn test_pandoc_write_plain() {
        let lua = Lua::new();

        super::super::constructors::register_pandoc_namespace(
            &lua,
            std::sync::Arc::new(super::super::runtime::NativeRuntime::new()),
            super::super::mediabag::create_shared_mediabag(),
        )
        .unwrap();

        let result: String = lua
            .load(
                r#"
                local doc = pandoc.read("Hello *world*", "markdown")
                return pandoc.write(doc, "plain")
            "#,
            )
            .eval()
            .unwrap();

        assert!(result.contains("Hello"));
        assert!(result.contains("world"));
    }

    #[test]
    fn test_pandoc_write_qmd() {
        let lua = Lua::new();

        super::super::constructors::register_pandoc_namespace(
            &lua,
            std::sync::Arc::new(super::super::runtime::NativeRuntime::new()),
            super::super::mediabag::create_shared_mediabag(),
        )
        .unwrap();

        let result: String = lua
            .load(
                "
                local doc = pandoc.read('# Title\\n\\nParagraph', 'markdown')
                return pandoc.write(doc, 'qmd')
            ",
            )
            .eval()
            .unwrap();

        assert!(result.contains("Title"));
        assert!(result.contains("Paragraph"));
    }

    #[test]
    fn test_pandoc_read_with_reader_options() {
        let lua = Lua::new();
        let pandoc = lua.create_table().unwrap();
        register_pandoc_readwrite(&lua, &pandoc).unwrap();
        lua.globals().set("pandoc", pandoc).unwrap();

        // Read with reader options
        let result: Value = lua
            .load(
                r#"
                local opts = { columns = 100, standalone = true }
                return pandoc.read("Hello", "markdown", opts)
            "#,
            )
            .eval()
            .unwrap();

        assert!(result.as_table().is_some());
    }

    #[test]
    fn test_pandoc_write_with_writer_options() {
        let lua = Lua::new();

        super::super::constructors::register_pandoc_namespace(
            &lua,
            std::sync::Arc::new(super::super::runtime::NativeRuntime::new()),
            super::super::mediabag::create_shared_mediabag(),
        )
        .unwrap();

        let result: String = lua
            .load(
                r#"
                local doc = pandoc.read("Test", "markdown")
                local opts = { columns = 100 }
                return pandoc.write(doc, "html", opts)
            "#,
            )
            .eval()
            .unwrap();

        assert!(result.contains("Test"));
    }

    #[test]
    fn test_pandoc_read_missing_content() {
        let lua = Lua::new();
        let pandoc = lua.create_table().unwrap();
        register_pandoc_readwrite(&lua, &pandoc).unwrap();
        lua.globals().set("pandoc", pandoc).unwrap();

        let result: Result<Value> = lua.load("return pandoc.read()").eval();

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("requires at least one argument"));
    }

    #[test]
    fn test_pandoc_read_non_string_content() {
        let lua = Lua::new();
        let pandoc = lua.create_table().unwrap();
        register_pandoc_readwrite(&lua, &pandoc).unwrap();
        lua.globals().set("pandoc", pandoc).unwrap();

        let result: Result<Value> = lua.load("return pandoc.read(123)").eval();

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("content must be a string"));
    }

    #[test]
    fn test_pandoc_write_missing_doc() {
        let lua = Lua::new();

        super::super::constructors::register_pandoc_namespace(
            &lua,
            std::sync::Arc::new(super::super::runtime::NativeRuntime::new()),
            super::super::mediabag::create_shared_mediabag(),
        )
        .unwrap();

        let result: Result<Value> = lua.load("return pandoc.write()").eval();

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("requires at least one argument"));
    }

    #[test]
    fn test_pandoc_write_non_table_doc() {
        let lua = Lua::new();

        super::super::constructors::register_pandoc_namespace(
            &lua,
            std::sync::Arc::new(super::super::runtime::NativeRuntime::new()),
            super::super::mediabag::create_shared_mediabag(),
        )
        .unwrap();

        let result: Result<Value> = lua.load("return pandoc.write('not a table')").eval();

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Expected Pandoc document table"));
    }

    #[test]
    fn test_pandoc_write_default_format() {
        let lua = Lua::new();

        super::super::constructors::register_pandoc_namespace(
            &lua,
            std::sync::Arc::new(super::super::runtime::NativeRuntime::new()),
            super::super::mediabag::create_shared_mediabag(),
        )
        .unwrap();

        // No format specified, should default to HTML
        let result: String = lua
            .load(
                r#"
                local doc = pandoc.read("Test", "markdown")
                return pandoc.write(doc)
            "#,
            )
            .eval()
            .unwrap();

        assert!(result.contains("<p>"));
    }

    #[test]
    fn test_pandoc_read_meta_is_native_shape() {
        // Modern-pandoc parity: doc.meta values are native Lua values
        // (Inlines userdata for markdown-parsed strings, numbers stay
        // numbers), not the legacy {t="MetaInlines", ...} tagged tables.
        let lua = Lua::new();
        super::super::constructors::register_pandoc_namespace(
            &lua,
            std::sync::Arc::new(super::super::runtime::NativeRuntime::new()),
            super::super::mediabag::create_shared_mediabag(),
        )
        .unwrap();

        let (title_type, meta_type, count, strong_text): (String, String, i64, String) = lua
            .load(
                r#"
                local doc = pandoc.read("---\ntitle: '*Hi*'\ncount: 3\n---\n\ntext", "qmd")
                local title = doc.meta.title
                return pandoc.utils.type(title),
                       pandoc.utils.type(doc.meta),
                       doc.meta.count,
                       title[1].content[1].text
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(title_type, "Inlines");
        assert_eq!(meta_type, "Meta");
        assert_eq!(count, 3);
        assert_eq!(strong_text, "Hi");
    }

    #[test]
    fn test_pandoc_write_accepts_missing_meta() {
        // A hand-built doc table without a meta field writes fine
        // (nil meta -> empty map at the boundary).
        let lua = Lua::new();
        super::super::constructors::register_pandoc_namespace(
            &lua,
            std::sync::Arc::new(super::super::runtime::NativeRuntime::new()),
            super::super::mediabag::create_shared_mediabag(),
        )
        .unwrap();

        let result: String = lua
            .load(
                r#"
                local doc = { blocks = { pandoc.Para('Hello') } }
                return pandoc.write(doc, "html")
            "#,
            )
            .eval()
            .unwrap();
        assert!(result.contains("Hello"));
    }

    #[test]
    fn test_pandoc_write_meta_round_trip_native_shape() {
        // Meta mutated in native shape survives a write: set a plain
        // string and check it lands in the JSON output as ConfigValue.
        let lua = Lua::new();
        super::super::constructors::register_pandoc_namespace(
            &lua,
            std::sync::Arc::new(super::super::runtime::NativeRuntime::new()),
            super::super::mediabag::create_shared_mediabag(),
        )
        .unwrap();

        let result: String = lua
            .load(
                r#"
                local doc = pandoc.read("hello", "qmd")
                doc.meta.subtitle = 'plain subtitle'
                doc.meta.priority = 7
                return pandoc.write(doc, "json")
            "#,
            )
            .eval()
            .unwrap();
        assert!(result.contains("subtitle"), "json output: {}", result);
        assert!(result.contains("plain subtitle"), "json output: {}", result);
        assert!(result.contains("priority"), "json output: {}", result);
    }

    #[test]
    fn test_pandoc_read_json() {
        let lua = Lua::new();
        let pandoc = lua.create_table().unwrap();
        register_pandoc_readwrite(&lua, &pandoc).unwrap();
        lua.globals().set("pandoc", pandoc).unwrap();

        // Valid JSON format (simplified Pandoc JSON)
        let json_doc = r#"{"blocks":[{"t":"Para","c":[{"t":"Str","c":"Hello"}]}],"pandoc-api-version":[1,23],"meta":{}}"#;

        let code = format!(r#"return pandoc.read('{}', "json")"#, json_doc);
        let result: Value = lua.load(&code).eval().unwrap();

        assert!(result.as_table().is_some());
    }

    #[test]
    fn test_pandoc_write_json() {
        let lua = Lua::new();

        super::super::constructors::register_pandoc_namespace(
            &lua,
            std::sync::Arc::new(super::super::runtime::NativeRuntime::new()),
            super::super::mediabag::create_shared_mediabag(),
        )
        .unwrap();

        let result: String = lua
            .load(
                r#"
                local doc = pandoc.read("Test", "markdown")
                return pandoc.write(doc, "json")
            "#,
            )
            .eval()
            .unwrap();

        // Should be valid JSON
        assert!(result.contains("blocks"));
        assert!(result.contains("meta"));
    }

    #[test]
    fn test_option_constructors_via_lua() {
        let lua = Lua::new();
        let pandoc = lua.create_table().unwrap();
        register_pandoc_readwrite(&lua, &pandoc).unwrap();
        lua.globals().set("pandoc", pandoc).unwrap();

        // Test ReaderOptions constructor
        let result: Table = lua
            .load("return pandoc.ReaderOptions({ columns = 100 })")
            .eval()
            .unwrap();
        assert_eq!(result.get::<i64>("columns").unwrap(), 100);

        // Test WriterOptions constructor
        let result: Table = lua
            .load("return pandoc.WriterOptions({ dpi = 150 })")
            .eval()
            .unwrap();
        assert_eq!(result.get::<i64>("dpi").unwrap(), 150);
    }

    #[test]
    fn test_format_table_without_format_field() {
        let lua = Lua::new();

        // Table without explicit format should default to markdown
        let format_table = lua.create_table().unwrap();
        let extensions = lua.create_table().unwrap();
        extensions.set("smart", true).unwrap();
        format_table.set("extensions", extensions).unwrap();

        let parsed = parse_format_spec_from_lua(&lua, Value::Table(format_table)).unwrap();
        assert_eq!(parsed.base_format, "markdown");
    }

    /// The file ids every `Original` in a `pandoc.read` result is rooted in,
    /// read off the JSON writer's source-info pool (`t == 0` entries carry
    /// their file id in `d`), which holds every span the writer emits,
    /// including the parents of `Substring`s and the pieces of `Concat`s.
    ///
    /// The zero-length `Original(FileId(0), 0, 0)` is skipped: it is
    /// `location::empty_source_info()`, the reader-wide placeholder for a
    /// node with no source of its own (e.g. a list table's absent caption),
    /// not a span into this call's text.
    fn original_file_ids_of_read(lua: &Lua, chunk: &str) -> Vec<u64> {
        let value: Value = lua.load(chunk).eval().unwrap();
        let doc = lua_pandoc_to_rust(lua, value).unwrap();
        let mut buf = Vec::new();
        crate::writers::json::write_with_config(
            &doc,
            &crate::pandoc::ast_context::ASTContext::anonymous(),
            &mut buf,
            &crate::writers::json::JsonConfig::default(),
        )
        .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        let pool = json["astContext"]["p"]
            .as_array()
            .expect("the writer emits a source-info pool");
        let mut ids: Vec<u64> = pool
            .iter()
            .filter(|entry| entry["t"] == 0)
            .filter(|entry| !(entry["d"] == 0 && entry["r"] == serde_json::json!([0, 0])))
            .map(|entry| entry["d"].as_u64().expect("Original carries its file id"))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// GH #884: no span in a `pandoc.read` result may be rooted at
    /// FileId(0), which in the host document is the document itself. Every
    /// span roots in the one virtual file the call minted. The fixture
    /// exercises the reader's node kinds broadly, so a reader path that
    /// stamps a hard-coded FileId(0) instead of rerooting through
    /// `parent_source_info` shows up here.
    #[test]
    fn pandoc_read_spans_root_in_the_call_s_own_file() {
        let lua = Lua::new();
        let pandoc = lua.create_table().unwrap();
        register_pandoc_readwrite(&lua, &pandoc).unwrap();
        lua.globals().set("pandoc", pandoc).unwrap();

        let ids = original_file_ids_of_read(
            &lua,
            r#"return pandoc.read([==[
# Heading {#hid .cls}

Para with *emph*, **strong**, `code`{.x}, $x^2$, "quoted", a soft
break, a hard\
break, [link](http://a.b "title"){.c}, ![img](i.png){width=10},
[span]{.s key="v"}, ~~del~~, ^sup^, ~sub~, a note^[inline note], and
a reference note[^1].

[^1]: The note.

- item one
- item two

1. ordered
2. list

Term
: Definition

> quoted block

| a | b |
|---|---|
| 1 | 2 |

::: {.callout-note}
Inside a div.
:::

```python
print(1)
```

::: {.list-table}
* - a
  - b
:::
]==], "markdown")"#,
        );

        assert!(
            !ids.is_empty(),
            "the result should carry Original spans at all"
        );
        assert!(
            !ids.contains(&0),
            "a span is rooted at FileId(0), the host document's id: {ids:?}"
        );
        assert_eq!(
            ids.len(),
            1,
            "every span should root in the one file this call minted: {ids:?}"
        );
    }

    /// Two reads (even from the same line) mint distinct files, so their
    /// spans never alias each other once registered.
    #[test]
    fn two_pandoc_reads_mint_distinct_files() {
        let lua = Lua::new();
        let pandoc = lua.create_table().unwrap();
        register_pandoc_readwrite(&lua, &pandoc).unwrap();
        lua.globals().set("pandoc", pandoc).unwrap();

        let first = original_file_ids_of_read(&lua, r#"return pandoc.read("same", "markdown")"#);
        let second = original_file_ids_of_read(&lua, r#"return pandoc.read("same", "markdown")"#);
        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);
        assert_ne!(first, second);
    }

    /// A Lua state with the full `pandoc` and `quarto` namespaces, as a
    /// filter sees them.
    fn filter_lua() -> Lua {
        let lua = Lua::new();
        super::super::constructors::register_pandoc_namespace(
            &lua,
            std::sync::Arc::new(super::super::runtime::NativeRuntime::new()),
            super::super::mediabag::create_shared_mediabag(),
        )
        .unwrap();
        super::super::diagnostics::register_quarto_namespace(&lua).unwrap();
        lua
    }

    /// bd-fwswbxnl: the reader's own warnings reach the host, located in
    /// the call's text.
    #[test]
    fn pandoc_read_forwards_the_reader_s_warnings() {
        let lua = filter_lua();
        lua.load(r#"pandoc.read("Term\n: Definition", "markdown")"#)
            .exec()
            .unwrap();

        let diags = super::super::diagnostics::extract_lua_diagnostics(&lua).unwrap();
        let sources = take_read_sources(&lua);
        let q_2_54: Vec<_> = diags
            .iter()
            .filter(|d| d.code.as_deref() == Some("Q-2-54"))
            .collect();
        assert_eq!(q_2_54.len(), 1, "{diags:#?}");
        let (file, _, _) = q_2_54[0]
            .location
            .as_ref()
            .and_then(|l| l.resolve_byte_range())
            .expect("located");
        assert_eq!(sources.len(), 1);
        assert_eq!(file, sources[0].file_id.0);
    }

    /// bd-fwswbxnl: the json arm's desugaring diagnostics (here Q-2-35, a
    /// list-table div that is not a list of lists) reach the host too.
    #[test]
    fn pandoc_read_json_forwards_desugaring_diagnostics() {
        let lua = filter_lua();
        lua.load(
            r#"
            local doc = pandoc.Pandoc({
              pandoc.Div({ pandoc.Para({ pandoc.Str("x") }) }, pandoc.Attr("", {"list-table"})),
            })
            pandoc.read(pandoc.write(doc, "json"), "json")
        "#,
        )
        .exec()
        .unwrap();

        let diags = super::super::diagnostics::extract_lua_diagnostics(&lua).unwrap();
        assert!(
            diags.iter().any(|d| d.code.as_deref() == Some("Q-2-35")),
            "{diags:#?}"
        );
    }

    /// GH #885: `commonmark` reads ordinary prose that the qmd reader rejects
    /// or rewrites (reserved braces, `$`, bare brackets) as the text it is.
    #[test]
    fn pandoc_read_commonmark_keeps_prose_qmd_rejects() {
        let lua = filter_lua();
        for text in [
            "Set {env:VAR} first.",
            "Uses $HOME and $PATH.",
            r#"Allowed kinds: ["skills", "commands"] only."#,
        ] {
            lua.globals().set("SRC", text).unwrap();
            let out: String = lua
                .load("return pandoc.utils.stringify(pandoc.read(SRC, 'commonmark').blocks)")
                .eval()
                .unwrap_or_else(|e| panic!("{text:?}: {e}"));
            assert_eq!(out, text);
        }
        // Raw HTML in CommonMark, an error in qmd: it must at least read.
        lua.load(r#"pandoc.read('Call it "<name>".', 'commonmark')"#)
            .exec()
            .unwrap();
    }

    /// GH #885: commonmark-read spans root in the call's own file, like the
    /// qmd arm's (no FileId(0), one minted id).
    #[test]
    fn pandoc_read_commonmark_spans_root_in_the_call_s_own_file() {
        let lua = filter_lua();
        let ids = original_file_ids_of_read(
            &lua,
            r#"return pandoc.read([==[
# Heading

Para with *emph*, **strong**, `code`, a [link](http://a.b "t"),
![img](i.png), an &amp; entity, a hard\
break and <https://auto.link>.

- item
- item

> quote

```python
x = 1
```
]==], "commonmark")"#,
        );
        assert!(!ids.is_empty());
        assert!(!ids.contains(&0), "{ids:?}");
        assert_eq!(ids.len(), 1, "{ids:?}");
    }

    /// GH #885: `commonmark` is a listed reader, and its result gets the
    /// structural desugaring the other arms apply (here, an implicit figure).
    #[test]
    fn pandoc_read_commonmark_is_listed_and_desugared() {
        let lua = filter_lua();
        let listed: bool = lua.load("return pandoc.readers.commonmark").eval().unwrap();
        assert!(listed);
        let kind: String = lua
            .load("return pandoc.read('![alt](x.png)', 'commonmark').blocks[1].t")
            .eval()
            .unwrap();
        assert_eq!(kind, "Figure");
    }

    /// Minted ids survive the trip through Lua: `quarto.warn(msg, node)`
    /// serializes the node's location into a Lua table, whose integers are
    /// i64, and the JSON writer's ids reach JavaScript, whose numbers are
    /// exact only to 2^53. Many reads, so that a full-width hash (half of
    /// which exceed i64::MAX) cannot pass by luck.
    #[test]
    fn minted_ids_survive_quarto_warn() {
        let lua = filter_lua();
        lua.load(
            r#"
            for i = 1, 64 do
              local doc = pandoc.read("text number " .. i, "commonmark")
              quarto.warn("w" .. i, doc.blocks[1])
            end
        "#,
        )
        .exec()
        .unwrap();

        let sources = take_read_sources(&lua);
        let diags = super::super::diagnostics::extract_lua_diagnostics(&lua).unwrap();
        assert_eq!(sources.len(), 64);
        assert_eq!(diags.len(), 64);
        for (source, diag) in sources.iter().zip(&diags) {
            assert!(source.file_id.0 < (1 << 53), "{:?}", source.file_id);
            let (file, _, _) = diag
                .location
                .as_ref()
                .and_then(|l| l.resolve_byte_range())
                .unwrap_or_else(|| panic!("{} lost its location", diag.title));
            assert_eq!(file, source.file_id.0, "{}", diag.title);
        }
    }
}
