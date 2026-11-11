use crate::error::EnumConvertResultExt;
use crate::internal::macros::{make_packet_enum, make_struct};
use crate::internal::{SauerRead, SauerWrite, check_tag, constants::*};
use crate::route::{C2S, Chan0, S2C};
use crate::{ParseError, data::*};

make_packet_enum!(
/// Server -> Client (Channel 0)
[S2C<Chan0>] Chan0Server {
    /// `N_DEMOPACKET` Demo playback marker.
    [N_DEMOPACKET] DemoPacket,
    /// `N_POS` Constant physical position updates.
    [N_POS] [RELIABLE false] Position,
    /// `N_TELEPORT` Sent to immediately teleport a player.
    [N_TELEPORT] Teleport,
    /// `N_JUMPPAD` Sent to apply a jump-pad acceleration.
    [N_JUMPPAD] JumpPad,
});

make_packet_enum!(
/// Client -> Server (Channel 0)
[C2S<Chan0>] Chan0Client {
    /// `N_POS` Standard physical position update sent constantly.
    [N_POS] [RELIABLE false] Position,
    /// `N_TELEPORT` Broadcasts the client teleporting.
    [N_TELEPORT] Teleport,
    /// `N_JUMPPAD` Broadcasts the client hitting a jump pad.
    [N_JUMPPAD] JumpPad,
});

/// `N_POS` Constant physical position updates.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Position {
    pub client_num: Cn,
    pub phys_state: PhysState,
    /// Least significant bit of current life sequence (to make sure this packet is not from a previous life)
    pub life_seq_parity: bool,
    pub move_dir: Movement,
    pub strafe: Strafe,
    pub touches_gameclip: bool,
    pub pos: Vec3<f32>,
    pub dir: Direction,
    pub roll: i8,
    pub vel_speed: u16,
    pub vel_dir: Direction,
    pub fall_mag: u16,
    pub fall_dir: Direction,
}

make_struct!(
/// `N_TELEPORT` Sent to immediately teleport a player.
[N_TELEPORT] Teleport {
    client_num: Cn,
    tp: i32,
    td: i32,
});

make_struct!(
/// `N_JUMPPAD` Sent to apply a jump-pad acceleration.
[N_JUMPPAD] JumpPad {
    client_num: Cn,
    jp: i32,
});

make_struct!(
/// `N_DEMOPACKET` Demo playback marker (server only).
[N_DEMOPACKET] DemoPacket {}
);

impl SauerWrite for Position {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_POS.sauer_write(out);

        // The cn goes out unsigned here, unlike everywhere else.
        (self.client_num.0 as u32).sauer_write(out);

        // Pack phys_state, life sequence, and movement into 1 byte
        let phys_state_val = (self.phys_state as u8) & 0x07;
        let life_seq_val = if self.life_seq_parity { 1 } else { 0 };
        let move_val = (self.move_dir as i8 as u8) & 0x03;
        let strafe_val = (self.strafe as i8 as u8) & 0x03;

        let phys_byte = phys_state_val | (life_seq_val << 3) | (move_val << 4) | (strafe_val << 6);
        phys_byte.sauer_write(out);

        // Compute flags
        let mut flags: u32 = 0;
        let o_x = (self.pos.x * DMF as f32) as i32;
        let o_y = (self.pos.y * DMF as f32) as i32;
        let o_z = (self.pos.z * DMF as f32) as i32;

        if o_x < 0 || o_x > 0xFFFF {
            flags |= 1 << 0;
        }
        if o_y < 0 || o_y > 0xFFFF {
            flags |= 1 << 1;
        }
        if o_z < 0 || o_z > 0xFFFF {
            flags |= 1 << 2;
        }

        if self.vel_speed > 0xFF {
            flags |= 1 << 3;
        }

        if self.fall_mag > 0 {
            flags |= 1 << 4;
            if self.fall_mag > 0xFF {
                flags |= 1 << 5;
            }

            // Engine defaults straight down as pitch = -90, yaw = 0
            if self.fall_dir.yaw != 0 || self.fall_dir.pitch != -90 {
                flags |= 1 << 6;
            }
        }

        if self.touches_gameclip {
            flags |= 1 << 7;
        }

        flags.sauer_write(out);

        // Manually serialize positions
        for o in [o_x, o_y, o_z] {
            (o as u8).sauer_write(out);
            ((o >> 8) as u8).sauer_write(out);
            if o < 0 || o > 0xFFFF {
                ((o >> 16) as u8).sauer_write(out);
            }
        }

        self.dir.sauer_write(out);

        // Bias roll by +90 before wire
        let biased_roll = (self.roll as i16 + 90).clamp(0, 180) as u8;
        biased_roll.sauer_write(out);

        // Velocity speed (1 or 2 bytes)
        (self.vel_speed as u8).sauer_write(out);
        if self.vel_speed > 0xFF {
            ((self.vel_speed >> 8) as u8).sauer_write(out);
        }

        self.vel_dir.sauer_write(out);

