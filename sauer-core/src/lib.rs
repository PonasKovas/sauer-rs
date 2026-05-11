use cube_string::CubeString;
use num_enum::{FromPrimitive, IntoPrimitive, TryFromPrimitive};

pub mod io;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Vec3<T> {
    pub x: T,
    pub y: T,
    pub z: T,
}

impl<T> std::ops::Index<usize> for Vec3<T> {
    type Output = T;
    fn index(&self, index: usize) -> &Self::Output {
        match index {
            0 => &self.x,
            1 => &self.y,
            2 => &self.z,
            _ => panic!("Index out of bounds for Vec3"),
        }
    }
}

impl<T> std::ops::IndexMut<usize> for Vec3<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        match index {
            0 => &mut self.x,
            1 => &mut self.y,
            2 => &mut self.z,
            _ => panic!("Index out of bounds for Vec3"),
        }
    }
}

#[cfg(feature = "lua")]
const _: () = {
    use mlua::{FromLua, IntoLua, UserData, UserDataFields};
    impl<T: IntoLua + Clone> UserData for Vec3<T> {
        fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
            fields.add_field_method_get("x", |_lua, this| Ok(this.x.clone()));
            fields.add_field_method_get("y", |_lua, this| Ok(this.y.clone()));
            fields.add_field_method_get("z", |_lua, this| Ok(this.z.clone()));
        }
    }
    impl<T: FromLua> FromLua for Vec3<T> {
        fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self> {
            let table = mlua::Table::from_lua(value, lua)?;
            Ok(Self {
                x: table.get("x")?,
                y: table.get("y")?,
                z: table.get("z")?,
            })
        }
    }
};

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, TryFromPrimitive, IntoPrimitive,
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(i32)]
pub enum Orient {
    Left = 0,
    Right = 1,
    Back = 2,
    Front = 3,
    Bottom = 4,
    Top = 5,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Selection {
    pub o: Vec3<i32>,
    pub s: Vec3<i32>,
    pub grid: i32,
    pub orient: Orient,
    pub cx: i32,
    pub cxs: i32,
    pub cy: i32,
    pub cys: i32,
    pub corner: i32,
}

#[cfg(feature = "lua")]
const _: () = {
    use mlua::{FromLua, IntoLua, UserData, UserDataFields, Value};

    impl UserData for Selection {
        fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
            fields.add_field_method_get("o", |_lua, this| Ok(this.o.clone()));
            fields.add_field_method_get("s", |_lua, this| Ok(this.s.clone()));
            fields.add_field_method_get("grid", |_lua, this| Ok(this.grid));
            fields.add_field_method_get("orient", |_lua, this| Ok(this.orient));
            fields.add_field_method_get("cx", |_lua, this| Ok(this.cx));
            fields.add_field_method_get("cxs", |_lua, this| Ok(this.cxs));
            fields.add_field_method_get("cy", |_lua, this| Ok(this.cy));
            fields.add_field_method_get("cys", |_lua, this| Ok(this.cys));
            fields.add_field_method_get("corner", |_lua, this| Ok(this.corner));
        }
    }

    impl FromLua for Selection {
        fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self> {
            let table = mlua::Table::from_lua(value, lua)?;
            Ok(Self {
                o: table.get("o")?,
                s: table.get("s")?,
                grid: table.get("grid")?,
                orient: table.get("orient")?,
                cx: table.get("cx")?,
                cxs: table.get("cxs")?,
                cy: table.get("cy")?,
                cys: table.get("cys")?,
                corner: table.get("corner")?,
            })
        }
    }

    macro_rules! impl_lua_enum {
        ($name:ident, $( $variant:ident ),+ ) => {
            impl IntoLua for $name {
                fn into_lua(self, lua: &mlua::Lua) -> mlua::Result<Value> {
                    let s = match self {
                        $( $name::$variant => stringify!($variant), )+
                    };
                    Ok(Value::String(lua.create_string(s.as_bytes())?))
                }
            }
            impl FromLua for $name {
                fn from_lua(value: Value, lua: &mlua::Lua) -> mlua::Result<Self> {
                    let s = String::from_lua(value, lua)?;
                    match s.as_str() {
                        $( stringify!($variant) => Ok($name::$variant), )+
                        _ => Err(mlua::Error::FromLuaConversionError {
                            from: "string",
                            to: stringify!($name).to_owned(),
                            message: Some(format!("unknown variant '{}'", s)),
                        }),
                    }
                }
            }
        };
    }

