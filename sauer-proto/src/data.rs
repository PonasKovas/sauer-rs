use num_enum::TryFromPrimitive;

mod base_info;
mod base_spawn;
mod client_resume_state;
mod client_token_state;
mod dropped_token;
mod edit_var_value;
mod flag_spawn;
mod flag_state;
mod hit_info;
mod item_spawn;
mod master_client;
mod player_position;
mod player_state;
mod sel_info;
mod stolen_token;
mod team_info;
mod token_base_spawn;
mod token_state;
mod vector3;

pub use base_info::BaseInfo;
pub use base_spawn::BaseSpawn;
pub use client_resume_state::{ClientResumeState, ClientState};
pub use client_token_state::ClientTokenState;
pub use dropped_token::DroppedToken;
pub use edit_var_value::EditVarValue;
pub use flag_spawn::FlagSpawn;
pub use flag_state::{FlagState, FlagStateHoldTeam, FlagStateOwner};
pub use hit_info::HitInfo;
pub use item_spawn::ItemSpawn;
pub use master_client::{MasterClient, Privilege};
pub use player_position::PlayerPosition;
pub use player_state::PlayerState;
pub use sel_info::SelInfo;
pub use stolen_token::StolenToken;
pub use team_info::TeamInfo;
pub use token_base_spawn::TokenBaseSpawn;
pub use token_state::TokenState;
pub use vector3::Vector3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, TryFromPrimitive)]
#[repr(i32)]
pub enum Gun {
    Fist = 0,
    Sg = 1,
    Cg = 2,
    Rl = 3,
    Rifle = 4,
    Gl = 5,
    Pistol = 6,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, TryFromPrimitive)]
#[repr(i32)]
pub enum ItemType {
    Shells = 1,
    Bullets = 2,
    Rockets = 3,
    Rounds = 4,
    Grenades = 5,
    Cartridges = 6,
    Health = 7,
    Boost = 8,
    GreenArmour = 9,
    YellowArmour = 10,
    Quad = 11,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, TryFromPrimitive)]
#[repr(i32)]
pub enum MasterMode {
    Open = 0,
    Veto = 1,
    Locked = 2,
    Private = 3,
    Password = 4,
    Auth = 5,
}

/// Player model selection (0-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, TryFromPrimitive)]
#[repr(i32)]
pub enum PlayerModel {
    MrFixit = 0,
    Snoutx10k = 1,
    Ogro2 = 2,
    Inky = 3,
    CaptainCannon = 4,
}

/// Game mode identifier, including local-only modes (negative values).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, TryFromPrimitive)]
#[repr(i32)]
pub enum GameMode {
    Sp = -3,
    Dmsp = -2,
    Demo = -1,
    Ffa = 0,
    CoopEdit = 1,
    TeamPlay = 2,
    Instagib = 3,
    InstaTeam = 4,
    Efficiency = 5,
    EfficTeam = 6,
    Tactics = 7,
    TacTeam = 8,
    Capture = 9,
    RegenCapture = 10,
    Ctf = 11,
    InstaCTF = 12,
    Protect = 13,
    InstaProtect = 14,
    Hold = 15,
    InstaHold = 16,
    EfficCTF = 17,
    EfficProtect = 18,
    EfficHold = 19,
    Collect = 20,
    InstaCollect = 21,
    EfficCollect = 22,
}

/// Server-side AI bot type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, TryFromPrimitive)]
#[repr(i32)]
pub enum AiType {
    None = 0,
    Bot = 1,
}

/// Player armour type, determining damage reduction percentage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, TryFromPrimitive)]
#[repr(i32)]
pub enum ArmourType {
    /// Blue armour: 20% damage reduction.
    Blue = 0,
    /// Green armour: 40% damage reduction.
    Green = 1,
    /// Yellow armour: 60% damage reduction.
    Yellow = 2,
}

/// Physical movement/contact state of an entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, TryFromPrimitive)]
#[repr(u8)]
pub enum PhysState {
    Float = 0,
    Fall = 1,
    Slide = 2,
    Slope = 3,
    Floor = 4,
    StepUp = 5,
    StepDown = 6,
    Bounce = 7,
}

/// Cube face orientation used in cooperative editing (O_LEFT through O_TOP).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, TryFromPrimitive)]
#[repr(i32)]
pub enum Orient {
    Left = 0,
    Right = 1,
    Back = 2,
    Front = 3,
    Bottom = 4,
    Top = 5,
}

/// Map entity type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, TryFromPrimitive)]
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
}

/// Edit face operation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, TryFromPrimitive)]
#[repr(i32)]
pub enum EditFaceMode {
    /// Push or pull cube edges.
    Default = 0,
    /// Fill the selected region.
    Fill = 1,
    /// Corner edit mode.
    Corner = 2,
}
