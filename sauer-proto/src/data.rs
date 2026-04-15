use crate::internal::{
    SauerRead, SauerWrite,
    constants::{DMF, DNF},
    macros::{make_enum, make_struct},
};
use cube_string::CubeString;
use num_enum::TryFromPrimitive;

mod base_info;
mod base_spawn;
mod direction;
mod edit_var_value;
mod flag_state;
pub(crate) mod vec3;

pub use base_info::BaseInfo;
pub use base_spawn::BaseSpawn;
pub use direction::Direction;
pub use edit_var_value::EditVarValue;
pub use flag_state::{FlagState, FlagStateHoldTeam, FlagStateOwner};
pub use vec3::Vec3;

make_enum!([i32] Gun {
    Fist = 0, Sg = 1, Cg = 2, Rl = 3, Rifle = 4, Gl = 5, Pistol = 6,
});

make_enum!([i32] ItemType {
    Shells = 8, Bullets = 9, Rockets = 10, Rounds = 11, Grenades = 12,
    Cartridges = 13, Health = 14, Boost = 15, GreenArmour = 16,
    YellowArmour = 17, Quad = 18,
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
/// Cube face orientation used in cooperative editing (O_LEFT through O_TOP).
[i32] Orient {
    Left = 0, Right = 1, Back = 2, Front = 3, Bottom = 4, Top = 5,
});

make_enum!(
/// Map entity type.
[i32] EntityType {
    Empty = 0, Light = 1, MapModel = 2, PlayerStart = 3, EnvMap = 4,
    Particles = 5, MapSound = 6, SpotLight = 7, IShells = 8, IBullets = 9,
    IRockets = 10, IRounds = 11, IGrenades = 12, ICartridges = 13,
    IHealth = 14, IBoost = 15, IGreenArmour = 16, IYellowArmour = 17,
    IQuad = 18, Teleport = 19, TeleDest = 20, Monster = 21, Carrot = 22,
    JumpPad = 23, Base = 24, RespawnPoint = 25, Box = 26, Barrel = 27,
    Platform = 28, Elevator = 29, Flag = 30,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, TryFromPrimitive)]
#[repr(i8)]
pub enum Movement {
    Back = -1,
    None = 0,
    Forward = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, TryFromPrimitive)]
#[repr(i8)]
pub enum Strafe {
    Left = -1,
    None = 0,
    Right = 1,
}

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
/// Cube selection information for cooperative editing geometry modifications.
SelInfo {
    o: Vec3<i32>,
    s: Vec3<i32>,
    grid: i32,
    orient: Orient,
    cx: i32,
    cxs: i32,
    cy: i32,
    cys: i32,
    corner: i32,
});

make_struct!(
/// Contains ray/hit information from a weapon shot or explosion.
HitInfo {
    target_cn: i32,
    life_sequence: i32,
    dist: f32,
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
