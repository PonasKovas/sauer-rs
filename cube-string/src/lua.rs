//! Lua strings are UTF-8 (script source is), so each cube byte crosses as its
//! own char, color codes included, the same way serde does it.

use crate::{CubeStr, CubeString, raw};
use alloc::{borrow::ToOwned, string::String};
use mlua::{FromLua, IntoLua, Lua, Value};

impl IntoLua for CubeString {
    fn into_lua(self, lua: &Lua) -> mlua::Result<Value> {
        (&*self).into_lua(lua)
    }
}

impl<'a> IntoLua for &'a CubeStr {
    fn into_lua(self, lua: &Lua) -> mlua::Result<Value> {
        let s: String = self.as_bytes().iter().map(|&b| raw::cube2uni(b)).collect();
        Ok(Value::String(lua.create_string(s)?))
    }
}

impl FromLua for CubeString {
    fn from_lua(value: Value, _lua: &Lua) -> mlua::Result<Self> {
        let error = |message: String| mlua::Error::FromLuaConversionError {
            from: "string",
            to: "CubeString".to_owned(),
            message: Some(message),
        };
        match value {
            Value::Nil => Ok(CubeString::new()),
            Value::String(s) => {
                let s = s.to_str().map_err(|e| error(alloc::format!("{e}")))?;
                CubeString::from_str(&s).map_err(|e| error(alloc::format!("{e}")))
            }
            other => Err(mlua::Error::FromLuaConversionError {
                from: other.type_name(),
                to: "CubeString".to_owned(),
                message: Some("expected string".to_owned()),
            }),
        }
    }
}
