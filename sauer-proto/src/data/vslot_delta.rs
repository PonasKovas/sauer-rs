use crate::ParseError;
use crate::internal::{SauerRead, SauerWrite};
use cube_string::CubeString;
use sauer_core::io::{getfloat, getint, getstring, getuint, putfloat, putint, putstring, putuint};

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VSlotDeltaParam {
    pub name: CubeString,
    pub values: [f32; 4],
}

#[derive(Debug, Clone, Default, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VSlotDelta {
    pub params: Vec<VSlotDeltaParam>,
    pub scale: Option<f32>,
    pub rotation: Option<i32>,
    pub offset: Option<(i32, i32)>,
    pub scroll: Option<(f32, f32)>,
    pub layer: Option<i32>,
    pub alpha: Option<(f32, f32)>,
    pub color: Option<(f32, f32, f32)>,
}

impl VSlotDelta {
    pub fn is_empty(&self) -> bool {
        self.params.is_empty()
            && self.scale.is_none()
            && self.rotation.is_none()
            && self.offset.is_none()
            && self.scroll.is_none()
            && self.layer.is_none()
            && self.alpha.is_none()
            && self.color.is_none()
    }
}

impl SauerWrite for VSlotDelta {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        for p in &self.params {
            out.push(0);
            putstring(out, p.name.as_bytes());
            for v in &p.values {
                putfloat(out, *v);
            }
        }
        if let Some(scale) = self.scale {
            out.push(1);
            putfloat(out, scale);
        }
        if let Some(rotation) = self.rotation {
            out.push(2);
            putint(out, rotation);
        }
        if let Some(offset) = self.offset {
            out.push(3);
            putint(out, offset.0);
            putint(out, offset.1);
        }
        if let Some(scroll) = self.scroll {
            out.push(4);
            putfloat(out, scroll.0);
            putfloat(out, scroll.1);
        }
        if let Some(layer) = self.layer {
            out.push(5);
            putuint(out, layer);
        }
        if let Some(alpha) = self.alpha {
            out.push(6);
            putfloat(out, alpha.0);
            putfloat(out, alpha.1);
        }
        if let Some(color) = self.color {
            out.push(7);
            putfloat(out, color.0);
            putfloat(out, color.1);
            putfloat(out, color.2);
        }
        out.push(0xFF);
    }
}

impl SauerRead for VSlotDelta {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        let mut delta = VSlotDelta::default();
        loop {
            if reader.is_empty() {
                break;
            }
            let type_byte = reader[0];
            *reader = &reader[1..];
            if type_byte >= 0x80 {
                break;
            }
            match type_byte {
                0 => {
                    let name =
                        getstring(reader).ok_or(ParseError::UnexpectedEof { expected: 1 })?;
                    let mut values = [0.0; 4];
                    for i in 0..4 {
                        values[i] =
                            getfloat(reader).ok_or(ParseError::UnexpectedEof { expected: 4 })?;
                    }
                    delta.params.push(VSlotDeltaParam { name, values });
                }
                1 => {
                    delta.scale =
                        Some(getfloat(reader).ok_or(ParseError::UnexpectedEof { expected: 4 })?)
                }
                2 => {
                    delta.rotation =
                        Some(getint(reader).ok_or(ParseError::UnexpectedEof { expected: 1 })?)
                }
                3 => {
                    let x = getint(reader).ok_or(ParseError::UnexpectedEof { expected: 1 })?;
                    let y = getint(reader).ok_or(ParseError::UnexpectedEof { expected: 1 })?;
                    delta.offset = Some((x, y));
                }
                4 => {
                    let s = getfloat(reader).ok_or(ParseError::UnexpectedEof { expected: 4 })?;
                    let t = getfloat(reader).ok_or(ParseError::UnexpectedEof { expected: 4 })?;
                    delta.scroll = Some((s, t));
                }
                5 => {
                    delta.layer =
                        Some(getuint(reader).ok_or(ParseError::UnexpectedEof { expected: 1 })?)
                }
                6 => {
                    let f = getfloat(reader).ok_or(ParseError::UnexpectedEof { expected: 4 })?;
                    let b = getfloat(reader).ok_or(ParseError::UnexpectedEof { expected: 4 })?;
                    delta.alpha = Some((f, b));
                }
                7 => {
                    let r = getfloat(reader).ok_or(ParseError::UnexpectedEof { expected: 4 })?;
                    let g = getfloat(reader).ok_or(ParseError::UnexpectedEof { expected: 4 })?;
                    let b = getfloat(reader).ok_or(ParseError::UnexpectedEof { expected: 4 })?;
                    delta.color = Some((r, g, b));
                }
                v => {
                    return Err(ParseError::ValueOutOfRange {
                        value: v as i32,
                        max: 7,
                    });
                }
            }
        }
        Ok(delta)
    }
}

