use crate::data::ItemType;
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
