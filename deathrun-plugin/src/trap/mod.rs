pub mod commands;
pub mod ents;
pub mod guard;
pub mod persist;
pub mod placement;
pub mod record;
pub mod trigger;
pub mod vault;

pub use commands::{
    cmd_deltrap, cmd_edittrap, cmd_endtrap, cmd_newtrap, cmd_settrap, cmd_trapinfo, cmd_traptime,
    cmd_traptime_print, cmd_traptimeout, cmd_traptimeout_print,
};
pub use ents::EntChange;
pub use placement::{rebuild_all_trap_buttons, reset_button_colors_by_pos};
pub use trigger::trigger_trap;

pub const BUTTON_SIZE: i32 = 8;
pub const BUTTON_FRONT_TEX: u16 = 248;
pub const BUTTON_SIDE_TEX: u16 = 351;
/// Texture scale of the button sides. 1/8 is the engine minimum (`vscale` clamps to it).
pub const BUTTON_SIDE_SCALE: f32 = 0.125;
pub const BUTTON_COLOR_READY: [f32; 3] = [1.0, 1.0, 1.0];
pub const BUTTON_COLOR_ACTIVE: [f32; 3] = [0.2, 0.2, 0.2];
/// Button alpha while someone aims at it. Only takes effect on clients the
/// button was made see-through for (see `placement::update_seethrough_button`).
pub const BUTTON_SEETHROUGH_ALPHA: f32 = 0.3;

/// Limit for one trap's compressed geometry (what gets sent when it fires).
pub const TRAP_BUDGET: usize = 1024 * 1024;

pub struct TrapEditState {
    pub trap_idx: usize,
    pub original_map: Box<sauer_map::Map>,
    pub is_edit: bool,
}

/// The geometry a trap pastes over its clip box when it fires.
#[derive(Clone)]
pub struct TrapGeom {
    pub clip_pos: [i32; 3],
    pub clip_s: [i32; 3],
    pub clip_grid: i32,
    pub payload: sauer_proto::data::ClipboardPayload,
    /// Where the copy lives in the vault, relative to the vault origin.
    /// `None` until the vault has placed it.
    pub shelf: Option<[i32; 3]>,
}

impl TrapGeom {
    pub fn extent(&self) -> [i32; 3] {
        self.clip_s.map(|s| s * self.clip_grid)
    }

    pub fn clip_box(&self) -> ([i32; 3], [i32; 3]) {
        let ext = self.extent();
        (
            self.clip_pos,
            std::array::from_fn(|i| self.clip_pos[i] + ext[i]),
        )
    }
}

#[derive(Clone)]
pub struct Trap {
    pub button_pos: [i32; 3],
    pub button_size: i32,
    pub button_orient: u8,

    pub geom: Option<TrapGeom>,
    pub ents: Vec<EntChange>,

    pub time: u32,
    pub timeout: u32,
    pub is_active: bool,
}

impl Trap {
    pub fn new(button_pos: [i32; 3], button_size: i32, button_orient: u8) -> Self {
        Self {
            button_pos,
            button_size,
            button_orient,
            geom: None,
            ents: Vec::new(),
            time: 2000,
            timeout: 20000,
            is_active: false,
        }
    }

    pub fn button_box(&self) -> ([i32; 3], [i32; 3]) {
        (
            self.button_pos,
            self.button_pos.map(|v| v + self.button_size),
        )
    }

    pub fn contains_button_point(&self, p: [f32; 3], margin: f32) -> bool {
        let (min, max) = self.button_box();
        (0..3).all(|ax| p[ax] >= min[ax] as f32 - margin && p[ax] < max[ax] as f32 + margin)
    }

    pub fn does_anything(&self) -> bool {
        self.geom.is_some() || !self.ents.is_empty()
    }
}
