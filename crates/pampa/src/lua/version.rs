/*
 * lua/version.rs
 * Copyright (c) 2026 Posit, PBC
 *
 * `pandoc.types.Version`: pandoc's comparable version object, and the
 * values built from it — `quarto.version`, `PANDOC_VERSION`,
 * `PANDOC_API_VERSION`.
 *
 * Quarto 1 filters do `quarto.version >= "1.4"`,
 * `PANDOC_VERSION < pandoc.types.Version("2.11.3.2")`, and
 * `PANDOC_VERSION:must_be_at_least("2.1")`; they also do
 * `table.concat(quarto.version, ".")`, which works on pandoc's userdata
 * because Lua 5.4's `table.concat` accepts any value with `__index` and
 * `__len` metamethods (`checktab`, `TAB_R | TAB_L`). GH #942.
 */

use mlua::{AnyUserData, Lua, MetaMethod, Result, Table, UserData, UserDataMethods, Value};

/// A dotted-integer version, compared lexicographically on its
/// components with a shorter prefix ordering first (`1.4 < 1.4.1`), as
/// pandoc's `Version` (Haskell's `Data.Version`) does.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LuaVersion(pub Vec<u64>);

impl LuaVersion {
    /// The leading dotted-integer prefix of a version string: the part a
    /// `Version` can hold. `"0.37.0-nightly.20261027"` → `0.37.0`;
    /// `"1.9.13+test.20260910"` → `1.9.13`. A string with no leading
    /// digit yields `0`, never an error — callers such as
    /// `register_quarto_version` must always produce a Version (Q1's
    /// `leadingDottedNumber` has the same contract).
    pub fn leading(s: &str) -> Self {
        let mut parts = Vec::new();
        for piece in s.split('.') {
            let digits: String = piece.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                break;
            }
            // Digit-only strings of plausible length always parse; a
            // pathological run of digits saturates rather than panics.
            parts.push(digits.parse::<u64>().unwrap_or(u64::MAX));
            if digits.len() != piece.len() {
                // "0-nightly" — the component ended at a non-digit, so
                // nothing after it is part of the version.
                break;
            }
        }
        if parts.is_empty() {
            parts.push(0);
        }
        LuaVersion(parts)
    }

    /// Parse a strict version spec string: one or more `.`-separated
    /// non-negative integers and nothing else.
    fn parse_strict(s: &str) -> Option<Self> {
        if s.is_empty() {
            return None;
        }
        s.split('.')
            .map(|p| {
                if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
                    None
                } else {
                    p.parse::<u64>().ok()
                }
            })
            .collect::<Option<Vec<_>>>()
            .map(LuaVersion)
    }

    /// Coerce a Lua value into a version the way `pandoc.types.Version`
    /// does: a Version, a version string, a non-negative integer, or a
    /// sequence of non-negative integers.
    fn from_value(value: &Value) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud
                .borrow::<LuaVersion>()
                .map(|v| v.clone())
                .map_err(|_| version_expected(value)),
            Value::String(s) => {
                let s = s.to_str()?;
                LuaVersion::parse_strict(&s).ok_or_else(|| {
                    mlua::Error::runtime(format!(
                        "could not parse '{}' as a version: expected dot-separated integers",
                        &*s
                    ))
                })
            }
            Value::Integer(n) if *n >= 0 => Ok(LuaVersion(vec![*n as u64])),
            Value::Number(n) if *n >= 0.0 && n.fract() == 0.0 => Ok(LuaVersion(vec![*n as u64])),
            Value::Table(t) => {
                let mut parts = Vec::new();
                for item in t.clone().sequence_values::<Value>() {
                    match item? {
                        Value::Integer(n) if n >= 0 => parts.push(n as u64),
                        Value::Number(n) if n >= 0.0 && n.fract() == 0.0 => parts.push(n as u64),
                        other => {
                            return Err(mlua::Error::runtime(format!(
                                "version component must be a non-negative integer, got {}",
                                other.type_name()
                            )));
                        }
                    }
                }
                Ok(LuaVersion(parts))
            }
            _ => Err(version_expected(value)),
        }
    }

    fn display(&self) -> String {
        self.0
            .iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(".")
    }
}

