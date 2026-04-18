use crate::data::{ItemType, Vec3};

#[derive(Debug, Clone, PartialEq)]
pub struct BaseSpawn {
    /// The ammo type stored at this base, or `None` if the base holds no ammo.
    pub ammotype: Option<ItemType>,
    pub pos: Vec3<f32>,
}

#[cfg(feature = "lua")]
const _: () = {
    use mlua::{UserData, UserDataFields};

    impl UserData for BaseSpawn {
        fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
            fields.add_field_method_get("ammotype", |_lua, this| Ok(this.ammotype));
            fields.add_field_method_get("pos", |_lua, this| Ok(this.pos));
        }
    }
};
