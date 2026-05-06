use crate::internal::{
    constants::{DMF, DNF},
    macros::{make_enum, make_struct},
    primitives::{read_varint_float, write_varint_float},
};
use cube_string::CubeString;
use num_enum::TryFromPrimitive;

mod base_info;
mod base_spawn;
mod direction;
mod edit_var_value;
mod flag_state;
pub(crate) mod vec3;
pub mod vslot_delta;

pub use base_info::BaseInfo;
pub use base_spawn::BaseSpawn;
pub use direction::Direction;
pub use edit_var_value::EditVarValue;
pub use flag_state::{FlagState, FlagStateHoldTeam, FlagStateOwner};
pub use vslot_delta::{VSlotDelta, VSlotDeltaParam};

pub use sauer_core::{EntityType, ItemType, Orient, Selection as SelInfo, Vec3};

impl crate::internal::SauerWrite for Orient {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        (*self as i32).sauer_write(out);
    }
}
impl crate::internal::SauerRead for Orient {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, crate::ParseError> {
        Ok(Orient::try_from(i32::sauer_read(reader)?).unwrap_or(Orient::Left))
    }
}
impl crate::internal::SauerWrite for EntityType {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        (i32::from(*self)).sauer_write(out);
    }
}
impl crate::internal::SauerRead for EntityType {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, crate::ParseError> {
        Ok(EntityType::try_from(i32::sauer_read(reader)?).unwrap_or(EntityType::Empty))
    }
}
impl crate::internal::SauerWrite for ItemType {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        (*self as i32).sauer_write(out);
    }
}
impl crate::internal::SauerRead for ItemType {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, crate::ParseError> {
        Ok(ItemType::try_from(i32::sauer_read(reader)?).unwrap_or(ItemType::Shells))
    }
}

impl crate::internal::SauerWrite for SelInfo {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        self.o.sauer_write(out);
        self.s.sauer_write(out);
        self.grid.sauer_write(out);
        self.orient.sauer_write(out);
        self.cx.sauer_write(out);
        self.cxs.sauer_write(out);
        self.cy.sauer_write(out);
        self.cys.sauer_write(out);
        self.corner.sauer_write(out);
    }
}
impl crate::internal::SauerRead for SelInfo {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, crate::ParseError> {
        Ok(SelInfo {
            o: crate::internal::SauerRead::sauer_read(reader)?,
            s: crate::internal::SauerRead::sauer_read(reader)?,
            grid: crate::internal::SauerRead::sauer_read(reader)?,
            orient: crate::internal::SauerRead::sauer_read(reader)?,
            cx: crate::internal::SauerRead::sauer_read(reader)?,
            cxs: crate::internal::SauerRead::sauer_read(reader)?,
            cy: crate::internal::SauerRead::sauer_read(reader)?,
            cys: crate::internal::SauerRead::sauer_read(reader)?,
            corner: crate::internal::SauerRead::sauer_read(reader)?,
        })
    }
}

make_enum!([i32] SetTeamReason {
    Switched = 0, Forced = 1,
});

make_enum!([i32] Gun {
    Fist = 0, Sg = 1, Cg = 2, Rl = 3, Rifle = 4, Gl = 5, Pistol = 6,
});

make_enum!([i32] MasterMode {
    Open = 0, Veto = 1, Locked = 2, Private = 3, Password = 4, Auth = 5,
});

make_enum!([i32] PlayerModel {
    MrFixit = 0, Snoutx10k = 1, Ogro2 = 2, Inky = 3, CaptainCannon = 4,
});

make_enum!(
    /// Game mode identifier, including local-only modes (negative values).
    [i32] GameMode {
    Sp = -3, Dmsp = -2, Demo = -1, Ffa = 0, CoopEdit = 1, TeamPlay = 2,
    Instagib = 3, InstaTeam = 4, Efficiency = 5, EfficTeam = 6,
    Tactics = 7, TacTeam = 8, Capture = 9, RegenCapture = 10, Ctf = 11,
    InstaCTF = 12, Protect = 13, InstaProtect = 14, Hold = 15,
    InstaHold = 16, EfficCTF = 17, EfficProtect = 18, EfficHold = 19,
    Collect = 20, InstaCollect = 21, EfficCollect = 22,
});

impl GameMode {
    pub fn has_teams(&self) -> bool {
        ![
            Self::Ffa,
            Self::CoopEdit,
            Self::Instagib,
            Self::Efficiency,
            Self::Tactics,
        ]
        .contains(self)
    }
    pub fn is_insta(&self) -> bool {
        [
            Self::Instagib,
            Self::InstaTeam,
            Self::InstaCTF,
            Self::InstaProtect,
            Self::InstaHold,
            Self::InstaCollect,
        ]
        .contains(self)
    }
}

make_enum!(
/// Server-side AI bot type.
[i32] AiType {
    None = 0, Bot = 1,
});

make_enum!(
/// Player armour type, determining damage reduction percentage.
[i32] ArmourType {
    /// Blue armour: 20% damage reduction.
    Blue = 0,
    /// Green armour: 40% damage reduction.
    Green = 1,
    /// Yellow armour: 60% damage reduction.
    Yellow = 2,
});

