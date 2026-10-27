/*
 * lua/host_context.rs
 * Copyright (c) 2026 Posit, PBC
 *
 * What a Lua script learns about where it is running, from the host's
 * `HostContext` (GH #939, #940):
 *
 * - `quarto.project` — `directory`, `offset`, `output_directory`,
 *   `profile`, with Quarto 1's shape (nil paths outside a project, the
 *   profile list always present);
 * - `os.getenv` — answered from the host's environment snapshot when it
 *   supplied one, so the stdlib's libc `getenv` never runs on a filter
 *   worker thread and `--profile` is visible as `QUARTO_PROFILE`.
 *
 * Registered after `quarto` and `os` exist, in both the filter and the
 * shortcode environments.
 */

use mlua::{Lua, Result, Table, Value};

use crate::lua_options::HostContext;

/// Register everything the host context contributes to the environment.
pub fn register_host_context(lua: &Lua, host: &HostContext) -> Result<()> {
    register_quarto_project(lua, host)?;
    install_os_getenv(lua, host)?;
    Ok(())
}

/// `quarto.project`: Quarto 1's table, with nils where Q1 had them.
///
/// `profile` is a `pandoc.List` so Q1 code can call `:includes("drafts")`
/// on it; it is present (possibly empty) even outside a project, as in Q1
/// where it came from a filter param rather than the project.
fn register_quarto_project(lua: &Lua, host: &HostContext) -> Result<()> {
    let quarto: Table = lua.globals().get("quarto")?;
    let project = lua.create_table()?;
    if let Some(p) = &host.project {
        project.set("directory", p.directory.to_string_lossy().to_string())?;
        project.set("offset", p.offset.as_str())?;
        project.set(
            "output_directory",
            p.output_directory.to_string_lossy().to_string(),
        )?;
    }
    project.set(
        "profile",
        super::list::create_string_list_table(lua, &host.profiles)?,
    )?;
    quarto.set("project", project)?;
    Ok(())
}

/// Replace `os.getenv` with a lookup in the host's snapshot. Without a
/// snapshot the table is left alone: the stdlib function on native, the
/// runtime-backed one `os_wasm` installs on WASM.
fn install_os_getenv(lua: &Lua, host: &HostContext) -> Result<()> {
    let Some(env) = host.env.clone() else {
        return Ok(());
    };
    let os: Table = match lua.globals().get::<Value>("os")? {
        Value::Table(t) => t,
        _ => return Ok(()),
    };
    os.set(
        "getenv",
        lua.create_function(move |_, name: String| Ok(env.get(&name).cloned()))?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lua_options::HostProject;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn lua_with(host: &HostContext) -> Lua {
        let lua = Lua::new();
        let runtime: Arc<dyn crate::lua::runtime::SystemRuntime> =
            Arc::new(crate::lua::runtime::NativeRuntime::new());
        super::super::constructors::register_pandoc_namespace(
            &lua,
            runtime,
            super::super::mediabag::create_shared_mediabag(),
        )
        .unwrap();
        super::super::quarto_api::register_quarto_api(&lua).unwrap();
        register_host_context(&lua, host).unwrap();
        lua
    }

    fn eval<T: mlua::FromLua>(lua: &Lua, src: &str) -> T {
        lua.load(src).eval().unwrap()
    }

    #[test]
    fn quarto_project_outside_a_project_has_q1_nils_and_an_empty_profile_list() {
        let lua = lua_with(&HostContext::default());
        assert!(eval::<bool>(&lua, "return quarto.project.directory == nil"));
        assert!(eval::<bool>(&lua, "return quarto.project.offset == nil"));
        assert!(eval::<bool>(
            &lua,
            "return quarto.project.output_directory == nil"
        ));
        assert_eq!(eval::<i64>(&lua, "return #quarto.project.profile"), 0);
        assert_eq!(
            eval::<String>(&lua, "return pandoc.utils.type(quarto.project.profile)"),
            "List"
        );
    }

    #[test]
    fn quarto_project_inside_a_project() {
        let dir = PathBuf::from("/proj");
        let host = HostContext {
            project: Some(HostProject {
                directory: dir.clone(),
                output_directory: dir.join("_site"),
                offset: "..".to_string(),
            }),
            profiles: vec!["drafts".to_string(), "release".to_string()],
            ..Default::default()
        };
        let lua = lua_with(&host);
        assert_eq!(
            eval::<String>(&lua, "return quarto.project.directory"),
            dir.to_string_lossy()
        );
        assert_eq!(eval::<String>(&lua, "return quarto.project.offset"), "..");
        assert_eq!(
            eval::<String>(&lua, "return quarto.project.output_directory"),
            dir.join("_site").to_string_lossy()
        );
        assert!(eval::<bool>(
            &lua,
            "return quarto.project.profile:includes('drafts') and #quarto.project.profile == 2"
        ));
    }

    #[test]
    fn os_getenv_reads_the_snapshot_only() {
        let mut env = HashMap::new();
        env.insert("QUARTO_PROFILE".to_string(), "drafts,release".to_string());
        env.insert("FROM_FILE".to_string(), "yes".to_string());
        let host = HostContext {
            env: Some(Arc::new(env)),
            ..Default::default()
        };
        let lua = lua_with(&host);
        assert_eq!(
            eval::<String>(&lua, "return os.getenv('QUARTO_PROFILE')"),
            "drafts,release"
        );
        assert_eq!(eval::<String>(&lua, "return os.getenv('FROM_FILE')"), "yes");
        // Not in the snapshot means not visible, even when the real
        // process has it: the snapshot is the whole environment.
        assert!(eval::<bool>(&lua, "return os.getenv('PATH') == nil"));
    }

    #[test]
    fn os_getenv_without_a_snapshot_is_the_stdlib() {
        let lua = lua_with(&HostContext::default());
        // PATH is set in every environment the test suite runs under.
        assert!(eval::<bool>(
            &lua,
            "return type(os.getenv('PATH')) == 'string'"
        ));
    }
}
