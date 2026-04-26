use sauer_server::plugin::Plugin;

pub struct DeathrunPlay;

impl DeathrunPlay {
    pub fn new() -> Self {
        Self
    }
}

impl Plugin for DeathrunPlay {}
