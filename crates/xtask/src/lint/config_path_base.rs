//! Lint rule: don't pick a base directory for a config-authored path.
//!
//! A path written in YAML (`_quarto.yml`, a `_metadata.yml`, front
//! matter, an extension manifest) is relative to the file that declared
//! it; a leading `/` means the project root
//! (`claude-notes/designs/path-resolution-model.md`). After the metadata
//! merge no consumer can know which file that was from the string alone,
//! so a consumer that joins the string onto a directory of its own
//! choosing (`doc_dir.join(raw)`, `project_dir.join(&spec)`,
//! `Path::new(&raw)`, a runtime read of the bare string) is wrong for
//! every declarer but one. This was the class behind GH #945 and the
//! 2026-10-28 consumer sweep (bd-nqq41zgx, bd-lwvumpfk); bd-40luf359
//! added this lint.
//!
//! The safe pattern is `quarto_core::config_paths::resolve_config_path`
//! (or one of the resolvers built on it), which reads the value's own
//! provenance.
//!
//! ## What this rule flags
//!
//! Within one function body, a local bound from a **metadata read** is
//! *tainted*:
//!
//! ```ignore
//! let raw = meta.get("template").and_then(|v| v.as_plain_text())?;   // tainted
//! let spec = raw.trim().to_string();                                  // tainted (derived)
//! ```
//!
//! A metadata read is a `.get(..)` / `.get_path(..)` on an expression
//! whose final identifier is `meta` / `metadata` (the convention
//! `metadata-as-str` also keys on), or any `.as_plain_text()` /
//! `.as_str()` call. Two more sources cover the shapes the 2026-10-28
//! sweep actually found:
//!
//! - **a string parameter beside a directory parameter** — a helper
//!   `fn probe(raw: &str, doc_dir: &Path)` is the ad-hoc resolver shape
//!   itself, so every string-typed parameter (`&str`, `String`,
//!   `Option<&str>`, `&[String]`, …) of a function that also takes a
//!   `Path` / `PathBuf` is tainted;
//! - **closure parameters** of a method call whose receiver is a
//!   metadata read or mentions a tainted local
//!   (`value.as_plain_text().map(|s| doc_dir.join(s))`,
//!   `partials.iter().map(|p| dir.join(p))`).
//!
//! Taint propagates through `let`s that mention a tainted local, and is
//! **cleared** by a `let` whose initializer calls a resolver — a function
//! or method whose name starts with `resolve`, or one of `pivot_form`,
//! `declaring_dir`, `page_url_for`, `base_dir_for`, `file_base_dir` —
//! since the resolver owns the base-directory decision.
//!
//! A violation is a tainted local reaching one of these sinks:
//!
//! ```ignore
//! dir.join(raw)            // any `.join(..)` whose argument mentions a tainted local
//! PathBuf::from(&raw)      // conversion to an owned path
//! Path::new(&raw)          // conversion to a borrowed path
//! runtime.is_file(&raw)    // a filesystem probe/read of the bare string
//! ```
//!
//! Filesystem sinks: `is_file`, `is_dir`, `path_exists`, `exists`,
//! `file_read_string`, `file_read`, `read_to_string`, `read`,
//! `canonicalize`, `metadata`.
//!
//! ## Scope
//!
//! First cut (bd-40luf359): files under `crates/quarto-core/src/stage/`
//! and `crates/quarto-core/src/transforms/`, the pipeline consumers. The
//! blessed mechanisms (`config_paths.rs`, `metadata_merge.rs`,
//! `project/format_paths.rs`, `project/mod.rs`, `glob/`) live outside
//! that scope or are listed in `BLESSED_SUFFIXES`. Test code is skipped.
//!
//! ## Suppressing a legitimate site
//!
//! A path that is **not** config-authored (an artifact name, a name the
//! stage itself generated, a URL-space string that is relativized later)
//! can be marked on the offending line or the line above:
//!
//! ```ignore
//! // lint:allow(config-path-base) — `name` is an artifact id, not a YAML path
//! let dest = lib_dir.join(name);
//! ```
//!
//! ## Limits
//!
//! The analysis is intra-procedural and name-based: a tainted string
//! passed to a helper is not followed into it (the parameter rule above
//! is what catches the helper), and a helper that takes only strings is
//! not flagged. Macro bodies (`format!`) are opaque. Shadowing inside one
//! function is not modelled; a name is tainted for the whole body once
//! any binding taints it.

