//! Keeping a server-side copy of a coop-edited map: each edit packet applied
//! the way a client applies it (the engine's `mpedittex`, `mpeditface`...).
//!
//! Selections are checked against the map first, so a bad packet can't
//! reach the octree; edits whose selection is invalid change nothing, like
//! on clients. What may be edited is up to the caller: compare an edit's
//! bounds against [`Protected`] before applying it.

use ogz_rs::data::{Entity, Variable};
use sauer_core::Aabb;
use sauer_map::map::MAX_ENTS;
use sauer_map::{Map, Node, Selection};
use sauer_proto::channel1;
use sauer_proto::data::{ClipboardPayload, EditVarValue, UndoPayload};

/// Boxes no client edit may touch.
#[derive(Clone, Debug, Default)]
pub struct Protected(Vec<Aabb>);

impl Protected {
    pub fn protect(&mut self, b: Aabb) {
        self.0.push(b);
    }

    /// Lifts the protection of exactly this box.
    pub fn unprotect(&mut self, b: Aabb) {
        self.0.retain(|&p| p != b);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether an edit of `bounds` touches a protected box.
    pub fn blocks(&self, bounds: &Aabb) -> bool {
        self.0.iter().any(|p| p.overlaps(bounds))
    }
}

/// The box an entity at `pos` takes, for checking entity edits.
pub fn entity_box(pos: [f32; 3]) -> Aabb {
    Aabb::cube(pos.map(|v| v as i32), 1)
}

/// Modifies a global map variable.
pub fn edit_var(map: &mut Map, packet: &channel1::EditVar) {
    let name_ok = packet
        .name
        .as_str()
        .is_some_and(|n| sauer_map::map_vars::accepts(n, &packet.val));
    if !name_ok {
        return;
    }
    let ogz_var = match &packet.val {
        EditVarValue::Int(v) => Variable::Int {
            name: packet.name.clone(),
            value: *v,
        },
        EditVarValue::Float(v) => Variable::Float {
            name: packet.name.clone(),
            value: *v,
        },
        EditVarValue::String(v) => Variable::String {
            name: packet.name.clone(),
            value: v.clone(),
        },
    };

    if let Some(existing) = map.vars.iter_mut().find(|v| match v {
        Variable::Int { name: n, .. }
        | Variable::Float { name: n, .. }
        | Variable::String { name: n, .. } => n == &packet.name,
    }) {
        *existing = ogz_var;
    } else {
        map.vars.push(ogz_var);
    }
}

/// Applies a texture modification to a geometry selection.
pub fn edit_tex(map: &mut Map, packet: &channel1::EditTex) {
    let Some(sel) = packet.sel.validate(map.size) else {
        return;
    };
    // Like the engine, a full vslot table leaves the faces as they are.
    let Some(tex) = sauer_map::vslot::resolve_tex(&mut map.vslots, &packet.tex, true) else {
        return;
    };
    map.set_texture(&sel, tex, packet.all_faces);
    map.invalidate_cache();
}

/// Applies a material filter modification to a geometry selection.
pub fn edit_mat(map: &mut Map, packet: &channel1::EditMat) {
    let packet_mat = packet.mat as i32;
    let packet_filter = packet.filter as i32;
    let mut filtermat = 0u16;
    let mut filtermask = 0u16;
    let mut filtergeom = 0i32;

    if packet_filter >= 0 {
        filtermat = (packet_filter & 0xFFFF) as u16;
        filtergeom = packet_filter & !0xFFFF;
        filtermask = if (filtermat & 0x1F) != 0 {
            0x1F
        } else if (filtermat & 0xE0) != 0 {
            0xE0
        } else {
            filtermat
        };
    }

    let Some(sel) = packet.sel.validate(map.size) else {
        return;
    };
    let (matid, matmask) = if packet_mat < 0 {
        let matmask = if (filtermat & 0x1C) == 0x0C {
            filtermask & !0xE0u16
        } else if (filtermat & 0x1C) == 0x08 {
            filtermask & !0x0100u16
        } else {
            filtermask
        };
        (0u16, matmask)
    } else {
        let mut matid = (packet_mat & 0xFFFF) as u16;
        let matmask = if (matid & 0x1F) != 0 {
            0
        } else if (matid & 0xE0) != 0 {
            !0xE0u16
        } else {
            !matid
        };
        if (matid & 0x1C) == 0x0C {
            matid |= 0x40;
        }
        if (matid & 0x1C) == 0x08 {
            matid |= 0x0100;
        }
        (matid, matmask)
    };

    map.edit_sel(&sel, |node, _| match node {
        Node::Leaf(cube) => {
            let current = cube.material.to_bits();
            if (current & filtermask) == filtermat {
                let skip = match filtergeom {
                    0x10000 => !cube.is_empty(),
                    0x20000 => cube.is_empty(),
                    0x30000 => !cube.is_entirely_solid(),
                    0x40000 => cube.is_entirely_solid(),
                    _ => false,
                };
                if !skip {
                    let new_bits = if matid != 0 {
                        (current & matmask) | matid
                    } else {
                        0
                    };
                    cube.material = ogz_rs::data::Material::from_bits(new_bits);
                }
            }
        }
        Node::Branch(_) => {}
    });
    map.invalidate_cache();
}

/// Clears geometry (fills with air) in a selection. Like the client's
/// `delcube` (`makeempty`), the material goes back to air too.
pub fn del_cube(map: &mut Map, packet: &channel1::DelCube) {
    let Some(sel) = packet.sel.validate(map.size) else {
        return;
    };
    map.fill(&sel, false);
    map.set_material(&sel, Default::default());
    map.invalidate_cache();
}

/// Replaces all instances of a texture with another, optionally within a
/// selection. Without one it touches the whole map.
pub fn replace(map: &mut Map, packet: &channel1::Replace) {
    let sel = match packet.in_sel {
        true => match packet.sel.validate(map.size) {
            Some(sel) => Some(sel),
            None => return,
        },
        false => None,
    };
    // Like the engine's `mpreplacetex`: the old texture must already exist,
    // the new one is created if needed; on failure nothing changes.
    let Some(old_tex) = sauer_map::vslot::resolve_tex(&mut map.vslots, &packet.old_tex, false)
    else {
        return;
    };
    let Some(new_tex) = sauer_map::vslot::resolve_tex(&mut map.vslots, &packet.new_tex, true)
    else {
        return;
    };
    map.replace_texture(sel.as_ref(), old_tex, new_tex);
    map.invalidate_cache();
}

/// The box a replace can touch: its selection, or the whole map.
pub fn replace_bounds(map: &Map, packet: &channel1::Replace) -> Aabb {
    match packet.in_sel {
        true => packet.sel.bounds(),
        false => Aabb::cube([0; 3], map.size),
    }
}

/// Extrudes, pushes corners of, or fills a face.
pub fn edit_face(map: &mut Map, packet: &channel1::EditFace) {
    let Some(sel) = packet.sel.validate(map.size) else {
        return;
    };
    map.edit_face(&sel, packet.dir.clamp(-1, 1), packet.mode as i32);
    map.invalidate_cache();
}

/// Creates or modifies an entity.
pub fn edit_ent(map: &mut Map, packet: &channel1::EditEnt) {
    let id = packet.ent_id as usize;
    let pos = [packet.pos.x, packet.pos.y, packet.pos.z];
    let attrs = packet.attrs.map(|x| x as i16);
    if id < map.entities.len() {
        let ent = &mut map.entities[id];
        ent.position = pos;
        ent.ent_type = packet.type_;
        ent.attrs = attrs;
    } else if id == map.entities.len() && id < MAX_ENTS {
        map.entities.push(Entity {
            position: pos,
            attrs,
            ent_type: packet.type_,
            extra_data: Vec::new(),
        });
    }
}

/// Applies vslot changes (color, scroll, offset...) to faces.
pub fn edit_vslot(map: &mut Map, packet: &channel1::EditVSlot) {
    if packet.vslot.is_empty() {
        return;
    }
    let Some(sel) = packet.sel.validate(map.size) else {
        return;
    };
    map.apply_vslots(&sel, packet.all_faces, packet.delta, &packet.vslot);
    map.invalidate_cache();
}

/// Where a paste lands: the paste position with the clipboard's size.
fn paste_sel(packet: &channel1::PasteClient, clipboard: &ClipboardPayload) -> Selection {
    let mut sel = packet.sel.clone();
    sel.s = clipboard.sel.s;
    sel.orient = clipboard.sel.orient;
    sel
}

pub fn paste_bounds(packet: &channel1::PasteClient, clipboard: &ClipboardPayload) -> Aabb {
    paste_sel(packet, clipboard).bounds()
}

/// Pastes the sender's clipboard at the paste selection.
pub fn paste(map: &mut Map, packet: &channel1::PasteClient, clipboard: &ClipboardPayload) {
    let Some(sel) = paste_sel(packet, clipboard).validate(map.size) else {
        return;
    };
    // The clipboard uses the sending client's vslot numbering.
    let mut nodes = clipboard.nodes.clone();
    sauer_map::vslot::import(&mut map.vslots, &clipboard.vslots, &mut nodes);
    map.apply_block(&sel, nodes, true);
}

/// Rolls a region or some entities back to an undo/redo snapshot.
pub fn undo_redo(map: &mut Map, payload: &UndoPayload) {
    match payload {
        UndoPayload::Cubes {
            sel, nodes, vslots, ..
        } => {
            let Some(sel) = sel.validate(map.size) else {
                return;
            };
            // Undo blocks use the sending client's vslot numbering.
            let mut nodes = nodes.clone();
            sauer_map::vslot::import(&mut map.vslots, vslots, &mut nodes);
            map.apply_block(&sel, nodes, false);
        }
        UndoPayload::Entities(ents) => {
            for ue in ents {
                let id = ue.index as usize;
                if id >= MAX_ENTS {
                    continue;
                }
                if id >= map.entities.len() {
                    map.entities.resize(
                        id + 1,
                        Entity {
                            position: [0.0; 3],
                            attrs: [0; 5],
                            ent_type: sauer_core::EntityType::Empty,
                            extra_data: Vec::new(),
                        },
                    );
                }
                map.entities[id] = ue.ent.clone();
            }
        }
    }
}

/// The box an `N_EDITF` changes: the selection's face slab, or for an extrude
/// (mode 1) the slab between the old and new face. Saturates, so it's safe on
/// an unchecked selection.
pub fn edit_face_bounds(sel: &Selection, dir: i32, mode: i32) -> Aabb {
    let Aabb { mut min, mut max } = sel.bounds();
    let d = (sel.orient as usize) >> 1;
    let dc = (sel.orient as usize) & 1;
    let step = dir.clamp(-1, 1).saturating_mul(sel.grid);
    let extrude = mode == 1 && sel.cx <= 0 && sel.cy <= 0 && sel.cxs & 1 == 0 && sel.cys & 1 == 0;
    if extrude {
        let (old_face, new_face) = if dc == 1 {
            (max[d], max[d].saturating_sub(step))
        } else {
            (min[d], min[d].saturating_add(step))
        };
        min[d] = old_face.min(new_face);
        max[d] = old_face.max(new_face);
    } else if dc == 1 {
        min[d] = max[d].saturating_sub(sel.grid);
    } else {
        max[d] = min[d].saturating_add(sel.grid);
    }
    Aabb::new(min, max)
}

/// The `N_EDITENT` that puts `ent` in slot `index`.
pub fn edit_ent_packet(index: usize, ent: &Entity) -> channel1::EditEnt {
    channel1::EditEnt {
        ent_id: index as i32,
        pos: sauer_proto::data::Vec3 {
            x: ent.position[0],
            y: ent.position[1],
            z: ent.position[2],
        },
        type_: ent.ent_type,
        attrs: ent.attrs.map(i32::from),
    }
}
