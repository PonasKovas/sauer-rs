use nom::{
    IResult,
    bytes::complete::take,
    combinator::verify,
    multi::count,
    number::complete::{le_f32, le_i16, le_i32, le_u16, le_u32, le_u8},
};

use crate::types::*;

// ─── safety limits ────────────────────────────────────────────────────────────

const MAX_VSLOTS: i32 = 65_536;
const MAX_ENTS: i32 = 10_000;
const MAX_LIGHTMAPS: i32 = 8_192;
const MAX_PVS_BLOCKS: i32 = 65_536;
const MAX_PVS_BUFFER: u32 = 64 * 1024 * 1024; // 64 MB
const MAX_WATER_PLANES: u32 = 1_024;
const MAX_OCTREE_DEPTH: u32 = 20; // world size 2^20 = 1 M
const MAX_BLENDMAP_DEPTH: u32 = 20;
const MAX_SHADER_PARAMS: u16 = 1_024;
const MAX_NAME_LEN: u16 = 4_096;
const MAX_EXTRA_GAME_DATA: u16 = 65_535;
const MAX_MRU_COUNT: u16 = 65_535;
const MAX_VIEWCELLS_DEPTH: u32 = 32;

// ─── primitives ───────────────────────────────────────────────────────────────

fn parse_vec3(i: &[u8]) -> IResult<&[u8], Vec3> {
    let (i, x) = le_f32(i)?;
    let (i, y) = le_f32(i)?;
    let (i, z) = le_f32(i)?;
    Ok((i, Vec3 { x, y, z }))
}

/// Read a length-prefixed string where the length is a `u16`.
fn parse_string_u16(i: &[u8]) -> IResult<&[u8], String> {
    let (i, len) = verify(le_u16, |&l| l <= MAX_NAME_LEN)(i)?;
    let (i, bytes) = take(len as usize)(i)?;
    let s = String::from_utf8_lossy(bytes).into_owned();
    Ok((i, s))
}

// ─── header ───────────────────────────────────────────────────────────────────

pub(crate) fn parse_header(i: &[u8]) -> IResult<&[u8], Header> {
    let (i, magic) = take(4usize)(i)?;
    if magic != b"OCTA" {
        return Err(nom::Err::Failure(nom::error::Error::new(
            i,
            nom::error::ErrorKind::Tag,
        )));
    }
    let (i, version) = le_i32(i)?;
    let (i, header_size) = le_i32(i)?;
    let (i, world_size) = le_i32(i)?;
    let (i, num_ents) = le_i32(i)?;
    let (i, num_pvs) = le_i32(i)?;
    let (i, num_lightmaps) = le_i32(i)?;
    let (i, blendmap) = le_i32(i)?;
    let (i, num_vars) = le_i32(i)?;
    let (i, num_vslots) = le_i32(i)?;
    Ok((
        i,
        Header {
            version,
            header_size,
            world_size,
            num_ents,
            num_pvs,
            num_lightmaps,
            blendmap,
            num_vars,
            num_vslots,
        },
    ))
}

// ─── map variables ────────────────────────────────────────────────────────────

fn parse_map_variable(i: &[u8]) -> IResult<&[u8], MapVariable> {
    let (i, var_type) = le_u8(i)?;
    let (i, name) = parse_string_u16(i)?;
    let (i, value) = match var_type {
        0 => {
            let (i, v) = le_i32(i)?;
            (i, MapVariableValue::Int(v))
        }
        1 => {
            let (i, v) = le_f32(i)?;
            (i, MapVariableValue::Float(v))
        }
        2 => {
            let (i, s) = parse_string_u16(i)?;
            (i, MapVariableValue::String(s))
        }
        _ => {
            return Err(nom::Err::Failure(nom::error::Error::new(
                i,
                nom::error::ErrorKind::Switch,
            )))
        }
    };
    Ok((i, MapVariable { name, value }))
}

pub(crate) fn parse_map_variables(i: &[u8], num: i32) -> IResult<&[u8], Vec<MapVariable>> {
    let mut input = i;
    let mut vars = Vec::with_capacity(num as usize);
    for _ in 0..num {
        let (rest, var) = parse_map_variable(input)?;
        vars.push(var);
        input = rest;
    }
    Ok((input, vars))
}

// ─── game type / extras ───────────────────────────────────────────────────────