    impl_lua_enum!(Orient, Left, Right, Back, Front, Bottom, Top);
    impl_lua_enum!(
        ItemType,
        Shells,
        Bullets,
        Rockets,
        Rounds,
        Grenades,
        Cartridges,
        Health,
        Boost,
        GreenArmour,
        YellowArmour,
        Quad
    );

    impl IntoLua for EntityType {
        fn into_lua(self, lua: &mlua::Lua) -> mlua::Result<Value> {
            let s = match self {
                EntityType::Empty => "Empty",
                EntityType::Light => "Light",
                EntityType::MapModel => "MapModel",
                EntityType::PlayerStart => "PlayerStart",
                EntityType::EnvMap => "EnvMap",
                EntityType::Particles => "Particles",
                EntityType::MapSound => "MapSound",
                EntityType::SpotLight => "SpotLight",
                EntityType::IShells => "IShells",
                EntityType::IBullets => "IBullets",
                EntityType::IRockets => "IRockets",
                EntityType::IRounds => "IRounds",
                EntityType::IGrenades => "IGrenades",
                EntityType::ICartridges => "ICartridges",
                EntityType::IHealth => "IHealth",
                EntityType::IBoost => "IBoost",
                EntityType::IGreenArmour => "IGreenArmour",
                EntityType::IYellowArmour => "IYellowArmour",
                EntityType::IQuad => "IQuad",
                EntityType::Teleport => "Teleport",
                EntityType::TeleDest => "TeleDest",
                EntityType::Monster => "Monster",
                EntityType::Carrot => "Carrot",
                EntityType::JumpPad => "JumpPad",
                EntityType::Base => "Base",
                EntityType::RespawnPoint => "RespawnPoint",
                EntityType::Box => "Box",
                EntityType::Barrel => "Barrel",
                EntityType::Platform => "Platform",
                EntityType::Elevator => "Elevator",
                EntityType::Flag => "Flag",
                EntityType::GameSpecific(id) => return id.into_lua(lua),
            };
            Ok(Value::String(lua.create_string(s.as_bytes())?))
        }
    }

