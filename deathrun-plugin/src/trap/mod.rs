use sauer_proto::channel1;

pub mod commands;
pub mod intersect;
pub mod placement;
pub mod stored;
pub mod trigger;
pub mod vslot_store;

pub use commands::{
    cmd_deltrap, cmd_edittrap, cmd_endtrap, cmd_newtrap, cmd_settrap, cmd_traptime,
    cmd_traptimeout, cmd_traptime_print, cmd_traptimeout_print,
};
pub use intersect::{intersects_bounds, intersects_edit_face, intersects_sel};
pub use placement::{
    check_and_rebuild_traps, rebuild_all_trap_buttons, reset_button_colors_by_pos,
};
pub use stored::{StoredTrap, sync_traps_to_map};
pub use trigger::trigger_trap;

pub const BUTTON_SIZE: i32 = 8;
pub const BUTTON_FRONT_TEX: u16 = 248;
pub const BUTTON_SIDE_TEX: u16 = 242;
pub const BUTTON_COLOR_READY: [f32; 3] = [1.0, 1.0, 1.0];
pub const BUTTON_COLOR_ACTIVE: [f32; 3] = [0.2, 0.2, 0.2];

pub const TRAP_STORE_VERSION: u8 = 3;
pub const TRAP_DOMAIN: u8 = 1;

pub const ACTIVE_TRAP_STORE_VERSION: u8 = 1;
pub const ACTIVE_TRAP_DOMAIN: u8 = 2;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ActiveTrap {
    pub trap_idx: usize,
    pub progress_bar_ent_id: Option<usize>,
    pub assigned_ids: Vec<usize>,
}

#[derive(Clone)]
pub enum RecordedPacket {
    EditFace(channel1::EditFace),
    EditTex(channel1::EditTex),
    EditMat(channel1::EditMat),
    DelCube(channel1::DelCube),
    EditVSlot(channel1::EditVSlot),
    Replace(channel1::Replace),
    Flip(channel1::Flip),
    Rotate(channel1::Rotate),
    EditEnt(channel1::EditEnt),
}

pub struct TrapEditState {
    pub trap_idx: usize,
    pub original_map: Box<crate::map_state::MapState>,
    pub affected_entities: std::collections::HashSet<usize>,
}
