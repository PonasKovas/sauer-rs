pub mod commands;
pub mod ents;
pub mod guard;
pub mod persist;
pub mod place;
pub mod placement;
pub mod record;
pub mod trigger;
pub mod vault;

pub use commands::{
    cmd_cleartrap, cmd_deltrap, cmd_edittrap, cmd_trapinfo, cmd_traptime, cmd_traptime_print,
    cmd_traptimeout, cmd_traptimeout_print,
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
/// Preview tints while placing a button: fine, inside a trap's frame, can't go there.
pub const BUTTON_COLOR_GOOD: [f32; 3] = [0.2, 1.0, 0.2];
pub const BUTTON_COLOR_WARN: [f32; 3] = [1.0, 0.9, 0.1];
pub const BUTTON_COLOR_BAD: [f32; 3] = [1.0, 0.15, 0.15];
/// Button alpha while someone aims at it. Only takes effect on clients the
/// button was made see-through for (see `placement::update_seethrough_button`).
pub const BUTTON_SEETHROUGH_ALPHA: f32 = 0.3;

/// Limit for one trap's compressed geometry (what gets sent when it fires).
pub const TRAP_BUDGET: usize = 1024 * 1024;

/// A trap's identity. Unlike its index in `DeathrunState::traps` it stays the
/// same when other traps are deleted, and it's never reused, so whatever
/// holds one over time (a trigger task, an edit) can tell its trap is gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TrapId(u32);

impl std::fmt::Display for TrapId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl TrapId {
    fn next() -> Self {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        Self(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }
}

/// Where a trap is in its firing cycle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TrapPhase {
    #[default]
    Ready,
    /// Its active state is in the map.
    Firing,
    /// Back to its base state, but the button waits out the timeout.
    Cooldown,
}

pub struct TrapEditState {
    pub trap: TrapId,
    pub original_map: Box<sauer_map::Map>,
}

/// What the shared trap tools are doing. Editing a trap and placing a button
/// never happen at once: an edit records every change to the map, and
/// placing a button is one.
#[derive(Default)]
pub enum TrapMode {
    #[default]
    Idle,
    /// `#tedit`: every player's edits go into this trap's active state.
    Editing(TrapEditState),
    /// `#tnew` or `#tmove`: one player is choosing a spot for a button.
    Placing(place::Placing),
}

impl TrapMode {
    pub fn editing(&self) -> Option<&TrapEditState> {
        match self {
            Self::Editing(e) => Some(e),
            _ => None,
        }
    }

    pub fn placing(&self) -> Option<&place::Placing> {
        match self {
            Self::Placing(p) => Some(p),
            _ => None,
        }
    }

    /// Why map-wide actions (map upload, vote, /newmap) have to wait, if they do.
    pub fn busy_reason(&self) -> Option<&'static str> {
        match self {
            Self::Idle => None,
            Self::Editing(_) => Some("a trap is being edited (#tedit to finish)"),
            Self::Placing(_) => Some("a trap button is being placed"),
        }
    }
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
    /// Pastes the trap's geometry over its clip box.
    pub fn paste_into(&self, map: &mut sauer_map::Map) {
        let mut sel = self.payload.sel.clone();
        sel.o = sauer_proto::data::Vec3 {
            x: self.clip_pos[0],
            y: self.clip_pos[1],
            z: self.clip_pos[2],
        };
        if let Some(sel) = sel.validate(map.size) {
            map.apply_block(&sel, self.payload.nodes.clone(), false);
        }
    }

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
    pub id: TrapId,
    pub button_pos: [i32; 3],
    pub button_size: i32,
    pub button_orient: u8,

    pub geom: Option<TrapGeom>,
    pub ents: Vec<EntChange>,

    pub time: u32,
    pub timeout: u32,
    pub phase: TrapPhase,
}

impl Trap {
    pub fn new(button_pos: [i32; 3], button_size: i32, button_orient: u8) -> Self {
        Self {
            id: TrapId::next(),
            button_pos,
            button_size,
            button_orient,
            geom: None,
            ents: Vec::new(),
            time: 2000,
            timeout: 20000,
            phase: TrapPhase::Ready,
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
