use crate::{CubeStr, CubeString};
use alloc::borrow::ToOwned;
use mlua::{FromLua, IntoLua, Lua, Value};

impl IntoLua for CubeString {
    fn into_lua(self, lua: &Lua) -> mlua::Result<Value> {
        (&*self).into_lua(lua)
    }
}

impl<'a> IntoLua for &'a CubeStr {
    fn into_lua(self, lua: &Lua) -> mlua::Result<Value> {
        Ok(Value::String(lua.create_string(self.as_bytes())?))
    }
}

impl FromLua for CubeString {
    fn from_lua(value: Value, _lua: &Lua) -> mlua::Result<Self> {
        match value {
            Value::Nil => Ok(CubeString::new()),
            Value::String(s) => Ok(CubeString::from_bytes(s.as_bytes().to_vec())),
            other => Err(mlua::Error::FromLuaConversionError {
                from: other.type_name(),
                to: "CubeString".to_owned(),
                message: Some("expected string".to_owned()),
            }),
        }
    }
}