#[cfg(feature = "lua")]
const _: () = {
    use mlua::{FromLua, UserData, UserDataFields};

    impl UserData for VSlotDeltaParam {
        fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
            fields.add_field_method_get("name", |_lua, this| Ok(this.name.clone()));
            fields.add_field_method_get("values", |_lua, this| Ok(this.values.to_vec()));
        }
    }

    impl FromLua for VSlotDeltaParam {
        fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self> {
            let table = mlua::Table::from_lua(value, lua)?;
            let vals: Vec<f32> = table.get("values")?;
            let mut values = [0.0; 4];
            for (i, &v) in vals.iter().enumerate().take(4) {
                values[i] = v;
            }
            Ok(Self {
                name: table.get("name")?,
                values,
            })
        }
    }

    impl UserData for VSlotDelta {
        fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
            fields.add_field_method_get("params", |_lua, this| Ok(this.params.clone()));
            fields.add_field_method_get("scale", |_lua, this| Ok(this.scale));
            fields.add_field_method_get("rotation", |_lua, this| Ok(this.rotation));
            fields.add_field_method_get("offset", |_lua, this| {
                Ok(this.offset.map(|o| vec![o.0, o.1]))
            });
            fields.add_field_method_get("scroll", |_lua, this| {
                Ok(this.scroll.map(|s| vec![s.0, s.1]))
            });
            fields.add_field_method_get("layer", |_lua, this| Ok(this.layer));
            fields
                .add_field_method_get("alpha", |_lua, this| Ok(this.alpha.map(|a| vec![a.0, a.1])));
            fields.add_field_method_get("color", |_lua, this| {
                Ok(this.color.map(|c| vec![c.0, c.1, c.2]))
            });
        }
    }

    impl FromLua for VSlotDelta {
        fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self> {
            let table = mlua::Table::from_lua(value, lua)?;

            let offset_vals: Option<Vec<i32>> = table.get("offset").ok();
            let offset = offset_vals.and_then(|v| {
                if v.len() >= 2 {
                    Some((v[0], v[1]))
                } else {
                    None
                }
            });

            let scroll_vals: Option<Vec<f32>> = table.get("scroll").ok();
            let scroll = scroll_vals.and_then(|v| {
                if v.len() >= 2 {
                    Some((v[0], v[1]))
                } else {
                    None
                }
            });

            let alpha_vals: Option<Vec<f32>> = table.get("alpha").ok();
            let alpha = alpha_vals.and_then(|v| {
                if v.len() >= 2 {
                    Some((v[0], v[1]))
                } else {
                    None
                }
            });

            let color_vals: Option<Vec<f32>> = table.get("color").ok();
            let color = color_vals.and_then(|v| {
                if v.len() >= 3 {
                    Some((v[0], v[1], v[2]))
                } else {
                    None
                }
            });

            Ok(Self {
                params: table.get("params").unwrap_or_default(),
                scale: table.get("scale").ok(),
                rotation: table.get("rotation").ok(),
                offset,
                scroll,
                layer: table.get("layer").ok(),
                alpha,
                color,
            })
        }
    }
};