use std::collections::HashSet;
use std::path::Path;

use anyhow::Result;
use proc_macro2::Span;
use syn::visit::Visit;
use syn::{Expr, ExprCall, ExprMethodCall, File, ImplItemFn, ItemFn, ItemMod, Local, Pat};

use super::Violation;

const RULE_NAME: &str = "config-path-base";

/// The marker that suppresses a violation on a line (or the line above).
const ALLOW_MARKER: &str = "lint:allow(config-path-base)";

/// Directories (matched as path substrings, forward slashes) the rule
/// applies to.
const SCOPE_SUBSTRINGS: &[&str] = &["quarto-core/src/stage/", "quarto-core/src/transforms/"];

/// Files inside the scope that implement a blessed mechanism.
const BLESSED_SUFFIXES: &[&str] = &[
    // Mechanism 2/3: rewrites marked values into pivot form at merge time.
    "quarto-core/src/stage/stages/metadata_merge.rs",
];

/// Final-identifier names that mark an expression as "document metadata".
const METADATA_IDENTS: &[&str] = &["meta", "metadata"];

/// Method names that read a metadata value's string.
const STRING_READS: &[&str] = &["as_plain_text", "as_str"];

/// Function/method names (besides the `resolve*` prefix) that own the
/// base-directory decision: a `let` calling one of these is clean.
const SANITIZERS: &[&str] = &[
    "pivot_form",
    "declaring_dir",
    "page_url_for",
    "base_dir_for",
    "file_base_dir",
];

/// Filesystem probes/reads that take a path: a bare config string here
/// resolves against the process CWD.
const FS_SINKS: &[&str] = &[
    "is_file",
    "is_dir",
    "path_exists",
    "exists",
    "file_read_string",
    "file_read",
    "read_to_string",
    "read",
    "canonicalize",
    "metadata",
];

/// Check a file for config-originated strings joined onto a base dir.
pub fn check(path: &Path, content: &str) -> Result<Vec<Violation>> {
    let normalized = path.to_string_lossy().replace('\\', "/");
    if !SCOPE_SUBSTRINGS.iter().any(|s| normalized.contains(s)) {
        return Ok(Vec::new());
    }
    if BLESSED_SUFFIXES.iter().any(|s| normalized.ends_with(s)) {
        return Ok(Vec::new());
    }
    // Cheap pre-filter before parsing: no path construction, no sink.
    if !["join(", "Path::new(", "PathBuf::from("]
        .iter()
        .any(|m| content.contains(m))
        && !FS_SINKS.iter().any(|m| content.contains(&format!("{m}(")))
    {
        return Ok(Vec::new());
    }
    let syntax_tree: File = match syn::parse_file(content) {
        Ok(tree) => tree,
        Err(e) => {
            eprintln!(
                "Warning: Could not parse {}: {} (skipping)",
                path.display(),
                e
            );
            return Ok(Vec::new());
        }
    };
    let mut visitor = FnVisitor {
        violations: Vec::new(),
        file_path: path.to_path_buf(),
        lines: content.lines().map(|l| l.to_string()).collect(),
    };
    visitor.visit_file(&syntax_tree);
    Ok(visitor.violations)
}

/// Walks the file function by function; each body gets its own taint set.
struct FnVisitor {
    violations: Vec<Violation>,
    file_path: std::path::PathBuf,
    lines: Vec<String>,
}

impl FnVisitor {
    fn check_body(&mut self, sig: &syn::Signature, body: &syn::Block) {
        let taint = tainted_locals(sig, body);
        if taint.names.is_empty() && taint.closures.is_empty() {
            return;
        }
        let mut sinks = SinkVisitor {
            taint: &taint,
            active: vec![taint.names.clone()],
            hits: Vec::new(),
        };
        sinks.visit_block(body);
        for (span, what) in sinks.hits {
            self.record(span, what);
        }
    }

