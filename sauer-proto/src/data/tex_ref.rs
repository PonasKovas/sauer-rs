use crate::ParseError;
use crate::internal::primitives::{read_extra_block, write_extra_block};
use crate::internal::{SauerRead, SauerWrite};
use sauer_core::VSlotProps;

/// Wire values at or above this name a texture slot plus a packed vslot.
const PACKED_TEX: i32 = 0x10000;

/// A texture as `N_EDITT` and `N_REPLACE` send it.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TexRef {
    /// A vslot index in the sender's own numbering, sent when the vslot has
    /// no changes over its texture slot.
    Index(u16),
    /// Texture `slot` with changes on top. Sent this way because vslot
    /// indices of changed vslots differ between clients.
    Variant { slot: u16, vslot: VSlotProps },
}

impl TexRef {
    fn wire_index(&self) -> i32 {
        match self {
            TexRef::Index(i) => *i as i32,
            TexRef::Variant { slot, .. } => PACKED_TEX + *slot as i32,
        }
    }
}

/// Writes the texture indices themselves; the vslots go in [`write_extra`].
pub(crate) fn write_index(tex: &TexRef, out: &mut Vec<u8>) {
    tex.wire_index().sauer_write(out);
}

/// Reads a texture index. A [`TexRef::Variant`] comes back with an empty
/// vslot that [`read_extra`] fills in.
pub(crate) fn read_index(reader: &mut &[u8]) -> Result<TexRef, ParseError> {
    let tex = i32::sauer_read(reader)?;
    match tex {
        PACKED_TEX.. => Ok(TexRef::Variant {
            slot: tex as u16,
            vslot: VSlotProps::default(),
        }),
        0.. => Ok(TexRef::Index(tex as u16)),
        _ => Err(ParseError::ValueOutOfRange {
            value: tex,
            max: i32::MAX,
        }),
    }
}

/// Writes the extra block holding the vslot of every [`TexRef::Variant`] in
/// `texs`, in order.
pub(crate) fn write_extra(texs: &[&TexRef], out: &mut Vec<u8>) {
    write_extra_block(out, |out| {
        for tex in texs {
            if let TexRef::Variant { vslot, .. } = tex {
                vslot.sauer_write(out);
            }
        }
    });
}

/// Reads the block [`write_extra`] writes into the variants of `texs`.
pub(crate) fn read_extra(texs: &mut [&mut TexRef], reader: &mut &[u8]) -> Result<(), ParseError> {
    let mut extra = read_extra_block(reader)?;
    for tex in texs {
        if let TexRef::Variant { vslot, .. } = tex {
            *vslot = VSlotProps::sauer_read(&mut extra)?;
        }
    }
    Ok(())
}

#[cfg(feature = "lua")]
const _: () = {
    use mlua::{FromLua, IntoLua, Lua, Value};

    /// An index is a number, a variant a `{ slot, vslot }` table.
    impl IntoLua for TexRef {
        fn into_lua(self, lua: &Lua) -> mlua::Result<Value> {
            match self {
                TexRef::Index(i) => i.into_lua(lua),
                TexRef::Variant { slot, vslot } => {
                    let table = lua.create_table()?;
                    table.set("slot", slot)?;
                    table.set("vslot", vslot)?;
                    Ok(Value::Table(table))
                }
            }
        }
    }

    impl FromLua for TexRef {
        fn from_lua(value: Value, lua: &Lua) -> mlua::Result<Self> {
            match value {
                Value::Table(table) => Ok(TexRef::Variant {
                    slot: table.get("slot")?,
                    vslot: table.get("vslot")?,
                }),
                other => Ok(TexRef::Index(u16::from_lua(other, lua)?)),
            }
        }
    }
};