        // Fall handling
        if self.fall_mag > 0 {
            (self.fall_mag as u8).sauer_write(out);
            if self.fall_mag > 0xFF {
                ((self.fall_mag >> 8) as u8).sauer_write(out);
            }

            if self.fall_dir.yaw != 0 || self.fall_dir.pitch != -90 {
                self.fall_dir.sauer_write(out);
            }
        }
    }
}
impl SauerRead for Position {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_POS)?;

        let client_num = Cn::try_from(u32::sauer_read(reader)? as i32)?;

        let phys_byte = u8::sauer_read(reader)?;

        let phys_state = PhysState::try_from(phys_byte & 0x07).in_enum("PhysState")?;
        let life_seq_parity = (phys_byte & (1 << 3)) != 0;

        // Double shift does automatic Two's Complement sign-extension: (3 -> -1)
        let move_dir =
            Movement::try_from((((phys_byte >> 4) & 0x03) as i8) << 6 >> 6).in_enum("Movement")?;
        let strafe =
            Strafe::try_from((((phys_byte >> 6) & 0x03) as i8) << 6 >> 6).in_enum("Strafe")?;

        let flags = u32::sauer_read(reader)?;
        let touches_gameclip = (flags & (1 << 7)) != 0;

        // Manually deserialize positions
        let mut pos_arr = [0.0; 3];
        for i in 0..3 {
            let b0 = u8::sauer_read(reader)? as i32;
            let b1 = u8::sauer_read(reader)? as i32;
            let mut val = b0 | (b1 << 8);

            if (flags & (1 << i)) != 0 {
                let b2 = u8::sauer_read(reader)? as i32;
                val |= b2 << 16;
                // Sign extend 24-bit -> 32-bit integer
                if (val & 0x80_00_00) != 0 {
                    val |= -16777216; // identical to 0xFF_00_00_00
                }
            }
            pos_arr[i] = (val as f32) / DMF as f32;
        }
        let pos = Vec3 {
            x: pos_arr[0],
            y: pos_arr[1],
            z: pos_arr[2],
        };

        let dir = Direction::sauer_read(reader)?;

        let roll_byte = u8::sauer_read(reader)?;
        let roll = (roll_byte as i16 - 90) as i8; // Unbias roll

        let vel_b0 = u8::sauer_read(reader)?;
        let vel_speed = if (flags & (1 << 3)) != 0 {
            let vel_b1 = u8::sauer_read(reader)?;
            u16::from_le_bytes([vel_b0, vel_b1])
        } else {
            vel_b0 as u16
        };

        let vel_dir = Direction::sauer_read(reader)?;

        let (fall_mag, fall_dir) = if (flags & (1 << 4)) != 0 {
            let fall_b0 = u8::sauer_read(reader)?;
            let mag = if (flags & (1 << 5)) != 0 {
                let fall_b1 = u8::sauer_read(reader)?;
                u16::from_le_bytes([fall_b0, fall_b1])
            } else {
                fall_b0 as u16
            };

            let dir = if (flags & (1 << 6)) != 0 {
                Direction::sauer_read(reader)?
            } else {
                Direction { yaw: 0, pitch: -90 }
            };

            (mag, dir)
        } else {
            (0, Direction { yaw: 0, pitch: -90 })
        };

        Ok(Self {
            client_num,
            phys_state,
            life_seq_parity,
            move_dir,
            strafe,
            touches_gameclip,
            pos,
            dir,
            roll,
            vel_speed,
            vel_dir,
            fall_mag,
            fall_dir,
        })
    }
}

impl crate::PacketDescriptor for Position {
    fn packet_names(&self) -> Vec<&'static str> {
        vec![crate::internal::msg_id_to_name(N_POS)]
    }
}

#[cfg(feature = "lua")]
const _: () = {
    use mlua::{UserData, UserDataFields};

    impl UserData for Position {
        fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
            fields.add_field_method_get("client_num", |_lua, this| Ok(this.client_num));
            fields.add_field_method_get("phys_state", |_lua, this| Ok(this.phys_state));
            fields.add_field_method_get("life_seq_parity", |_lua, this| Ok(this.life_seq_parity));
            fields.add_field_method_get("move_dir", |_lua, this| Ok(this.move_dir));
            fields.add_field_method_get("strafe", |_lua, this| Ok(this.strafe));
            fields.add_field_method_get("touches_gameclip", |_lua, this| Ok(this.touches_gameclip));
            fields.add_field_method_get("pos", |_lua, this| Ok(this.pos));
            fields.add_field_method_get("dir", |_lua, this| Ok(this.dir));
            fields.add_field_method_get("roll", |_lua, this| Ok(this.roll));
            fields.add_field_method_get("vel_speed", |_lua, this| Ok(this.vel_speed));
            fields.add_field_method_get("vel_dir", |_lua, this| Ok(this.vel_dir));
            fields.add_field_method_get("fall_mag", |_lua, this| Ok(this.fall_mag));
            fields.add_field_method_get("fall_dir", |_lua, this| Ok(this.fall_dir));
        }
    }
};