    fn is_allowed(&self, line: usize) -> bool {
        let on = line
            .checked_sub(1)
            .and_then(|i| self.lines.get(i))
            .is_some_and(|l| l.contains(ALLOW_MARKER));
        let above = line
            .checked_sub(2)
            .and_then(|i| self.lines.get(i))
            .is_some_and(|l| l.contains(ALLOW_MARKER));
        on || above
    }

    fn record(&mut self, span: Span, what: String) {
        let start = span.start();
        let (line, column) = (start.line, start.column + 1);
        if self.is_allowed(line) {
            return;
        }
        self.violations.push(Violation {
            file: self.file_path.clone(),
            line,
            column,
            rule: RULE_NAME,
            message: format!(
                "{what}: a config-authored path string is resolved against a base \
                 directory the consumer chose; it is relative to the file that \
                 declared it"
            ),
            suggestion: Some(
                "Resolve the `ConfigValue` with `crate::config_paths::resolve_config_path` \
                 (or a resolver built on it) and use the result. If the string is not a \
                 YAML-authored path, add `// lint:allow(config-path-base)` with a reason."
                    .to_string(),
            ),
        });
    }
}

impl<'ast> Visit<'ast> for FnVisitor {
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        if is_test_attrs(&node.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, node);
    }

    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        if is_test_attrs(&node.attrs) {
            return;
        }
        self.check_body(&node.sig, &node.block);
        // Nested fns inside the body get their own pass.
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        if is_test_attrs(&node.attrs) {
            return;
        }
        self.check_body(&node.sig, &node.block);
        syn::visit::visit_impl_item_fn(self, node);
    }
}

// ─── taint collection ────────────────────────────────────────────────

/// What is tainted in one function body.
///
/// `names` are function-wide: parameters and `let` bindings (shadowing
/// is not modelled). `closures` are the closures whose *parameters* are
/// tainted, keyed by the closure's position — closure parameters are
/// scoped to their closure, so an untainted `|p|` elsewhere in the same
/// function is not confused with a tainted one (and a closure parameter
/// shadows an outer tainted name).
struct Taint {
    names: HashSet<String>,
    closures: HashSet<(usize, usize)>,
}

fn closure_key(closure: &syn::ExprClosure) -> (usize, usize) {
    let start = closure.or1_token.span.start();
    (start.line, start.column)
}

/// The tainted names in effect inside `closure`, given the names in
/// effect outside it.
fn closure_scope(
    active: &HashSet<String>,
    closure: &syn::ExprClosure,
    taint: &Taint,
) -> HashSet<String> {
    let params: Vec<String> = closure.inputs.iter().flat_map(pattern_names).collect();
    let mut next = active.clone();
    for p in &params {
        next.remove(p);
    }
    if taint.closures.contains(&closure_key(closure)) {
        next.extend(params);
    }
    next
}

/// Names tainted in one function: string parameters beside a directory
/// parameter, `let`s bound from a metadata read, closure parameters over
/// a tainted receiver — transitively, minus what a resolver cleaned.
/// Fixpoint over every binding in `body`, in any order.
fn tainted_locals(sig: &syn::Signature, body: &syn::Block) -> Taint {
    let mut taint = Taint {
        names: tainted_params(sig).into_iter().collect(),
        closures: HashSet::new(),
    };

    let mut bindings = BindingCollector {
        lets: Vec::new(),
        closures: Vec::new(),
    };
    bindings.visit_block(body);
    loop {
        let before = (taint.names.len(), taint.closures.len());
        for local in &bindings.lets {
            let Some(init) = &local.init else { continue };
            let init = &*init.expr;
            if calls_sanitizer(init) {
                continue;
            }
            if contains_metadata_read(init) || mentions_any(init, &taint.names, &taint) {
                taint.names.extend(pattern_names(&local.pat));
            }
        }
        for (receiver, closure) in &bindings.closures {
            if contains_metadata_read(receiver) || mentions_any(receiver, &taint.names, &taint) {
                taint.closures.insert(*closure);
            }
        }
        if (taint.names.len(), taint.closures.len()) == before {
            break;
        }
    }
    taint
}

