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
