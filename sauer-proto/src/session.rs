/// Connection configuration for stateful parsing
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Session {
    /// Whether the game is in `M_HOLD` is_hold_mode
    ///
    /// This determines the parsing of S2C `N_INITFLAGS` packet
    /// If its not correct, the packet will fail to parse and you will get error.
    pub is_hold_mode: bool,
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }
}
