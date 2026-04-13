use crate::{
    ParseError,
    data::Gun,
    internal::{ReaderCore, WriterCore},
};

/// Represents the game state of a player/client.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerState {
    pub life_sequence: i32,
    pub health: i32,
    pub max_health: i32,
    pub armour: i32,
    pub armour_type: i32,
    pub gun_select: Gun,
    /// Ammo count for each weapon (typically ordered from SG to PISTOL).
    pub ammo: Vec<i32>,
}

impl PlayerState {
    pub(crate) fn decode(r: &mut ReaderCore) -> Result<Self, ParseError> {
        let mut state = PlayerState {
            life_sequence: r.get_int()?,
            health: r.get_int()?,
            max_health: r.get_int()?,
            armour: r.get_int()?,
            armour_type: r.get_int()?,
            gun_select: Gun::try_from(r.get_int()?).unwrap_or(Gun::Fist),
            ammo: Vec::with_capacity(7),
        };
        // Guns from SG(1) to PISTOL(6) inclusive = 6 ammo slots
        for _ in 0..6 {
            state.ammo.push(r.get_int()?);
        }
        Ok(state)
    }

    pub(crate) fn encode(&self, w: &mut WriterCore) {
        w.put_int(self.life_sequence);
        w.put_int(self.health);
        w.put_int(self.max_health);
        w.put_int(self.armour);
        w.put_int(self.armour_type);
        w.put_int(self.gun_select as i32);

        for ammo_count in &self.ammo {
            w.put_int(*ammo_count);
        }
    }
}
