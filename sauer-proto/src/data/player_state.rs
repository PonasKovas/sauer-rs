use crate::{
    ParseError,
    data::{ArmourType, Gun},
    internal::{ReaderCore, WriterCore},
};

/// Represents the game state of a player/client.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerState {
    pub life_sequence: i32,
    pub health: i32,
    pub max_health: i32,
    pub armour: i32,
    pub armour_type: ArmourType,
    pub gun_select: Gun,
    /// Ammo count for each weapon (typically ordered from SG to PISTOL).
    pub ammo: Vec<i32>,
}

impl PlayerState {
    pub(crate) fn decode(r: &mut ReaderCore) -> Result<Self, ParseError> {
        let life_sequence = r.get_int()?;
        let health = r.get_int()?;
        let max_health = r.get_int()?;
        let armour = r.get_int()?;
        let armour_type_val = r.get_int()?;
        let armour_type =
            ArmourType::try_from(armour_type_val).map_err(|_| ParseError::InvalidEnumValue {
                value: armour_type_val,
                enum_name: "ArmourType",
            })?;
        let mut state = PlayerState {
            life_sequence,
            health,
            max_health,
            armour,
            armour_type,
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
        w.put_int(self.armour_type as i32);
        w.put_int(self.gun_select as i32);

        for ammo_count in &self.ammo {
            w.put_int(*ammo_count);
        }
    }
}
