use cube_string::CubeString;

/// Strict typing for variable modification in cooperative editing (`N_EDITVAR`).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EditVarValue {
    Int(i32),
    Float(f32),
    String(CubeString),
}

#[cfg(feature = "lua")]
const _: () = {
    use mlua::{FromLua, IntoLua, Lua, Value};

    impl IntoLua for EditVarValue {
        fn into_lua(self, lua: &Lua) -> mlua::Result<Value> {
            match self {
                EditVarValue::Int(x) => x.into_lua(lua),
                EditVarValue::Float(x) => x.into_lua(lua),
                EditVarValue::String(x) => x.into_lua(lua),
            }
        }
    }

    impl FromLua for EditVarValue {
        fn from_lua(value: Value, lua: &Lua) -> mlua::Result<Self> {
            match &value {
                Value::Integer(_) => Ok(EditVarValue::Int(i32::from_lua(value, lua)?)),
                Value::Number(_) => Ok(EditVarValue::Float(f32::from_lua(value, lua)?)),
                Value::String(_) => Ok(EditVarValue::String(CubeString::from_lua(value, lua)?)),
                other => Err(mlua::Error::FromLuaConversionError {
                    from: other.type_name(),
                    to: "EditVarValue".to_owned(),
                    message: Some("expected integer, number, or string".to_owned()),
                }),
            }
        }
    }
};
