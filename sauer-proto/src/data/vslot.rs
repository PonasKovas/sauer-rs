//! The packed vslot format of `packvslot`/`unpackvslot` in texture.cpp, used
//! by `N_EDITVSLOT`, [`TexRef`](crate::data::TexRef) and edit blocks.

use crate::ParseError;
use crate::internal::{SauerRead, SauerWrite};
use sauer_core::io::{getfloat, getint, getstring, getuint, putfloat, putint, putstring, putuint};
use sauer_core::{VSlotParam, VSlotProps};

const SHPARAM: u8 = 0;
const SCALE: u8 = 1;
const ROTATION: u8 = 2;
const OFFSET: u8 = 3;
const SCROLL: u8 = 4;
const LAYER: u8 = 5;
const ALPHA: u8 = 6;
const COLOR: u8 = 7;
/// Any byte from here on ends a packed vslot; the engine writes 0xFF.
const END: u8 = 0x80;

impl SauerWrite for VSlotProps {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        for p in &self.params {
            out.push(SHPARAM);
            putstring(out, p.name.as_bytes());
            for v in &p.values {
                putfloat(out, *v);
            }
        }
        if let Some(scale) = self.scale {
            out.push(SCALE);
            putfloat(out, scale);
        }
        if let Some(rotation) = self.rotation {
            out.push(ROTATION);
            putint(out, rotation);
        }
        if let Some((x, y)) = self.offset {
            out.push(OFFSET);
            putint(out, x);
            putint(out, y);
        }
        if let Some((s, t)) = self.scroll {
            out.push(SCROLL);
            putfloat(out, s);
            putfloat(out, t);
        }
        if let Some(layer) = self.layer {
            out.push(LAYER);
            putuint(out, layer);
        }
        if let Some((front, back)) = self.alpha {
            out.push(ALPHA);
            putfloat(out, front);
            putfloat(out, back);
        }
        if let Some((r, g, b)) = self.color {
            out.push(COLOR);
            putfloat(out, r);
            putfloat(out, g);
            putfloat(out, b);
        }
        out.push(0xFF);
    }
}

/// Reads up to and including the end marker. Running out of data also ends
/// it, like in the engine.
impl SauerRead for VSlotProps {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        let eof = |expected| ParseError::UnexpectedEof { expected };
        let float = |r: &mut &[u8]| getfloat(r).ok_or(eof(4));
        let int = |r: &mut &[u8]| getint(r).ok_or(eof(1));

        let mut props = VSlotProps::default();
        while let Some((&kind, rest)) = reader.split_first() {
            *reader = rest;
            match kind {
                SHPARAM => {
                    let name = getstring(reader).ok_or(eof(1))?;
                    let mut values = [0.0; 4];
                    for v in &mut values {
                        *v = float(reader)?;
                    }
                    props.params.push(VSlotParam { name, values });
                }
                SCALE => props.scale = Some(float(reader)?),
                ROTATION => props.rotation = Some(int(reader)?),
                OFFSET => props.offset = Some((int(reader)?, int(reader)?)),
                SCROLL => props.scroll = Some((float(reader)?, float(reader)?)),
                LAYER => props.layer = Some(getuint(reader).ok_or(eof(1))?),
                ALPHA => props.alpha = Some((float(reader)?, float(reader)?)),
                COLOR => props.color = Some((float(reader)?, float(reader)?, float(reader)?)),
                END.. => break,
                other => {
                    return Err(ParseError::ValueOutOfRange {
                        value: other as i32,
                        max: COLOR as i32,
                    });
                }
            }
        }
        Ok(props)
    }
}