/// The `&str` / `String` parameters of a function that also takes a
/// `Path` / `PathBuf`: the "string plus base directory" helper shape.
fn tainted_params(sig: &syn::Signature) -> Vec<String> {
    let typed: Vec<(&Pat, &syn::Type)> = sig
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            syn::FnArg::Typed(t) => Some((&*t.pat, &*t.ty)),
            syn::FnArg::Receiver(_) => None,
        })
        .collect();
    if !typed
        .iter()
        .any(|(_, ty)| mentions_type_ident(ty, &["Path", "PathBuf"]))
    {
        return Vec::new();
    }
    typed
        .iter()
        .filter(|(_, ty)| is_string_type(ty))
        .flat_map(|(pat, _)| pattern_names(pat))
        .collect()
}

/// Any type built on `str` / `String`: `&str`, `String`, `Option<&str>`,
/// `&[String]`, `Vec<String>`, …
fn is_string_type(ty: &syn::Type) -> bool {
    mentions_type_ident(ty, &["str", "String"])
}

/// True if any path segment in `ty` is one of `idents`.
fn mentions_type_ident(ty: &syn::Type, idents: &[&str]) -> bool {
    struct Finder<'a> {
        idents: &'a [&'a str],
        found: bool,
    }
    impl<'ast, 'a> Visit<'ast> for Finder<'a> {
        fn visit_path_segment(&mut self, node: &'ast syn::PathSegment) {
            if self.idents.contains(&node.ident.to_string().as_str()) {
                self.found = true;
            }
            syn::visit::visit_path_segment(self, node);
        }
    }
    let mut finder = Finder {
        idents,
        found: false,
    };
    finder.visit_type(ty);
    finder.found
}

/// Every `let` and every closure-argument method call in a body. Nested
/// `fn` items are their own scope and are not descended into.
struct BindingCollector<'ast> {
    lets: Vec<&'ast Local>,
    /// (method-call receiver, the closure argument's key)
    closures: Vec<(&'ast Expr, (usize, usize))>,
}

impl<'ast> Visit<'ast> for BindingCollector<'ast> {
    fn visit_item_fn(&mut self, _node: &'ast ItemFn) {}

    fn visit_local(&mut self, node: &'ast Local) {
        self.lets.push(node);
        syn::visit::visit_local(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        for arg in &node.args {
            if let Expr::Closure(closure) = arg
                && !closure.inputs.is_empty()
            {
                self.closures.push((&node.receiver, closure_key(closure)));
            }
        }
        syn::visit::visit_expr_method_call(self, node);
    }
}

/// Identifiers a `let` pattern binds (`x`, `(a, b)`, `Some(x)`, `mut x`, …).
fn pattern_names(pat: &Pat) -> Vec<String> {
    struct Names(Vec<String>);
    impl<'ast> Visit<'ast> for Names {
        fn visit_pat_ident(&mut self, node: &'ast syn::PatIdent) {
            self.0.push(node.ident.to_string());
            syn::visit::visit_pat_ident(self, node);
        }
    }
    let mut names = Names(Vec::new());
    names.visit_pat(pat);
    names.0
}

/// True if `expr` contains a `.get(<lit>)` / `.get_path(..)` on a
/// metadata expression, or any string read (`as_plain_text`/`as_str`).
fn contains_metadata_read(expr: &Expr) -> bool {
    struct Finder(bool);
    impl<'ast> Visit<'ast> for Finder {
        fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
            let method = call.method.to_string();
            if STRING_READS.contains(&method.as_str()) {
                self.0 = true;
            }
            if matches!(method.as_str(), "get" | "get_path" | "get_mut")
                && is_metadata_expr(&call.receiver)
            {
                self.0 = true;
            }
            syn::visit::visit_expr_method_call(self, call);
        }
    }
    let mut finder = Finder(false);
    finder.visit_expr(expr);
    finder.0
}

