use crate::data::ItemType;

/// Information to represent a spawned item in the world.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemSpawn {
    pub item_index: i32,
    pub item_type: ItemType,
}
