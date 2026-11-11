use crate::{
    ParseError,
    internal::{SauerRead, SauerWrite},
};
use std::fmt;

/// A client number: the engine's `clientnum`, which players see and type.
///
/// It's an `int` on the wire, so decoding checks the range. A packet that
/// names cn 256 or -1 fails to parse instead of wrapping onto another player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct Cn(pub u8);

impl Cn {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

impl fmt::Display for Cn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl TryFrom<i32> for Cn {
    type Error = ParseError;

    fn try_from(value: i32) -> Result<Self, ParseError> {
        u8::try_from(value)
            .map(Cn)
            .map_err(|_| ParseError::ValueOutOfRange {
                value,
                max: u8::MAX as i32,
            })
    }
}

impl From<Cn> for i32 {
    fn from(cn: Cn) -> i32 {
        cn.0 as i32
    }
}

impl SauerWrite for Cn {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        i32::from(*self).sauer_write(out);
    }
}

impl SauerRead for Cn {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        Cn::try_from(i32::sauer_read(reader)?)
    }
}

/// Fields where the engine sends -1 for "nobody" (the server, or no player).
impl SauerWrite for Option<Cn> {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        self.map_or(-1, i32::from).sauer_write(out);
    }
}

impl SauerRead for Option<Cn> {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        match i32::sauer_read(reader)? {
            -1 => Ok(None),
            v => Cn::try_from(v).map(Some),
        }
    }
}

#[cfg(feature = "lua")]
const _: () = {
    use mlua::{FromLua, IntoLua, Lua, Value};

    impl IntoLua for Cn {
        fn into_lua(self, lua: &Lua) -> mlua::Result<Value> {
            i32::from(self).into_lua(lua)
        }
    }

    impl FromLua for Cn {
        fn from_lua(value: Value, lua: &Lua) -> mlua::Result<Self> {
            let v = i32::from_lua(value, lua)?;
            Cn::try_from(v).map_err(|e| mlua::Error::FromLuaConversionError {
                from: "integer",
                to: "Cn".to_owned(),
                message: Some(e.to_string()),
            })
        }
    }
};
