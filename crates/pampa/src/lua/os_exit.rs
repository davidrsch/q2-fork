/*
 * lua/os_exit.rs
 * Copyright (c) 2026 Posit, PBC
 *
 * `os.exit` that fails the document instead of the process (GH #941).
 *
 * Quarto 1 filters call `os.exit(1)` to fail a render: pandoc was a child
 * process, so exiting it failed that one document. In q2 the filter runs
 * inside the renderer, and the stdlib's `os.exit` is libc `exit`: no page
 * is written (not even the ones already rendered), no summary is printed,
 * and errors on pages not yet reached are never reported. This module
 * replaces `os.exit` on the `os` table — the native stdlib's or the WASM
 * synthetic one — with a function that raises a Rust error carrying the
 * requested code; `apply_lua_filter` turns it into
 * `LuaFilterError::Exit`, which fails the current document the way
 * `error()` does.
 */

use mlua::{Lua, Result, Table, Value};

/// The error `os.exit` raises. Carries the status the script asked for;
/// the host reports it in the message rather than exiting with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LuaExit {
    pub code: i64,
}

impl std::fmt::Display for LuaExit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "os.exit({})", self.code)
    }
}

impl std::error::Error for LuaExit {}

/// Lua's own mapping of `os.exit`'s first argument to a status: none or
/// `true` is success (0), `false` is failure (1), a number is itself.
/// Anything else is a Lua type error, as in the stdlib.
fn exit_code(arg: Value) -> Result<i64> {
    match arg {
        Value::Nil => Ok(0),
        Value::Boolean(true) => Ok(0),
        Value::Boolean(false) => Ok(1),
        Value::Integer(n) => Ok(n),
        Value::Number(n) if n.fract() == 0.0 => Ok(n as i64),
        other => Err(mlua::Error::runtime(format!(
            "bad argument #1 to 'exit' (number or boolean expected, got {})",
            other.type_name()
        ))),
    }
}

/// Replace `os.exit` on the current `os` table. The optional second
/// argument (`close`) is accepted and ignored. A missing `os` table (a
/// restricted state without the synthetic one) is left alone.
pub fn install_os_exit(lua: &Lua) -> Result<()> {
    let os: Table = match lua.globals().get::<Value>("os")? {
        Value::Table(t) => t,
        _ => return Ok(()),
    };
    os.set(
        "exit",
        lua.create_function(|_, (code, _close): (Value, Value)| -> Result<()> {
            Err(mlua::Error::external(LuaExit {
                code: exit_code(code)?,
            }))
        })?,
    )?;
    Ok(())
}

/// The exit code if `err` is, or wraps, the error `os.exit` raised. mlua
/// nests the external error inside `CallbackError` (one level per Lua →
/// Rust → Lua boundary crossed) and sometimes `WithContext`; walk them.
pub fn requested_exit(err: &mlua::Error) -> Option<i64> {
    let mut current = err;
    loop {
        match current {
            mlua::Error::ExternalError(e) => {
                return e.downcast_ref::<LuaExit>().map(|exit| exit.code);
            }
            mlua::Error::CallbackError { cause, .. } => current = cause,
            mlua::Error::WithContext { cause, .. } => current = cause,
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lua() -> Lua {
        let lua = Lua::new();
        install_os_exit(&lua).unwrap();
        lua
    }

    fn code_of(src: &str) -> Option<i64> {
        let err = lua().load(src).exec().unwrap_err();
        requested_exit(&err)
    }

    #[test]
    fn exit_raises_with_luas_code_mapping() {
        assert_eq!(code_of("os.exit(1)"), Some(1));
        assert_eq!(code_of("os.exit(3, true)"), Some(3));
        assert_eq!(code_of("os.exit()"), Some(0));
        assert_eq!(code_of("os.exit(true)"), Some(0));
        assert_eq!(code_of("os.exit(false)"), Some(1));
        // Raised from inside a Lua function called from Lua: still found.
        assert_eq!(
            code_of("local function die() os.exit(2) end\nlocal t = { die }\nt[1]()"),
            Some(2)
        );
    }

    #[test]
    fn other_errors_are_not_exits() {
        let err = lua().load("error('plain')").exec().unwrap_err();
        assert_eq!(requested_exit(&err), None);
        let err = lua().load("os.exit('nope')").exec().unwrap_err();
        assert_eq!(requested_exit(&err), None);
        assert!(
            err.to_string().contains("bad argument #1 to 'exit'"),
            "{err}"
        );
    }

    #[test]
    fn exit_is_catchable_by_pcall_like_any_error() {
        // Documented caveat: pcall(os.exit, 1) catches it. Nobody does this
        // on purpose, and it is what a Rust-side error must do.
        let caught: bool = lua()
            .load("local ok = pcall(os.exit, 1) return ok == false")
            .eval()
            .unwrap();
        assert!(caught);
    }
}