pub(crate) fn parse_game_type_data(i: &[u8]) -> IResult<&[u8], GameTypeData> {
    let (i, gt_len) = le_u8(i)?;
    // +1 for the null terminator that is included in the file
    let (i, gt_bytes) = take(gt_len as usize + 1)(i)?;
    let game_type = String::from_utf8_lossy(
        gt_bytes
            .split(|&b| b == 0)
            .next()
            .unwrap_or(&[][..]),
    )
    .into_owned();

    let (i, extra_entity_info_size) = le_u16(i)?;
    let (i, extra_game_data_size) = verify(le_u16, |&l| l <= MAX_EXTRA_GAME_DATA)(i)?;
    let (i, extra_game_data_bytes) = take(extra_game_data_size as usize)(i)?;

    let (i, mru_count) = verify(le_u16, |&l| l <= MAX_MRU_COUNT)(i)?;
    let (i, texture_mru) = count(le_u16, mru_count as usize)(i)?;

    Ok((
        i,
        GameTypeData {
            game_type,
            extra_entity_info_size,
            extra_game_data: extra_game_data_bytes.to_vec(),
            texture_mru,
        },
    ))
}

// ─── entities ─────────────────────────────────────────────────────────────────

fn parse_entity(i: &[u8], extra_size: u16) -> IResult<&[u8], Entity> {
    let (i, position) = parse_vec3(i)?;
    let (i, a1) = le_i16(i)?;
    let (i, a2) = le_i16(i)?;
    let (i, a3) = le_i16(i)?;
    let (i, a4) = le_i16(i)?;
    let (i, a5) = le_i16(i)?;
    let (i, entity_type) = le_u8(i)?;
    let (i, _reserved) = le_u8(i)?;
    let (i, extra) = take(extra_size as usize)(i)?;
    Ok((
        i,
        Entity {
            position,
            attrs: [a1, a2, a3, a4, a5],
            entity_type,
            extra_payload: extra.to_vec(),
        },
    ))
}

pub(crate) fn parse_entities(
    i: &[u8],
    num: i32,
    extra_size: u16,
) -> IResult<&[u8], Vec<Entity>> {
    let actual = num.min(MAX_ENTS);
    let mut input = i;
    let mut ents = Vec::with_capacity(actual as usize);
    for _ in 0..num {
        let (rest, ent) = parse_entity(input, extra_size)?;
        if (ents.len() as i32) < actual {
            ents.push(ent);
        }
        input = rest;
    }
    Ok((input, ents))
}

// ─── vslots ───────────────────────────────────────────────────────────────────

fn parse_shader_param(i: &[u8]) -> IResult<&[u8], ShaderParam> {
    let (i, name) = parse_string_u16(i)?;
    let (i, v0) = le_f32(i)?;
    let (i, v1) = le_f32(i)?;
    let (i, v2) = le_f32(i)?;
    let (i, v3) = le_f32(i)?;
    Ok((i, ShaderParam { name, val: [v0, v1, v2, v3] }))
}

fn parse_vslot_entry(i: &[u8]) -> IResult<&[u8], (i32, VSlot)> {
    let (i, changed) = le_i32(i)?;
    if changed < 0 {
        // Represents abs(changed) empty default slots; no extra data to read.
        let empty = VSlot {
            prev_index: None,
            changed,
            shader_params: vec![],
            scale: None,
            rotation: None,
            offset: None,
            scroll: None,
            layer: None,
            alpha: None,
            color: None,
        };
        return Ok((i, (changed, empty)));
    }

    let (i, prev_index) = le_i32(i)?;
    let mut vs = VSlot {
        prev_index: Some(prev_index),
        changed,
        shader_params: vec![],
        scale: None,
        rotation: None,
        offset: None,
        scroll: None,
        layer: None,
        alpha: None,
        color: None,
    };

    let mut input = i;

    if changed & (1 << 0) != 0 {
        let (rest, num_params) =
            verify(le_u16, |&n| n <= MAX_SHADER_PARAMS)(input)?;
        let mut params = Vec::with_capacity(num_params as usize);
        let mut pi = rest;
        for _ in 0..num_params {
            let (pr, p) = parse_shader_param(pi)?;
            params.push(p);
            pi = pr;
        }
        vs.shader_params = params;
        input = pi;
    }
    if changed & (1 << 1) != 0 {
        let (rest, v) = le_f32(input)?;
        vs.scale = Some(v);
        input = rest;
    }
    if changed & (1 << 2) != 0 {
        let (rest, v) = le_i32(input)?;
        vs.rotation = Some(v);
        input = rest;
    }
    if changed & (1 << 3) != 0 {
        let (rest, x) = le_i32(input)?;
        let (rest, y) = le_i32(rest)?;
        vs.offset = Some([x, y]);
        input = rest;
    }
    if changed & (1 << 4) != 0 {
        let (rest, x) = le_f32(input)?;
        let (rest, y) = le_f32(rest)?;
        vs.scroll = Some([x, y]);
        input = rest;
    }
    if changed & (1 << 5) != 0 {
        let (rest, v) = le_i32(input)?;
        vs.layer = Some(v);
        input = rest;
    }
    if changed & (1 << 6) != 0 {
        let (rest, front) = le_f32(input)?;
        let (rest, back) = le_f32(rest)?;
        vs.alpha = Some([front, back]);
        input = rest;
    }
    if changed & (1 << 7) != 0 {
        let (rest, r) = le_f32(input)?;
        let (rest, g) = le_f32(rest)?;
        let (rest, b) = le_f32(rest)?;
        vs.color = Some([r, g, b]);
        input = rest;
    }

    Ok((input, (1, vs)))
}

