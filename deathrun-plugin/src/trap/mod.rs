pub mod commands;
pub mod placement;
pub mod stored;
pub mod trigger;

pub use commands::{
    cmd_deltrap, cmd_edittrap, cmd_endtrap, cmd_newtrap, cmd_settrap, cmd_trapinfo, cmd_traptime,
    cmd_traptime_print, cmd_traptimeout, cmd_traptimeout_print,
};
pub use placement::{rebuild_all_trap_buttons, reset_button_colors_by_pos};
pub use stored::{StoredTrap, sync_traps_to_map};
pub use trigger::trigger_trap;

pub const BUTTON_SIZE: i32 = 8;
pub const BUTTON_FRONT_TEX: u16 = 248;
pub const BUTTON_SIDE_TEX: u16 = 242;
pub const BUTTON_COLOR_READY: [f32; 3] = [1.0, 1.0, 1.0];
pub const BUTTON_COLOR_ACTIVE: [f32; 3] = [0.2, 0.2, 0.2];

pub struct TrapEditState {
    pub trap_idx: usize,
    pub original_map: Box<sauer_map::Map>,
    pub is_edit: bool,
}
