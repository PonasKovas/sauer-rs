use crate::{
    ParseError,
    data::{ItemType, Vec3, vec3},
    internal::{SauerRead, SauerWrite, constants::DMF},
};

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BaseSpawn {
    /// The ammo type stored at this base, or `None` if the base holds no ammo.
    pub ammotype: Option<ItemType>,
    pub pos: Vec3<f32>,
}

impl SauerWrite for BaseSpawn {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        self.ammotype.sauer_write(out);
        vec3::write_quantized::<DMF>(&self.pos, out);
    }
}

impl SauerRead for BaseSpawn {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        Ok(Self {
            ammotype: Option::<ItemType>::sauer_read(reader)?,
            pos: vec3::read_quantized::<DMF>(reader)?,
        })
    }
}

#[cfg(feature = "lua")]
const _: () = {
    use mlua::{FromLua, UserData, UserDataFields};

    impl UserData for BaseSpawn {
        fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
            fields.add_field_method_get("ammotype", |_lua, this| Ok(this.ammotype));
            fields.add_field_method_get("pos", |_lua, this| Ok(this.pos));
        }
    }

    impl FromLua for BaseSpawn {
        fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self> {
            let table = mlua::Table::from_lua(value, lua)?;
            Ok(Self {
                ammotype: table.get("ammotype")?,
                pos: table.get("pos")?,
            })
        }
    }
};