/// True if `expr` calls a resolver: a function or method whose name
/// starts with `resolve`, or one of [`SANITIZERS`].
fn calls_sanitizer(expr: &Expr) -> bool {
    fn is_sanitizer_name(name: &str) -> bool {
        name.starts_with("resolve") || SANITIZERS.contains(&name)
    }
    struct Finder(bool);
    impl<'ast> Visit<'ast> for Finder {
        fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
            if is_sanitizer_name(&call.method.to_string()) {
                self.0 = true;
            }
            syn::visit::visit_expr_method_call(self, call);
        }
        fn visit_expr_call(&mut self, call: &'ast ExprCall) {
            if let Expr::Path(p) = &*call.func
                && p.path
                    .segments
                    .last()
                    .is_some_and(|s| is_sanitizer_name(&s.ident.to_string()))
            {
                self.0 = true;
            }
            syn::visit::visit_expr_call(self, call);
        }
    }
    let mut finder = Finder(false);
    finder.visit_expr(expr);
    finder.0
}

/// True if `expr` mentions (as a bare path) a tainted name: one of
/// `active` outside any closure, with closure parameters scoped per
/// [`closure_scope`].
fn mentions_any(expr: &Expr, active: &HashSet<String>, taint: &Taint) -> bool {
    if active.is_empty() && taint.closures.is_empty() {
        return false;
    }
    struct Finder<'a> {
        taint: &'a Taint,
        active: Vec<HashSet<String>>,
        found: bool,
    }
    impl<'ast, 'a> Visit<'ast> for Finder<'a> {
        fn visit_item_fn(&mut self, _node: &'ast ItemFn) {}

        fn visit_expr_closure(&mut self, node: &'ast syn::ExprClosure) {
            let scope = closure_scope(self.active.last().unwrap(), node, self.taint);
            self.active.push(scope);
            syn::visit::visit_expr_closure(self, node);
            self.active.pop();
        }

        fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
            if node.path.segments.len() == 1
                && self
                    .active
                    .last()
                    .unwrap()
                    .contains(&node.path.segments[0].ident.to_string())
            {
                self.found = true;
            }
            syn::visit::visit_expr_path(self, node);
        }
    }
    let mut finder = Finder {
        taint,
        active: vec![active.clone()],
        found: false,
    };
    finder.visit_expr(expr);
    finder.found
}

/// True if `expr`'s final identifier is a metadata name (`meta`/`metadata`).
fn is_metadata_expr(expr: &Expr) -> bool {
    match expr {
        Expr::Path(p) => p
            .path
            .segments
            .last()
            .is_some_and(|s| METADATA_IDENTS.contains(&s.ident.to_string().as_str())),
        Expr::Field(f) => match &f.member {
            syn::Member::Named(ident) => METADATA_IDENTS.contains(&ident.to_string().as_str()),
            syn::Member::Unnamed(_) => false,
        },
        Expr::Reference(r) => is_metadata_expr(&r.expr),
        _ => false,
    }
}

// ─── sinks ───────────────────────────────────────────────────────────

struct SinkVisitor<'a> {
    taint: &'a Taint,
    /// The tainted names in effect, innermost closure scope last.
    active: Vec<HashSet<String>>,
    hits: Vec<(Span, String)>,
}

impl SinkVisitor<'_> {
    fn is_tainted(&self, expr: &Expr) -> bool {
        mentions_any(expr, self.active.last().unwrap(), self.taint)
    }
}

impl<'ast, 'a> Visit<'ast> for SinkVisitor<'a> {
    fn visit_item_fn(&mut self, _node: &'ast ItemFn) {}

    fn visit_expr_closure(&mut self, node: &'ast syn::ExprClosure) {
        let scope = closure_scope(self.active.last().unwrap(), node, self.taint);
        self.active.push(scope);
        syn::visit::visit_expr_closure(self, node);
        self.active.pop();
    }

    fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
        let method = call.method.to_string();
        let tainted_arg = call.args.iter().any(|a| self.is_tainted(a));
        if tainted_arg {
            if method == "join" {
                self.hits.push((
                    call.method.span(),
                    "`.join(..)` of a metadata string".into(),
                ));
            } else if FS_SINKS.contains(&method.as_str()) {
                self.hits.push((
                    call.method.span(),
                    format!("`.{method}(..)` probes a bare metadata string"),
                ));
            }
        }
        syn::visit::visit_expr_method_call(self, call);
    }