    impl FromLua for EntityType {
        fn from_lua(value: Value, lua: &mlua::Lua) -> mlua::Result<Self> {
            if let Value::Integer(i) = value {
                return Ok(EntityType::GameSpecific(i as i32));
            }
            let s = String::from_lua(value, lua)?;
            match s.as_str() {
                "Empty" => Ok(EntityType::Empty),
                "Light" => Ok(EntityType::Light),
                "MapModel" => Ok(EntityType::MapModel),
                "PlayerStart" => Ok(EntityType::PlayerStart),
                "EnvMap" => Ok(EntityType::EnvMap),
                "Particles" => Ok(EntityType::Particles),
                "MapSound" => Ok(EntityType::MapSound),
                "SpotLight" => Ok(EntityType::SpotLight),
                "IShells" => Ok(EntityType::IShells),
                "IBullets" => Ok(EntityType::IBullets),
                "IRockets" => Ok(EntityType::IRockets),
                "IRounds" => Ok(EntityType::IRounds),
                "IGrenades" => Ok(EntityType::IGrenades),
                "ICartridges" => Ok(EntityType::ICartridges),
                "IHealth" => Ok(EntityType::IHealth),
                "IBoost" => Ok(EntityType::IBoost),
                "IGreenArmour" => Ok(EntityType::IGreenArmour),
                "IYellowArmour" => Ok(EntityType::IYellowArmour),
                "IQuad" => Ok(EntityType::IQuad),
                "Teleport" => Ok(EntityType::Teleport),
                "TeleDest" => Ok(EntityType::TeleDest),
                "Monster" => Ok(EntityType::Monster),
                "Carrot" => Ok(EntityType::Carrot),
                "JumpPad" => Ok(EntityType::JumpPad),
                "Base" => Ok(EntityType::Base),
                "RespawnPoint" => Ok(EntityType::RespawnPoint),
                "Box" => Ok(EntityType::Box),
                "Barrel" => Ok(EntityType::Barrel),
                "Platform" => Ok(EntityType::Platform),
                "Elevator" => Ok(EntityType::Elevator),
                "Flag" => Ok(EntityType::Flag),
                _ => Err(mlua::Error::FromLuaConversionError {
                    from: "string",
                    to: "EntityType".to_owned(),
                    message: Some(format!("unknown variant '{}'", s)),
                }),
            }
        }
    }
};
impl Selection {
    pub fn bounds(&self) -> ([i32; 3], [i32; 3]) {
        let min = [self.o.x, self.o.y, self.o.z];
        let max = [
            min[0] + self.s.x * self.grid,
            min[1] + self.s.y * self.grid,
            min[2] + self.s.z * self.grid,
        ];
        (min, max)
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, FromPrimitive, IntoPrimitive,
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum Volume {
    #[default]
    Air = 0,
    Water,
    Lava,
    Glass,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, FromPrimitive, IntoPrimitive,
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum Clip {
    #[default]
    None = 0,
    NoClip,
    Clip,
    GameClip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Material {
    pub volume: Volume,
    pub clip: Clip,
    pub death: bool,
    pub alpha: bool,
}
impl Material {
    pub fn from_bits(bits: u16) -> Self {
        Self {
            volume: Volume::from(((bits >> 2) as u8) & 0b11),
            clip: Clip::from((bits >> 5) as u8 & 0b11),
            death: (bits & (1 << 8)) != 0,
            alpha: (bits & (1 << 10)) != 0,
        }
    }
    pub fn to_bits(self) -> u16 {
        ((self.volume as u16) << 2)
            | ((self.clip as u16) << 5)
            | ((self.death as u16) << 8)
            | ((self.alpha as u16) << 10)
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Node {
    Branch(Box<[Node; 8]>),
    Leaf(Cube),
}

#[derive(Debug, Clone, PartialEq, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Cube {
    pub edges: [u8; 12],
    pub textures: [u16; 6],
    pub material: Material,
}
impl Cube {
    pub fn is_empty(&self) -> bool {
        self.edges == [0x00; 12]
    }
    pub fn is_entirely_solid(&self) -> bool {
        self.edges == [0x80; 12]
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VSlotParam {
    pub name: CubeString,
    pub values: [f32; 4],
}

#[derive(Debug, Clone, Default, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VSlot {
    pub prev: i32,
    pub params: Vec<VSlotParam>,
    pub scale: Option<f32>,
    pub rotation: Option<i32>,
    pub offset: Option<(i32, i32)>,
    pub scroll: Option<(f32, f32)>,
    pub layer: Option<i32>,
    pub alpha: Option<(f32, f32)>,
    pub color: Option<(f32, f32, f32)>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, TryFromPrimitive, IntoPrimitive,
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(i32)]
pub enum EntityType {
    Empty = 0,
    Light = 1,
    MapModel = 2,
    PlayerStart = 3,
    EnvMap = 4,
    Particles = 5,
    MapSound = 6,
    SpotLight = 7,
    IShells = 8,
    IBullets = 9,
    IRockets = 10,
    IRounds = 11,
    IGrenades = 12,
    ICartridges = 13,
    IHealth = 14,
    IBoost = 15,
    IGreenArmour = 16,
    IYellowArmour = 17,
    IQuad = 18,
    Teleport = 19,
    TeleDest = 20,
    Monster = 21,
    Carrot = 22,
    JumpPad = 23,
    Base = 24,
    RespawnPoint = 25,
    Box = 26,
    Barrel = 27,
    Platform = 28,
    Elevator = 29,
    Flag = 30,
    #[num_enum(catch_all)]
    GameSpecific(i32),
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, TryFromPrimitive, IntoPrimitive,
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(i32)]
pub enum ItemType {
    Shells = 8,
    Bullets = 9,
    Rockets = 10,
    Rounds = 11,
    Grenades = 12,
    Cartridges = 13,
    Health = 14,
    Boost = 15,
    GreenArmour = 16,
    YellowArmour = 17,
    Quad = 18,
}
impl TryFrom<EntityType> for ItemType {
    type Error = ();
    fn try_from(ent_type: EntityType) -> Result<Self, Self::Error> {
        match ent_type {
            EntityType::IShells => Ok(ItemType::Shells),
            EntityType::IBullets => Ok(ItemType::Bullets),
            EntityType::IRockets => Ok(ItemType::Rockets),
            EntityType::IRounds => Ok(ItemType::Rounds),
            EntityType::IGrenades => Ok(ItemType::Grenades),
            EntityType::ICartridges => Ok(ItemType::Cartridges),
            EntityType::IHealth => Ok(ItemType::Health),
            EntityType::IBoost => Ok(ItemType::Boost),
            EntityType::IGreenArmour => Ok(ItemType::GreenArmour),
            EntityType::IYellowArmour => Ok(ItemType::YellowArmour),
            EntityType::IQuad => Ok(ItemType::Quad),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Entity {
    pub position: [f32; 3],
    pub attrs: [i16; 5],
    pub ent_type: EntityType,
    pub extra_data: Vec<u8>,
}