pub(crate) fn parse_vslots(i: &[u8], mut num_vslots: i32) -> IResult<&[u8], Vec<VSlot>> {
    let mut input = i;
    let mut vslots = Vec::new();
    while num_vslots > 0 {
        let (rest, (slot_count, vs)) = parse_vslot_entry(input)?;
        if vs.changed < 0 {
            let empty_count = (-vs.changed).min(num_vslots);
            for _ in 0..empty_count {
                vslots.push(VSlot {
                    prev_index: None,
                    changed: 0,
                    shader_params: vec![],
                    scale: None,
                    rotation: None,
                    offset: None,
                    scroll: None,
                    layer: None,
                    alpha: None,
                    color: None,
                });
            }
            num_vslots -= -vs.changed;
        } else {
            let _ = slot_count;
            vslots.push(vs);
            num_vslots -= 1;
        }
        input = rest;

        if vslots.len() as i32 > MAX_VSLOTS {
            return Err(nom::Err::Failure(nom::error::Error::new(
                input,
                nom::error::ErrorKind::TooLarge,
            )));
        }
    }
    Ok((input, vslots))
}

// ─── octree ───────────────────────────────────────────────────────────────────

fn parse_textures(i: &[u8]) -> IResult<&[u8], [u16; 6]> {
    let (i, t0) = le_u16(i)?;
    let (i, t1) = le_u16(i)?;
    let (i, t2) = le_u16(i)?;
    let (i, t3) = le_u16(i)?;
    let (i, t4) = le_u16(i)?;
    let (i, t5) = le_u16(i)?;
    Ok((i, [t0, t1, t2, t3, t4, t5]))
}

fn parse_surface_verts(
    i: &[u8],
    verts_mask: u8,
    num_verts_raw: u8,
) -> IResult<&[u8], SurfaceVerts> {
    let layer_verts = (num_verts_raw & 0x0F) as usize;
    let layer_dup = (num_verts_raw & 0x80) != 0;
    let has_xyz = (verts_mask & 0x04) != 0;
    let has_uv = (verts_mask & 0x40) != 0;
    let has_norm = (verts_mask & 0x80) != 0;

    let mut input = i;
    let mut xyz_bound = None;
    let mut uv_plane = None;
    let mut uv_plane_dup = None;
    let mut base_norm = None;

    if layer_verts == 4 {
        if has_xyz && (verts_mask & 0x01) != 0 {
            let (rest, a) = le_u16(input)?;
            let (rest, b) = le_u16(rest)?;
            let (rest, c) = le_u16(rest)?;
            let (rest, d) = le_u16(rest)?;
            xyz_bound = Some([a, b, c, d]);
            input = rest;
        }
        if has_uv && (verts_mask & 0x02) != 0 {
            let (rest, a) = le_u16(input)?;
            let (rest, b) = le_u16(rest)?;
            let (rest, c) = le_u16(rest)?;
            let (rest, d) = le_u16(rest)?;
            uv_plane = Some([a, b, c, d]);
            input = rest;
            if layer_dup {
                let (rest, a) = le_u16(input)?;
                let (rest, b) = le_u16(rest)?;
                let (rest, c) = le_u16(rest)?;
                let (rest, d) = le_u16(rest)?;
                uv_plane_dup = Some([a, b, c, d]);
                input = rest;
            }
        }
    }
    if has_norm && (verts_mask & 0x08) != 0 {
        let (rest, n) = le_u16(input)?;
        base_norm = Some(n);
        input = rest;
    }

    // per-vertex detail
    let mut verts = Vec::with_capacity(layer_verts);
    for _ in 0..layer_verts {
        let mut xy = None;
        let mut uv = None;
        let mut norm = None;
        if has_xyz {
            let (rest, x) = le_u16(input)?;
            let (rest, y) = le_u16(rest)?;
            xy = Some([x, y]);
            input = rest;
        }
        if has_uv {
            let (rest, u) = le_u16(input)?;
            let (rest, v) = le_u16(rest)?;
            uv = Some([u, v]);
            input = rest;
        }
        if has_norm {
            let (rest, n) = le_u16(input)?;
            norm = Some(n);
            input = rest;
        }
        verts.push(SurfaceVert { xy, uv, norm });
    }

    // layer dup UV loop
    let mut verts_dup_uv = Vec::new();
    if layer_dup {
        for _ in 0..layer_verts {
            if has_uv {
                let (rest, u) = le_u16(input)?;
                let (rest, v) = le_u16(rest)?;
                verts_dup_uv.push([u, v]);
                input = rest;
            }
        }
    }

    Ok((
        input,
        SurfaceVerts {
            xyz_bound,
            uv_plane,
            uv_plane_dup,
            base_norm,
            verts,
            verts_dup_uv,
        },
    ))
}