    fn visit_expr_call(&mut self, call: &'ast ExprCall) {
        if let Expr::Path(p) = &*call.func {
            let segs: Vec<String> = p
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            let is_path_ctor = matches!(
                segs.as_slice(),
                [.., a, b] if (a == "Path" && b == "new") || (a == "PathBuf" && b == "from")
            );
            let is_fs_read = matches!(
                segs.as_slice(),
                [.., a, b] if a == "fs" && FS_SINKS.contains(&b.as_str())
            );
            if (is_path_ctor || is_fs_read) && call.args.iter().any(|a| self.is_tainted(a)) {
                self.hits.push((
                    p.path.segments.last().unwrap().ident.span(),
                    format!("`{}` of a metadata string", segs.join("::")),
                ));
            }
        }
        syn::visit::visit_expr_call(self, call);
    }
}

/// True if the attribute list marks test-only code we should skip.
fn is_test_attrs(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let path = attr.path();
        if path.is_ident("test") {
            return true;
        }
        if path.segments.last().is_some_and(|s| s.ident == "test") {
            return true;
        }
        if path.is_ident("cfg") {
            let mut is_cfg_test = false;
            let _ = attr.parse_nested_meta(|nested| {
                if nested.path.is_ident("test") {
                    is_cfg_test = true;
                }
                Ok(())
            });
            return is_cfg_test;
        }
        false
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lint(src: &str) -> Vec<Violation> {
        check(
            Path::new("crates/quarto-core/src/transforms/example.rs"),
            src,
        )
        .unwrap()
    }

    #[test]
    fn join_of_a_metadata_string_is_flagged() {
        let v = lint(
            "fn f(meta: &ConfigValue, doc_dir: &Path) -> PathBuf {\n\
             let raw = meta.get(\"template\").and_then(|v| v.as_plain_text()).unwrap();\n\
             doc_dir.join(&raw)\n}\n",
        );
        assert_eq!(v.len(), 1, "{v:?}");
        assert_eq!(v[0].line, 3);
    }

    #[test]
    fn derived_locals_stay_tainted() {
        let v = lint(
            "fn f(meta: &ConfigValue, doc_dir: &Path) -> PathBuf {\n\
             let raw = meta.get(\"template\").and_then(|v| v.as_plain_text()).unwrap();\n\
             let spec = raw.trim().to_string();\n\
             PathBuf::from(spec)\n}\n",
        );
        assert_eq!(v.len(), 1, "{v:?}");
        assert_eq!(v[0].line, 4);
    }

    #[test]
    fn resolver_output_is_clean() {
        let v = lint(
            "fn f(meta: &ConfigValue, ctx: &PathCtx, out: &Path) -> PathBuf {\n\
             let value = meta.get(\"template\").unwrap();\n\
             let abs = crate::config_paths::resolve_config_path(value, ctx).unwrap().file().unwrap();\n\
             let rel = pivot_form(&abs, ctx.project_dir);\n\
             out.join(&rel)\n}\n",
        );
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn filesystem_probe_of_bare_string_is_flagged() {
        let v = lint(
            "fn f(meta: &ConfigValue, runtime: &dyn SystemRuntime) -> bool {\n\
             let raw = meta.get(\"csl\").and_then(|v| v.as_plain_text()).unwrap();\n\
             runtime.is_file(Path::new(&raw)).unwrap_or(false)\n}\n",
        );
        // `Path::new(&raw)` and `is_file(..)` both report; one site.
        assert!(!v.is_empty(), "{v:?}");
        assert!(v.iter().all(|x| x.line == 3), "{v:?}");
    }

    #[test]
    fn string_param_beside_a_dir_param_is_tainted() {
        let v = lint(
            "fn is_banner_image(banner: &str, input_dir: &Path) -> bool {\n\
             let path = Path::new(banner);\n\
             input_dir.join(path).is_file()\n}\n",
        );
        assert_eq!(v.len(), 2, "{v:?}");
        assert_eq!((v[0].line, v[1].line), (2, 3));
    }

    #[test]
    fn string_param_without_a_dir_param_is_not() {
        let v = lint("fn stem(name: &str, ext: &str) -> String {\n format!(\"{name}.{ext}\")\n}\n");
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn closure_param_over_a_metadata_read_is_tainted() {
        let v = lint(
            "fn f(items: &[ConfigValue], document_dir: &Path) -> Vec<PathBuf> {\n\
             items.iter().filter_map(|item| item.as_plain_text().map(|s| document_dir.join(s))).collect()\n}\n",
        );
        assert_eq!(v.len(), 1, "{v:?}");
    }

    #[test]
    fn closure_param_over_a_tainted_local_is_tainted() {
        let v = lint(
            "fn f(meta: &ConfigValue, dir: &Path) -> Vec<PathBuf> {\n\
             let partials = meta.get(\"template-partials\").and_then(|v| v.as_plain_text()).into_iter().collect::<Vec<_>>();\n\
             partials.iter().map(|p| dir.join(p)).collect()\n}\n",
        );
        assert_eq!(v.len(), 1, "{v:?}");
        assert_eq!(v[0].line, 3);
    }

    /// Closure parameters are scoped: an untainted `|p|` elsewhere in
    /// the function does not inherit taint from a tainted `|p|`, and a
    /// closure parameter shadows an outer tainted name.
    #[test]
    fn closure_params_are_scoped_to_their_closure() {
        let v = lint(
            "fn f(meta: &ConfigValue, dir: &Path, members: &[Profile]) -> Vec<PathBuf> {\n\
             let title = meta.get(\"title\").and_then(|v| v.as_plain_text());\n\
             let picked = members.iter().filter(|p| p.ok).map(|p| dir.join(p.name())).collect::<Vec<_>>();\n\
             let _ = title.map(|p| p.len());\n\
             picked\n}\n",
        );
        assert!(v.is_empty(), "{v:?}");
        let v = lint(
            "fn f(meta: &ConfigValue, dir: &Path, items: &[Item]) -> Vec<PathBuf> {\n\
             let raw = meta.get(\"x\").and_then(|v| v.as_plain_text()).unwrap();\n\
             items.iter().map(|raw| dir.join(raw.name())).collect()\n}\n",
        );
        assert!(
            v.is_empty(),
            "a closure parameter shadows the outer tainted `raw`; {v:?}"
        );
    }

    #[test]
    fn allow_marker_suppresses() {
        let v = lint(
            "fn f(meta: &ConfigValue, lib: &Path) -> PathBuf {\n\
             let name = meta.get(\"x\").and_then(|v| v.as_plain_text()).unwrap();\n\
             // lint:allow(config-path-base) — an artifact id, not a YAML path\n\
             lib.join(&name)\n}\n",
        );
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn out_of_scope_files_and_tests_are_skipped() {
        let src = "fn f(meta: &ConfigValue, d: &Path) -> PathBuf {\n\
             let raw = meta.get(\"x\").and_then(|v| v.as_plain_text()).unwrap();\n\
             d.join(&raw)\n}\n";
        assert!(
            check(Path::new("crates/quarto-core/src/project/mod.rs"), src)
                .unwrap()
                .is_empty()
        );
        let in_test = format!("#[cfg(test)]\nmod tests {{\n{src}}}\n");
        assert!(lint(&in_test).is_empty());
    }

    #[test]
    fn untainted_joins_are_ignored() {
        let v = lint(
            "fn f(meta: &ConfigValue, d: &Path, stem: &str) -> PathBuf {\n\
             let _title = meta.get(\"title\").and_then(|v| v.as_plain_text());\n\
             d.join(format!(\"{stem}_files\")).join(\"x\")\n}\n",
        );
        assert!(v.is_empty(), "{v:?}");
    }
}
