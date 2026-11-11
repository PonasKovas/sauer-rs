use sauer_proto::data::ClientState;

/// What a player is doing. The wire's [`ClientState`] has more values, but
/// these are the ones the server tracks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Alive,
    Dead,
    Spectator,
    /// In coop-edit mode. Leaving it goes back to being alive or dead.
    Editing {
        was_alive: bool,
    },
}

impl Status {
    pub fn client_state(self) -> ClientState {
        match self {
            Status::Alive => ClientState::Alive,
            Status::Dead => ClientState::Dead,
            Status::Spectator => ClientState::Spectator,
            Status::Editing { .. } => ClientState::Editing,
        }
    }

    /// Engine `N_EDITMODE`: only the living and the dead can start editing,
    /// and only an editor can stop. Returns false if the change isn't allowed.
    pub fn set_editing(&mut self, editing: bool) -> bool {
        *self = match (*self, editing) {
            (Status::Alive, true) => Status::Editing { was_alive: true },
            (Status::Dead, true) => Status::Editing { was_alive: false },
            (Status::Editing { was_alive }, false) => {
                if was_alive {
                    Status::Alive
                } else {
                    Status::Dead
                }
            }
            _ => return false,
        };
        true
    }
}