fn parse_surfaces(i: &[u8]) -> IResult<&[u8], NodeSurfaces> {
    let (i, surf_mask) = le_u8(i)?;
    let (i, _total_verts) = le_u8(i)?; // consumed but not stored

    let mut input = i;
    let mut surfaces: [Option<SurfaceInfo>; 6] = [None, None, None, None, None, None];

    for face in 0..6usize {
        if (surf_mask & (1 << face)) != 0 {
            let (rest, lm0) = le_u8(input)?;
            let (rest, lm1) = le_u8(rest)?;
            let (rest, verts_mask) = le_u8(rest)?;
            let (rest, num_verts_raw) = le_u8(rest)?;
            let (rest, sv) = parse_surface_verts(rest, verts_mask, num_verts_raw)?;
            surfaces[face] = Some(SurfaceInfo {
                lmid: [lm0, lm1],
                verts_mask,
                num_verts_raw,
                verts: sv,
            });
            input = rest;
        }
    }

    Ok((input, NodeSurfaces { surf_mask, surfaces }))
}

fn parse_octree_node<'a>(i: &'a [u8], depth: u32) -> IResult<&'a [u8], OctreeNode> {
    if depth > MAX_OCTREE_DEPTH {
        tracing::error!("Octree depth limit exceeded");
        return Err(nom::Err::Failure(nom::error::Error::new(
            i,
            nom::error::ErrorKind::TooLarge,
        )));
    }

    let (i, octsav) = le_u8(i)?;
    let node_type = octsav & 0x07;
    let has_material = (octsav & 0x40) != 0;
    let has_merged = (octsav & 0x80) != 0;
    let has_surfaces = (octsav & 0x20) != 0;

    tracing::trace!(depth, node_type, "octree node");

    match node_type {
        // OCTSAV_CHILDREN
        0 => {
            let (i, children) = parse_octree_children(i, depth)?;
            let (i, material) = parse_opt_u16(i, has_material)?;
            let (i, merged) = parse_opt_u8(i, has_merged)?;
            Ok((i, OctreeNode::Children { children, material, merged }))
        }
        // OCTSAV_EMPTY
        1 => {
            let (i, textures) = parse_textures(i)?;
            let (i, material) = parse_opt_u16(i, has_material)?;
            let (i, merged) = parse_opt_u8(i, has_merged)?;
            let (i, surfaces) = parse_opt_surfaces(i, has_surfaces)?;
            Ok((i, OctreeNode::Empty { textures, material, merged, surfaces }))
        }
        // OCTSAV_SOLID
        2 => {
            let (i, textures) = parse_textures(i)?;
            let (i, material) = parse_opt_u16(i, has_material)?;
            let (i, merged) = parse_opt_u8(i, has_merged)?;
            let (i, surfaces) = parse_opt_surfaces(i, has_surfaces)?;
            Ok((i, OctreeNode::Solid { textures, material, merged, surfaces }))
        }
        // OCTSAV_NORMAL
        3 => {
            let (i, textures) = parse_textures(i)?;
            let (i, edge_bytes) = take(12usize)(i)?;
            let mut edges = [0u8; 12];
            edges.copy_from_slice(edge_bytes);
            let (i, material) = parse_opt_u16(i, has_material)?;
            let (i, merged) = parse_opt_u8(i, has_merged)?;
            let (i, surfaces) = parse_opt_surfaces(i, has_surfaces)?;
            Ok((i, OctreeNode::Normal { textures, edges, material, merged, surfaces }))
        }
        // OCTSAV_LODCUBE
        4 => {
            let (i, children) = parse_octree_children(i, depth)?;
            let (i, material) = parse_opt_u16(i, has_material)?;
            let (i, merged) = parse_opt_u8(i, has_merged)?;
            Ok((i, OctreeNode::LodCube { children, material, merged }))
        }
        _ => Err(nom::Err::Failure(nom::error::Error::new(
            i,
            nom::error::ErrorKind::Switch,
        ))),
    }
}

