use crate::{
    ParseError,
    data::{PhysState, Vector3},
    internal::{ReaderCore, WriterCore, constants::*},
};

/// Packed physstate byte as transmitted in the Pos packet:
/// bits 0-2: phys state, bit 3: life sequence, bits 4-5: move, bits 6-7: strafe
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PositionPhysState {
    pub phys_state: PhysState,
    pub life_sequence: bool,
    /// Movement direction: -1, 0, or 1
    pub move_dir: i8,
    /// Strafe direction: -1, 0, or 1
    pub strafe: i8,
}

impl PositionPhysState {
    pub fn from_bits(bits: u8) -> Self {
        let phys_state = PhysState::try_from(bits & 0b111).unwrap_or_default();
        let life_sequence = (bits & (1 << 3)) != 0;
        let move_raw = (bits >> 4) & 0b11;
        let strafe_raw = (bits >> 6) & 0b11;
        // 2-bit sign extension: 0b11 (3) -> -1, 0b10 (2) -> -2, etc.
        let move_dir = if move_raw >= 2 {
            move_raw as i8 - 4
        } else {
            move_raw as i8
        };
        let strafe = if strafe_raw >= 2 {
            strafe_raw as i8 - 4
        } else {
            strafe_raw as i8
        };
        Self {
            phys_state,
            life_sequence,
            move_dir,
            strafe,
        }
    }

    pub fn to_bits(self) -> u8 {
        (self.phys_state as u8)
            | ((self.life_sequence as u8) << 3)
            | (((self.move_dir as u8) & 0b11) << 4)
            | (((self.strafe as u8) & 0b11) << 6)
    }
}

/// Complete definition of a physical position broadcast over Channel 0.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerPosition {
    pub client_num: u32,
    /// Packed phys state byte: phys state (3 bits), life sequence (1 bit), move (2 bits), strafe (2 bits).
    pub phys_state: PositionPhysState,
    pub flags: u32,
    pub pos: Vector3<f32>,
    pub dir: u16,
    pub roll: u8,
    pub mag: u16,
    pub vel_dir: u16,
    pub fall_mag: u16,
    pub fall_dir: u16,
}

impl PlayerPosition {
    pub(crate) fn decode(r: &mut ReaderCore) -> Result<Self, ParseError> {
        let client_num = r.get_uint()?;
        let phys_state = PositionPhysState::from_bits(r.get_byte()?);
        let flags = r.get_uint()?;

        let mut coords = [0.0; 3];
        for (k, coord) in coords.iter_mut().enumerate() {
            let mut n = (r.get_byte()? as i32) | ((r.get_byte()? as i32) << 8);
            if (flags & (1 << k)) != 0 {
                n |= (r.get_byte()? as i32) << 16;
                if (n & 0x800000) != 0 {
                    n |= -16777216;
                } // Sign extension
            }
            *coord = n as f32 / DMF;
        }

        let dir = (r.get_byte()? as u16) | ((r.get_byte()? as u16) << 8);
        let roll = r.get_byte()?;

        let mut mag = r.get_byte()? as u16;
        if (flags & (1 << 3)) != 0 {
            mag |= (r.get_byte()? as u16) << 8;
        }

        let vel_dir = (r.get_byte()? as u16) | ((r.get_byte()? as u16) << 8);

        let mut fall_mag = 0;
        let mut fall_dir = 0;
        if (flags & (1 << 4)) != 0 {
            fall_mag = r.get_byte()? as u16;
            if (flags & (1 << 5)) != 0 {
                fall_mag |= (r.get_byte()? as u16) << 8;
            }
            if (flags & (1 << 6)) != 0 {
                fall_dir = (r.get_byte()? as u16) | ((r.get_byte()? as u16) << 8);
            }
        }

        Ok(PlayerPosition {
            client_num,
            phys_state,
            flags,
            pos: Vector3 {
                x: coords[0],
                y: coords[1],
                z: coords[2],
            },
            dir,
            roll,
            mag,
            vel_dir,
            fall_mag,
            fall_dir,
        })
    }

    pub(crate) fn encode(&self, w: &mut WriterCore) {
        w.put_uint(self.client_num);
        w.put_byte(self.phys_state.to_bits());
        w.put_uint(self.flags);

        let coords = [self.pos.x, self.pos.y, self.pos.z];
        for (k, coord) in coords.iter().enumerate() {
            let n = (coord * DMF) as i32;
            w.put_byte((n & 0xFF) as u8);
            w.put_byte(((n >> 8) & 0xFF) as u8);
            if (self.flags & (1 << k)) != 0 {
                w.put_byte(((n >> 16) & 0xFF) as u8);
            }
        }

        w.put_byte((self.dir & 0xFF) as u8);
        w.put_byte(((self.dir >> 8) & 0xFF) as u8);
        w.put_byte(self.roll);

        w.put_byte((self.mag & 0xFF) as u8);
        if (self.flags & (1 << 3)) != 0 {
            w.put_byte(((self.mag >> 8) & 0xFF) as u8);
        }

        w.put_byte((self.vel_dir & 0xFF) as u8);
        w.put_byte(((self.vel_dir >> 8) & 0xFF) as u8);

        if (self.flags & (1 << 4)) != 0 {
            w.put_byte((self.fall_mag & 0xFF) as u8);
            if (self.flags & (1 << 5)) != 0 {
                w.put_byte(((self.fall_mag >> 8) & 0xFF) as u8);
            }
            if (self.flags & (1 << 6)) != 0 {
                w.put_byte((self.fall_dir & 0xFF) as u8);
                w.put_byte(((self.fall_dir >> 8) & 0xFF) as u8);
            }
        }
    }
}
