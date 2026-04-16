use crate::data::Vec3;

#[derive(Debug, Clone, PartialEq)]
pub struct FlagState {
    pub version: i32,
    pub spawn_index: i32,
    pub owner: FlagStateOwner,
    pub invis: bool,
    /// Must only be `Some` if the gamemode is `M_HOLD`!
    ///
    /// Client will fail to parse it otherwise and the stream will corrupt!
    pub hold: Option<FlagStateHoldTeam>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FlagStateOwner {
    Owned { owner: i32 },
    Dropped { dropped: bool, drop_loc: Vec3<f32> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum FlagStateHoldTeam {
    None,
    Team { team: i32, hold_time: i32 },
}