fn parse_octree_children(i: &[u8], depth: u32) -> IResult<&[u8], Box<[OctreeNode; 8]>> {
    let mut input = i;
    let mut nodes: Vec<OctreeNode> = Vec::with_capacity(8);
    for _ in 0..8 {
        let (rest, node) = parse_octree_node(input, depth + 1)?;
        nodes.push(node);
        input = rest;
    }
    let arr: [OctreeNode; 8] = nodes.try_into().unwrap();
    Ok((input, Box::new(arr)))
}

fn parse_opt_u16(i: &[u8], flag: bool) -> IResult<&[u8], Option<u16>> {
    if flag {
        let (i, v) = le_u16(i)?;
        Ok((i, Some(v)))
    } else {
        Ok((i, None))
    }
}

fn parse_opt_u8(i: &[u8], flag: bool) -> IResult<&[u8], Option<u8>> {
    if flag {
        let (i, v) = le_u8(i)?;
        Ok((i, Some(v)))
    } else {
        Ok((i, None))
    }
}

fn parse_opt_surfaces(i: &[u8], flag: bool) -> IResult<&[u8], Option<NodeSurfaces>> {
    if flag {
        let (i, s) = parse_surfaces(i)?;
        Ok((i, Some(s)))
    } else {
        Ok((i, None))
    }
}

pub(crate) fn parse_octree(i: &[u8]) -> IResult<&[u8], OctreeNode> {
    let _span = tracing::debug_span!("octree").entered();
    parse_octree_node(i, 0)
}

// ─── lightmaps ────────────────────────────────────────────────────────────────

fn parse_lightmap(i: &[u8]) -> IResult<&[u8], Lightmap> {
    let (i, raw) = le_u8(i)?;
    let lm_type = raw & 0x7F;
    let has_unlit = (raw & 0x80) != 0;

    let (i, unlit_coords) = if has_unlit {
        let (i, ux) = le_u16(i)?;
        let (i, uy) = le_u16(i)?;
        (i, Some([ux, uy]))
    } else {
        (i, None)
    };

    // BPP: alpha diffuse (type & 0x10 && (type & 0x0F) != 2) → 4, else 3
    let bpp = if (lm_type & 0x10) != 0 && (lm_type & 0x0F) != 2 {
        4usize
    } else {
        3usize
    };
    let data_size = bpp * 512 * 512;
    let (i, data) = take(data_size)(i)?;

    Ok((
        i,
        Lightmap {
            lm_type,
            unlit_coords,
            data: data.to_vec(),
        },
    ))
}

pub(crate) fn parse_lightmaps(i: &[u8], num: i32) -> IResult<&[u8], Vec<Lightmap>> {
    let clamped = num.min(MAX_LIGHTMAPS);
    let mut input = i;
    let mut lms = Vec::with_capacity(clamped as usize);
    for _ in 0..clamped {
        let (rest, lm) = parse_lightmap(input)?;
        lms.push(lm);
        input = rest;
    }
    Ok((input, lms))
}

// ─── PVS ──────────────────────────────────────────────────────────────────────