fn version_expected(value: &Value) -> mlua::Error {
    mlua::Error::runtime(format!(
        "Version expected, got {} (a version string, integer, or sequence of integers)",
        value.type_name()
    ))
}

impl UserData for LuaVersion {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| Ok(this.display()));
        methods.add_meta_method(MetaMethod::Len, |_, this, ()| Ok(this.0.len() as i64));
        // Lua 5.4 only consults `__eq` when both operands are userdata, so
        // `v == "1.4"` is always false, exactly as in pandoc.
        methods.add_meta_method(MetaMethod::Eq, |_, this, other: Value| {
            Ok(match &other {
                Value::UserData(ud) => ud.borrow::<LuaVersion>().is_ok_and(|o| *o == *this),
                _ => false,
            })
        });
        // `__lt`/`__le` as meta *functions*: for `v >= "1.4"` Lua calls
        // `__le("1.4", v)`, with the string first, so the Version is not
        // always the receiver.
        methods.add_meta_function(MetaMethod::Lt, |_, (a, b): (Value, Value)| {
            Ok(LuaVersion::from_value(&a)? < LuaVersion::from_value(&b)?)
        });
        methods.add_meta_function(MetaMethod::Le, |_, (a, b): (Value, Value)| {
            Ok(LuaVersion::from_value(&a)? <= LuaVersion::from_value(&b)?)
        });
        // Integer keys index the components (1-based, as a Lua sequence);
        // `must_be_at_least` is the one method. Done inside `__index`
        // rather than `add_method` so this metamethod owns the lookup —
        // `table.concat` needs it present alongside `__len`.
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: Value| match key {
            Value::Integer(i) => Ok(match usize::try_from(i) {
                Ok(i) if i >= 1 && i <= this.0.len() => Value::Integer(this.0[i - 1] as i64),
                _ => Value::Nil,
            }),
            Value::Number(n) if n.fract() == 0.0 => {
                let i = n as i64;
                Ok(match usize::try_from(i) {
                    Ok(i) if i >= 1 && i <= this.0.len() => Value::Integer(this.0[i - 1] as i64),
                    _ => Value::Nil,
                })
            }
            Value::String(s) if s.to_str().is_ok_and(|s| &*s == "must_be_at_least") => {
                Ok(Value::Function(lua.create_function(
                    |_, (this, expected, message): (AnyUserData, Value, Option<String>)| {
                        let actual = this.borrow::<LuaVersion>()?;
                        let expected = LuaVersion::from_value(&expected)?;
                        if *actual < expected {
                            let message = message.unwrap_or_else(|| {
                                "expected version %s or newer, got %s".to_string()
                            });
                            // pandoc formats the message with string.format;
                            // the two `%s` are expected then actual.
                            let message = message.replacen("%s", &expected.display(), 1).replacen(
                                "%s",
                                &actual.display(),
                                1,
                            );
                            return Err(mlua::Error::runtime(message));
                        }
                        Ok(())
                    },
                )?))
            }
            _ => Ok(Value::Nil),
        });
    }
}

/// Wrap components as a `Version` userdata.
pub fn create_version(lua: &Lua, components: &[u64]) -> Result<AnyUserData> {
    lua.create_userdata(LuaVersion(components.to_vec()))
}

