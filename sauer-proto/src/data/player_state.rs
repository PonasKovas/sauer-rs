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
    /// Ammo count for each weapon, ordered SG through PISTOL (6 slots).
    pub ammo: [i32; 6],
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
        let gun_select = Gun::try_from(r.get_int()?).unwrap_or(Gun::Fist);
        let mut ammo = [0i32; 6];
        for slot in &mut ammo {
            *slot = r.get_int()?;
        }
        Ok(PlayerState {
            life_sequence,
            health,
            max_health,
            armour,
            armour_type,
            gun_select,
            ammo,
        })
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
