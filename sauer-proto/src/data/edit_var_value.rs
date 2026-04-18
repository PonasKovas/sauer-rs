use cube_string::CubeString;

/// Strict typing for variable modification in cooperative editing (`N_EDITVAR`).
#[derive(Debug, Clone, PartialEq)]
pub enum EditVarValue {
    Int(i32),
    Float(f32),
    String(CubeString),
}

#[cfg(feature = "lua")]
const _: () = {
    use mlua::{IntoLua, Lua, Value};

    impl IntoLua for EditVarValue {
        fn into_lua(self, lua: &Lua) -> mlua::Result<Value> {
            match self {
                EditVarValue::Int(x) => x.into_lua(lua),
                EditVarValue::Float(x) => x.into_lua(lua),
                EditVarValue::String(x) => x.into_lua(lua),
            }
        }
    }
};