/// Register `pandoc.types` with its `Version` constructor.
pub fn register_pandoc_types(lua: &Lua, pandoc: &Table) -> Result<()> {
    let types = lua.create_table()?;
    types.set(
        "Version",
        lua.create_function(|lua, spec: Value| {
            lua.create_userdata(LuaVersion::from_value(&spec)?)
        })?,
    )?;
    pandoc.set("types", types)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lua() -> Lua {
        let lua = Lua::new();
        let pandoc = lua.create_table().unwrap();
        register_pandoc_types(&lua, &pandoc).unwrap();
        lua.globals().set("pandoc", pandoc).unwrap();
        lua.globals()
            .set("V", create_version(&lua, &[0, 36, 0]).unwrap())
            .unwrap();
        lua
    }

    fn eval<T: mlua::FromLua>(lua: &Lua, src: &str) -> T {
        lua.load(src).eval().unwrap()
    }

    #[test]
    fn leading_prefix() {
        assert_eq!(LuaVersion::leading("0.36.0").0, vec![0, 36, 0]);
        assert_eq!(
            LuaVersion::leading("0.37.0-nightly.20261027").0,
            vec![0, 37, 0]
        );
        assert_eq!(
            LuaVersion::leading("1.9.13+test.20260910").0,
            vec![1, 9, 13]
        );
        assert_eq!(LuaVersion::leading("1.4").0, vec![1, 4]);
        assert_eq!(LuaVersion::leading("unknown").0, vec![0]);
        assert_eq!(LuaVersion::leading("").0, vec![0]);
    }

    #[test]
    fn tostring_and_concat() {
        let lua = lua();
        assert_eq!(eval::<String>(&lua, "return tostring(V)"), "0.36.0");
        // The whole point of __index + __len: Lua 5.4's table.concat
        // accepts the userdata, so Q1-era code keeps working.
        assert_eq!(
            eval::<String>(&lua, "return table.concat(V, '.')"),
            "0.36.0"
        );
        assert_eq!(eval::<i64>(&lua, "return #V"), 3);
        assert_eq!(eval::<i64>(&lua, "return V[2]"), 36);
        assert!(eval::<bool>(&lua, "return V[4] == nil"));
        assert_eq!(eval::<String>(&lua, "return type(V)"), "userdata");
    }

    #[test]
    fn comparisons_with_strings_either_side() {
        let lua = lua();
        assert!(eval::<bool>(&lua, "return V >= '0.36'"));
        assert!(eval::<bool>(&lua, "return V > '0.36'"));
        assert!(eval::<bool>(&lua, "return V < '1.4'"));
        assert!(eval::<bool>(&lua, "return '1.4' > V"));
        assert!(eval::<bool>(&lua, "return '0.36.0' <= V"));
        assert!(!eval::<bool>(&lua, "return V >= '1.4'"));
        assert!(eval::<bool>(
            &lua,
            "return V == pandoc.types.Version('0.36.0')"
        ));
        assert!(eval::<bool>(
            &lua,
            "return V ~= pandoc.types.Version('0.36')"
        ));
        // Lua never calls __eq for mixed types; pandoc behaves the same.
        assert!(!eval::<bool>(&lua, "return V == '0.36.0'"));
        // Shorter prefix orders first, as in Data.Version.
        assert!(eval::<bool>(
            &lua,
            "return pandoc.types.Version('1.4') < pandoc.types.Version('1.4.0')"
        ));
    }

    #[test]
    fn constructor_accepts_pandoc_shapes() {
        let lua = lua();
        assert_eq!(
            eval::<String>(&lua, "return tostring(pandoc.types.Version('2.11.3.2'))"),
            "2.11.3.2"
        );
        assert_eq!(
            eval::<String>(&lua, "return tostring(pandoc.types.Version(3))"),
            "3"
        );
        assert_eq!(
            eval::<String>(&lua, "return tostring(pandoc.types.Version({1, 23, 1}))"),
            "1.23.1"
        );
        assert_eq!(
            eval::<String>(&lua, "return tostring(pandoc.types.Version(V))"),
            "0.36.0"
        );
        let err = lua
            .load("return pandoc.types.Version('1.2-beta')")
            .eval::<Value>()
            .unwrap_err()
            .to_string();
        assert!(err.contains("could not parse '1.2-beta'"), "{err}");
        let err = lua
            .load("return pandoc.types.Version(true)")
            .eval::<Value>()
            .unwrap_err()
            .to_string();
        assert!(err.contains("Version expected, got boolean"), "{err}");
    }

    #[test]
    fn must_be_at_least() {
        let lua = lua();
        lua.load("V:must_be_at_least('0.36')").exec().unwrap();
        lua.load("V:must_be_at_least(V)").exec().unwrap();
        let err = lua
            .load("V:must_be_at_least('1.4')")
            .exec()
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("expected version 1.4 or newer, got 0.36.0"),
            "{err}"
        );
        let err = lua
            .load("V:must_be_at_least('1.4', 'need %s, have %s')")
            .exec()
            .unwrap_err()
            .to_string();
        assert!(err.contains("need 1.4, have 0.36.0"), "{err}");
    }
}