fn parse_viewcells(i: &[u8], depth: u32) -> IResult<&[u8], ViewcellsNode> {
    if depth > MAX_VIEWCELLS_DEPTH {
        tracing::error!("Viewcells depth limit exceeded");
        return Err(nom::Err::Failure(nom::error::Error::new(
            i,
            nom::error::ErrorKind::TooLarge,
        )));
    }
    tracing::trace!(depth, "viewcells node");

    let (i, leaf_mask) = le_u8(i)?;
    let mut input = i;
    let mut children: Vec<ViewcellsNode> = Vec::with_capacity(8);

    for bit in 0..8u8 {
        if (leaf_mask & (1 << bit)) != 0 {
            let (rest, idx) = le_i32(input)?;
            children.push(ViewcellsNode::Leaf(idx));
            input = rest;
        } else {
            let (rest, child) = parse_viewcells(input, depth + 1)?;
            children.push(child);
            input = rest;
        }
    }

    let arr: [ViewcellsNode; 8] = children.try_into().unwrap();
    Ok((input, ViewcellsNode::Branch(Box::new(arr))))
}

pub(crate) fn parse_pvs(i: &[u8], num_pvs: i32) -> IResult<&[u8], Option<PvsData>> {
    if num_pvs <= 0 {
        return Ok((i, None));
    }
    let _span = tracing::debug_span!("pvs").entered();

    let (i, totallen_raw) = le_u32(i)?;
    let has_water = (totallen_raw & 0x8000_0000) != 0;
    let buffer_length = totallen_raw & 0x7FFF_FFFF;

    if buffer_length > MAX_PVS_BUFFER {
        return Err(nom::Err::Failure(nom::error::Error::new(
            i,
            nom::error::ErrorKind::TooLarge,
        )));
    }

    let (i, water_heights) = if has_water {
        let (i, num_wp) =
            verify(le_u32, |&n| n <= MAX_WATER_PLANES)(i)?;
        let mut wh = Vec::with_capacity(num_wp as usize);
        let mut wi = i;
        for _ in 0..num_wp {
            let (rest, h) = le_i32(wi)?;
            wh.push(h);
            wi = rest;
        }
        (wi, wh)
    } else {
        (i, vec![])
    };

    let clamped_pvs = num_pvs.min(MAX_PVS_BLOCKS) as usize;
    let (i, block_lengths) = count(le_u16, clamped_pvs)(i)?;
    let (i, buffer_bytes) = take(buffer_length as usize)(i)?;

    let (i, viewcells) = parse_viewcells(i, 0)?;

    Ok((
        i,
        Some(PvsData {
            water_heights,
            block_lengths,
            buffer: buffer_bytes.to_vec(),
            viewcells,
        }),
    ))
}

// ─── blendmap ─────────────────────────────────────────────────────────────────

fn parse_blendmap_node(i: &[u8], depth: u32) -> IResult<&[u8], BlendmapNode> {
    if depth > MAX_BLENDMAP_DEPTH {
        tracing::error!("Blendmap depth limit exceeded");
        return Err(nom::Err::Failure(nom::error::Error::new(
            i,
            nom::error::ErrorKind::TooLarge,
        )));
    }
    tracing::trace!(depth, "blendmap node");

    let (i, node_type) = le_u8(i)?;
    match node_type {
        0 => {
            // BRANCH: 4 children
            let mut input = i;
            let mut children: Vec<BlendmapNode> = Vec::with_capacity(4);
            for _ in 0..4 {
                let (rest, child) = parse_blendmap_node(input, depth + 1)?;
                children.push(child);
                input = rest;
            }
            let arr: [BlendmapNode; 4] = children.try_into().unwrap();
            Ok((input, BlendmapNode::Branch(Box::new(arr))))
        }
        1 => {
            // SOLID
            let (i, val) = le_u8(i)?;
            Ok((i, BlendmapNode::Solid(val)))
        }
        2 => {
            // IMAGE: 64×64 = 4096 bytes
            let (i, data) = take(4096usize)(i)?;
            let mut arr = Box::new([0u8; 4096]);
            arr.copy_from_slice(data);
            Ok((i, BlendmapNode::Image(arr)))
        }
        _ => Err(nom::Err::Failure(nom::error::Error::new(
            i,
            nom::error::ErrorKind::Switch,
        ))),
    }
}

pub(crate) fn parse_blendmap(i: &[u8], has_blendmap: i32) -> IResult<&[u8], Option<BlendmapNode>> {
    if has_blendmap <= 0 {
        return Ok((i, None));
    }
    let _span = tracing::debug_span!("blendmap").entered();
    let (i, node) = parse_blendmap_node(i, 0)?;
    Ok((i, Some(node)))
}
