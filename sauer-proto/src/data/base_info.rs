use crate::{
    ParseError,
    data::ItemType,
    error::EnumConvertResultExt,
    internal::{SauerRead, SauerWrite},
};
use cube_string::CubeString;

#[derive(Debug, Clone, PartialEq)]
pub struct BaseInfo {
    /// The ammo type stored at this base, or `None` if the base holds no ammo.
    pub ammotype: Option<ItemType>,
    pub owner: CubeString,
    pub enemy: CubeString,
    pub converted: bool,
    pub ammo: i32,
}

impl SauerWrite for BaseInfo {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        self.ammotype.map_or(0, |t| t as i32).sauer_write(out);
        self.owner.sauer_write(out);
        self.enemy.sauer_write(out);
        self.converted.sauer_write(out);
        self.ammo.sauer_write(out);
    }
}

impl SauerRead for BaseInfo {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        let ammotype_val = i32::sauer_read(reader)?;
        let ammotype = if ammotype_val == 0 {
            None
        } else {
            Some(ItemType::try_from(ammotype_val).in_enum("ItemType")?)
        };

        Ok(Self {
            ammotype,
            owner: CubeString::sauer_read(reader)?,
            enemy: CubeString::sauer_read(reader)?,
            converted: bool::sauer_read(reader)?,
            ammo: i32::sauer_read(reader)?,
        })
    }
}

#[cfg(feature = "lua")]
const _: () = {
    use mlua::{FromLua, UserData, UserDataFields};

    impl UserData for BaseInfo {
        fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
            fields.add_field_method_get("ammotype", |_lua, this| Ok(this.ammotype));
            fields.add_field_method_get("owner", |_lua, this| Ok(this.owner.clone()));
            fields.add_field_method_get("enemy", |_lua, this| Ok(this.enemy.clone()));
            fields.add_field_method_get("converted", |_lua, this| Ok(this.converted));
            fields.add_field_method_get("ammo", |_lua, this| Ok(this.ammo));
        }
    }

    impl FromLua for BaseInfo {
        fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self> {
            let table = mlua::Table::from_lua(value, lua)?;
            Ok(Self {
                ammotype: table.get("ammotype")?,
                owner: table.get("owner")?,
                enemy: table.get("enemy")?,
                converted: table.get("converted")?,
                ammo: table.get("ammo")?,
            })
        }
    }
};
