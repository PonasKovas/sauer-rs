use crate::data::Vec3;

#[derive(Debug, Clone, PartialEq)]
pub struct FlagState {
    pub version: i32,
    pub spawn_index: i32,
    pub owner: FlagStateOwner,
    pub invis: bool,
    /// Must only be `Some` if the gamemode is `M_HOLD`!
    ///
    /// Client will fail to parse it otherwise and the stream will corrupt!
    pub hold: Option<FlagStateHoldTeam>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FlagStateOwner {
    Owned { owner: i32 },
    Dropped { dropped: bool, drop_loc: Vec3<f32> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum FlagStateHoldTeam {
    None,
    Team { team: i32, hold_time: i32 },
}

#[cfg(feature = "lua")]
const _: () = {
    use mlua::{IntoLua, Lua, Value};

    impl IntoLua for FlagState {
        fn into_lua(self, _lua: &Lua) -> mlua::Result<Value> {
            // not sure yet whats the best way to model FlagStateHoldTeam thing...
            // this api might change on rust side too, idk. since its a rare packet anyway
            // will just leave unimplemented for now
            //
            // shittiest packet in the game 100%
            Err(mlua::Error::ToLuaConversionError {
                from: "FlagState".to_owned(),
                to: "Value",
                message: Some("not yet implemented. shit ass packet".to_owned()),
            })
        }
    }
};