make_enum!(
/// Physical movement/contact state of an entity.
#[derive(Default)]
[u8] PhysState {
    #[default] Float = 0, Fall = 1, Slide = 2, Slope = 3, Floor = 4,
    StepUp = 5, StepDown = 6, Bounce = 7,
});

make_enum!(
/// Edit face operation mode.
[i32] EditFaceMode {
    /// Push or pull cube edges.
    Default = 0,
    /// Fill the selected region.
    Fill = 1,
    /// Corner edit mode.
    Corner = 2,
});

make_enum!([i32] ClientState {
    Alive = 0, Dead = 1, Spawning = 2, Lagged = 3, Editing = 4, Spectator = 5,
});

make_enum!([i32] Privilege {
    None = 0, Auth = 1, Master = 2, Admin = 3,
});

// No make_enum! macro for these two because they have some special retarded format on the wire

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, TryFromPrimitive)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(i8)]
pub enum Movement {
    Back = -1,
    None = 0,
    Forward = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, TryFromPrimitive)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(i8)]
pub enum Strafe {
    Left = -1,
    None = 0,
    Right = 1,
}

#[cfg(feature = "lua")]
const _: () = {
    use mlua::{FromLua, IntoLua, Lua, Value};

    impl IntoLua for Movement {
        fn into_lua(self, lua: &Lua) -> Result<Value, mlua::Error> {
            let s = match self {
                Movement::Back => "Back",
                Movement::None => "None",
                Movement::Forward => "Forward",
            };

            Ok(Value::String(lua.create_string(s.as_bytes())?))
        }
    }
    impl FromLua for Movement {
        fn from_lua(value: Value, lua: &Lua) -> mlua::Result<Self> {
            let s = String::from_lua(value, lua)?;
            match s.as_str() {
                "Back" => Ok(Movement::Back),
                "None" => Ok(Movement::None),
                "Forward" => Ok(Movement::Forward),
                _ => Err(mlua::Error::FromLuaConversionError {
                    from: "string",
                    to: "Movement".to_owned(),
                    message: Some(format!("unknown variant '{}' for Movement", s)),
                }),
            }
        }
    }

    impl IntoLua for Strafe {
        fn into_lua(self, lua: &Lua) -> Result<Value, mlua::Error> {
            let s = match self {
                Strafe::Left => "Left",
                Strafe::None => "None",
                Strafe::Right => "Right",
            };

            Ok(Value::String(lua.create_string(s.as_bytes())?))
        }
    }
    impl FromLua for Strafe {
        fn from_lua(value: Value, lua: &Lua) -> mlua::Result<Self> {
            let s = String::from_lua(value, lua)?;
            match s.as_str() {
                "Left" => Ok(Strafe::Left),
                "None" => Ok(Strafe::None),
                "Right" => Ok(Strafe::Right),
                _ => Err(mlua::Error::FromLuaConversionError {
                    from: "string",
                    to: "Strafe".to_owned(),
                    message: Some(format!("unknown variant '{}' for Strafe", s)),
                }),
            }
        }
    }
};

make_struct!(
/// State for a client resuming a game (e.g., late join).
ClientResumeState {
    client_num: i32,
    state: ClientState,
    frags: i32,
    flags: i32,
    deaths: i32,
    quad_millis: i32,
    player_state: PlayerState,
});

make_struct!(ClientTokenState {
    client_num: i32,
    tokens: i32,
});

make_struct!(DroppedToken {
    id: i32,
    team: i32,
    yaw: i32,
});

make_struct!(
/// Information to represent a spawned item in the world.
ItemSpawn {
    item_index: i32,
    item_type: ItemType,
});

make_struct!(
/// Represents a master/admin client configuration broadcast to everyone.
MasterClient {
    client_num: i32,
    privilege: Privilege,
});

make_struct!(
/// Ammo counts for each weapon slot.
PlayerAmmo {
    shotgun: i32,
    chaingun: i32,
    rocket_launcher: i32,
    rifle: i32,
    grenade_launcher: i32,
    pistol: i32,
});

make_struct!(
/// Represents the game state of a player/client.
PlayerState {
    life_sequence: i32,
    health: i32,
    max_health: i32,
    armour: i32,
    armour_type: ArmourType,
    gun_select: Gun,
    ammo: PlayerAmmo,
});

make_struct!(
/// Contains ray/hit information from a weapon shot or explosion.
HitInfo {
    target_cn: i32,
    life_sequence: i32,
    dist: f32 => write_varint_float::<DMF>, read_varint_float::<DMF>,
    rays: i32,
    dir: Vec3<f32> => vec3::write_quantized::<DNF>, vec3::read_quantized::<DNF>,
});

make_struct!(StolenToken { id: i32, yaw: i32 });

make_struct!(
/// Stores the number of frags a particular team has.
TeamInfo {
    team: CubeString,
    frags: i32,
});

make_struct!(TokenBaseSpawn {
    team: i32,
    pos: Vec3<f32> => vec3::write_quantized::<DMF>, vec3::read_quantized::<DMF>,
});

make_struct!(TokenState {
    id: i32,
    team: i32,
    yaw: i32,
    pos: Vec3<f32> => vec3::write_quantized::<DMF>, vec3::read_quantized::<DMF>,
});

make_struct!(FlagSpawn {
    team: i32,
    pos: Vec3<f32> => vec3::write_quantized::<DMF>, vec3::read_quantized::<DMF>,
});
