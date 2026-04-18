use crate::internal::{SauerRead, SauerWrite, read_slice};

#[derive(Debug, Clone, PartialEq, Copy)]
pub struct Direction {
    pub yaw: u16,
    pub pitch: i8,
}

impl SauerWrite for Direction {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        let safe_yaw = self.yaw % 360;
        // Bias it by +90 for the wire, and clamp it just in case
        let biased_pitch = (self.pitch as i16 + 90).clamp(0, 180) as u16;

        let packed = safe_yaw + biased_pitch * 360;
        out.extend_from_slice(&packed.to_le_bytes());
    }
}
impl SauerRead for Direction {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, crate::ParseError> {
        let bytes = read_slice(reader, 2)?;
        let packed = u16::from_le_bytes([bytes[0], bytes[1]]);

        let yaw = packed % 360;
        let biased_pitch = packed / 360;

        Ok(Self {
            yaw,
            // Remove the +90 bias from the wire
            pitch: (biased_pitch as i16 - 90) as i8,
        })
    }
}

#[cfg(feature = "lua")]
const _: () = {
    use mlua::{FromLua, UserData, UserDataFields};

    impl UserData for Direction {
        fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
            fields.add_field_method_get("yaw", |_lua, this| Ok(this.yaw));
            fields.add_field_method_get("pitch", |_lua, this| Ok(this.pitch));
        }
    }

    impl FromLua for Direction {
        fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self> {
            let table = mlua::Table::from_lua(value, lua)?;
            Ok(Self {
                yaw: table.get("yaw")?,
                pitch: table.get("pitch")?,
            })
        }
    }
};
