use crate::data::Vector3;

#[derive(Debug, Clone, PartialEq)]
pub struct FlagState {
    pub version: i32,
    pub spawn_index: i32,
    pub owner: FlagStateOwner,
    pub invis: bool,
    pub hold: Option<FlagStateHoldTeam>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FlagStateOwner {
    Owned {
        owner: i32,
    },
    Dropped {
        dropped: bool,
        drop_loc: Vector3<f32>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum FlagStateHoldTeam {
    None,
    Team { team: i32, hold_time: i32 },
}
