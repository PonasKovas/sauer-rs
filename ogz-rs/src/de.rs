use crate::data::*;
use crate::error::OgzError;
use flate2::read::MultiGzDecoder;
use std::io::{Cursor, Read};

// Safety limits — large enough for any legitimate map, small enough to catch garbage.
const MAX_DECOMPRESSED: u64 = 512 * 1024 * 1024; // 512 MB
const MAX_ENTITIES: i32 = 1_000_000;
const MAX_LIGHTMAPS: i32 = 16_384;
const MAX_PVS_BYTES: i32 = 256 * 1024 * 1024; // 256 MB
const MAX_VARS: i32 = 65_536;
const MAX_VSLOTS: i32 = 1_000_000;
const MAX_BLENDMAP_RES: i32 = 8_192; // 8192² = 64 MB

// Helper trait to abstract Little-Endian reads securely
trait ReadExt: Read {
    fn read_u8(&mut self) -> std::io::Result<u8> {
        let mut buf = [0; 1];
        self.read_exact(&mut buf)?;
        Ok(buf[0])
    }
    fn read_u16_le(&mut self) -> std::io::Result<u16> {
        let mut buf = [0; 2];
        self.read_exact(&mut buf)?;
        Ok(u16::from_le_bytes(buf))
    }
    fn read_i16_le(&mut self) -> std::io::Result<i16> {
        let mut buf = [0; 2];
        self.read_exact(&mut buf)?;
        Ok(i16::from_le_bytes(buf))
    }
    fn read_i32_le(&mut self) -> std::io::Result<i32> {
        let mut buf = [0; 4];
        self.read_exact(&mut buf)?;
        Ok(i32::from_le_bytes(buf))
    }
    fn read_f32_le(&mut self) -> std::io::Result<f32> {
        let mut buf = [0; 4];
        self.read_exact(&mut buf)?;
        Ok(f32::from_le_bytes(buf))
    }
}
impl<R: Read + ?Sized> ReadExt for R {}

/// Leniently decompresses an .ogz file, handling concatenated GZIP streams
/// and bypassing Sauerbraten's historically broken GZIP footers.
fn decompress_ogz(data: &[u8]) -> Result<Vec<u8>, OgzError> {
    let decoder = MultiGzDecoder::new(Cursor::new(data));
    let mut decompressed = Vec::new();

    // Read up to MAX_DECOMPRESSED + 1 bytes. If we get more than the limit, reject it.
    // This guards against compression bombs (tiny input → gigabyte output).
    match decoder
        .take(MAX_DECOMPRESSED + 1)
        .read_to_end(&mut decompressed)
    {
        Ok(_) => {}
        Err(e) => {
            // flate2 strictly enforces the GZIP trailer CRC32. Sauerbraten files often have
            // broken trailers but perfectly intact DEFLATE payloads. `read_to_end` successfully
            // appends the intact payload before throwing the trailer error, so if our buffer
            // has data, we can safely ignore the error.
            if decompressed.is_empty() {
                return Err(OgzError::Io(e));
            }
        }
    }
    if decompressed.len() > MAX_DECOMPRESSED as usize {
        return Err(OgzError::DecompressedTooLarge(decompressed.len()));
    }
    Ok(decompressed)
}
pub fn parse_map(data: &[u8]) -> Result<OgzMap, OgzError> {
    let decompressed = decompress_ogz(data)?;
    let mut cursor = Cursor::new(&decompressed);

    let mut magic = [0u8; 4];
    cursor.read_exact(&mut magic)?;
    if &magic != b"OCTA" {
        return Err(OgzError::InvalidMagic);
    }

    let version = cursor.read_i32_le()?;
    if version > 33 {
        return Err(OgzError::UnsupportedVersion(version));
    }

    let _headersize = cursor.read_i32_le()?;
    let world_size = cursor.read_i32_le()?;
    let numents = cursor.read_i32_le()?;
    let numpvs = cursor.read_i32_le()?;
    let lightmaps = cursor.read_i32_le()?;

    if numents > MAX_ENTITIES {
        return Err(OgzError::LimitExceeded("numents"));
    }
    if numpvs > MAX_PVS_BYTES {
        return Err(OgzError::LimitExceeded("numpvs"));
    }
    if lightmaps > MAX_LIGHTMAPS {
        return Err(OgzError::LimitExceeded("lightmaps"));
    }

    let mut blendmap_res;
    let map_vars;
    let mut map_vslots = 0;

    let config = if version <= 28 {
        let lightprecision = cursor.read_i32_le()?;
        let lighterror = cursor.read_i32_le()?;
        let lightlod = cursor.read_i32_le()?;
        let ambient = cursor.read_u8()?;
        let mut watercolour = [0; 3];
        cursor.read_exact(&mut watercolour)?;
        let blendmap_mode = cursor.read_u8()?;

        blendmap_res = blendmap_mode as i32;

        let lerpangle = cursor.read_u8()?;
        let lerpsubdiv = cursor.read_u8()?;
        let lerpsubdivsize = cursor.read_u8()?;
        let bumperror = cursor.read_u8()?;
        let mut skylight = [0; 3];
        cursor.read_exact(&mut skylight)?;
        let mut lavacolour = [0; 3];
        cursor.read_exact(&mut lavacolour)?;
        let mut waterfallcolour = [0; 3];
        cursor.read_exact(&mut waterfallcolour)?;

        cursor.read_exact(&mut [0u8; 10])?; // Reserved

        let mut maptitle_buf = [0u8; 128];
        cursor.read_exact(&mut maptitle_buf)?;
        let title_len = maptitle_buf.iter().position(|&c| c == 0).unwrap_or(128);

        MapConfig::Legacy {
            lightprecision,
            lighterror,
            lightlod,
            ambient,
            watercolour,
            blendmap_mode,
            lerpangle,
            lerpsubdiv,
            lerpsubdivsize,
            bumperror,
            skylight,
            lavacolour,
            waterfallcolour,
            maptitle: CubeString::from_bytes(&maptitle_buf[..title_len]),
        }
    } else {
        blendmap_res = cursor.read_i32_le()?;
        map_vars = cursor.read_i32_le()?;
        if version >= 30 {
            map_vslots = cursor.read_i32_le()?;
        }

        if map_vars > MAX_VARS {
            return Err(OgzError::LimitExceeded("numvars"));
        }
        if map_vslots > MAX_VSLOTS {
            return Err(OgzError::LimitExceeded("numvslots"));
        }

        let mut variables = Vec::new();
        for _ in 0..map_vars {
            let v_type = cursor.read_u8()?;
            let namelen = cursor.read_u16_le()?;
            let mut name_buf = vec![0u8; namelen as usize];
            cursor.read_exact(&mut name_buf)?;
            let name = CubeString(name_buf);

            let var = match v_type {
                0 => Variable::Int {
                    name,
                    value: cursor.read_i32_le()?,
                },
                1 => Variable::Float {
                    name,
                    value: cursor.read_f32_le()?,
                },
                2 => {
                    let slen = cursor.read_u16_le()?;
                    let mut sbuf = vec![0u8; slen as usize];
                    cursor.read_exact(&mut sbuf)?;
                    Variable::String {
                        name,
                        value: CubeString(sbuf),
                    }
                }
                _ => Variable::Int { name, value: 0 },
            };
            variables.push(var);
        }
        MapConfig::Modern { variables }
    };

    let mut game_type = None;
    let mut eif = 0;
    let mut extra_game_data = Vec::new();
    if version >= 16 {
        let g_len = cursor.read_u8()?;
        let mut g_buf = vec![0u8; g_len as usize + 1];
        cursor.read_exact(&mut g_buf)?;
        g_buf.pop(); // Remove null terminator
        game_type = Some(CubeString(g_buf));

        eif = cursor.read_u16_le()?;
        let extrasize = cursor.read_u16_le()?;
        extra_game_data = vec![0u8; extrasize as usize];
        cursor.read_exact(&mut extra_game_data)?;
    }

    let mut mru_textures = Vec::new();
    if version < 14 {
        for _ in 0..256 {
            mru_textures.push(cursor.read_u8()? as u16);
        }
    } else {
        let nummru = cursor.read_u16_le()?;
        for _ in 0..nummru {
            mru_textures.push(cursor.read_u16_le()?);
        }
    }

    let mut entities = Vec::new();
    for _ in 0..numents {
        let mut position = [0f32; 3];
        for p in &mut position {
            *p = cursor.read_f32_le()?;
        }

        let mut attrs = [0i16; 5];
        for a in &mut attrs {
            *a = cursor.read_i16_le()?;
        }

        let mut ent_type = cursor.read_u8()?;
        let _reserved = cursor.read_u8()?;

        let mut extra_data = vec![0u8; eif as usize];
        if eif > 0 {
            cursor.read_exact(&mut extra_data)?;
        }

        // Apply historically required map fixes directly on load
        if version <= 10 && ent_type >= 7 {
            ent_type += 1;
        }
        if version <= 12 && ent_type >= 8 {
            ent_type += 1;
        }
        if version <= 14 && ent_type == 16 {
            position[2] += attrs[2] as f32;
            attrs[2] = 0;
            attrs[3] = 0;
        }
        if version <= 20 && ent_type >= 30 {
            ent_type += 1;
        }
        if version <= 21 && ent_type >= 31 {
            ent_type += 1;
        }
        if version <= 22 && ent_type >= 32 {
            ent_type += 1;
        }
        if version <= 23 && ent_type >= 33 {
            ent_type += 1;
        }
        if version <= 30 && (ent_type == 1 || ent_type == 16) {
            attrs[0] = (attrs[0] as i32 + 180).rem_euclid(360) as i16;
        }

        entities.push(Entity {
            position,
            attrs,
            ent_type,
            extra_data,
        });
    }

    let mut vslots = Vec::new();
    let mut num_tracked = map_vslots;
    while num_tracked > 0 {
        let changed = cursor.read_i32_le()?;
        if changed < 0 {
            let skip = changed
                .checked_neg()
                .ok_or(OgzError::LimitExceeded("vslot skip"))?;
            if skip > num_tracked {
                return Err(OgzError::LimitExceeded("vslot skip"));
            }
            for _ in 0..skip {
                vslots.push(None);
            }
            num_tracked -= skip;
        } else {
            let prev = cursor.read_i32_le()?;
            let mut slot = VSlot {
                prev,
                ..Default::default()
            };

            if changed & (1 << 0) != 0 {
                let params = cursor.read_u16_le()?;
                if params > 1024 {
                    return Err(OgzError::LimitExceeded("vslot params"));
                }
                for _ in 0..params {
                    let nlen = cursor.read_u16_le()?;
                    let mut nbuf = vec![0u8; nlen as usize];
                    cursor.read_exact(&mut nbuf)?;
                    let mut vals = [0f32; 4];
                    for v in &mut vals {
                        *v = cursor.read_f32_le()?;
                    }
                    slot.params.push(VSlotParam {
                        name: CubeString(nbuf),
                        values: vals,
                    });
                }
            }
            if changed & (1 << 1) != 0 {
                slot.scale = Some(cursor.read_f32_le()?);
            }
            if changed & (1 << 2) != 0 {
                slot.rotation = Some(cursor.read_i32_le()?);
            }
            if changed & (1 << 3) != 0 {
                slot.offset = Some((cursor.read_i32_le()?, cursor.read_i32_le()?));
            }
            if changed & (1 << 4) != 0 {
                slot.scroll = Some((cursor.read_f32_le()?, cursor.read_f32_le()?));
            }
            if changed & (1 << 5) != 0 {
                slot.layer = Some(cursor.read_i32_le()?);
            }
            if changed & (1 << 6) != 0 {
                slot.alpha = Some((cursor.read_f32_le()?, cursor.read_f32_le()?));
            }
            if changed & (1 << 7) != 0 {
                slot.color = Some((
                    cursor.read_f32_le()?,
                    cursor.read_f32_le()?,
                    cursor.read_f32_le()?,
                ));
            }

            vslots.push(Some(slot));
            num_tracked -= 1;
        }
    }

    // C++ loadchildren reads 8 worldroot cubes directly (no wrapper byte).
    // We wrap them in a Children node to fit the recursive OctreeNode type.
    let octree = {
        let mut children = Vec::with_capacity(8);
        for _ in 0..8 {
            children.push(read_cube(&mut cursor, version)?);
        }
        OctreeNode::Children(Box::new(children.try_into().unwrap()))
    };

    let mut lm_vec = Vec::new();
    if version >= 7 {
        for _ in 0..lightmaps {
            let mut ty = 0;
            let mut unlit = None;
            if version >= 17 {
                ty = cursor.read_u8()?;
                if version >= 20 && (ty & 0x80) != 0 {
                    unlit = Some((cursor.read_u16_le()?, cursor.read_u16_le()?));
                }
            }
            // LM_ALPHA = 0x10 (bit 4), LM_TYPE = 0x0F, LM_BUMPMAP1 = 2
            // bpp=4 only when alpha flag is set AND type is not BUMPMAP1
            let lm_type = ty & 0x7F; // strip unlit-coords flag before type check
            let bpp = if (lm_type & 0x10) != 0 && (lm_type & 0x0F) != 2 {
                4
            } else {
                3
            };
            let mut lm_data = vec![0u8; bpp * 512 * 512];
            cursor.read_exact(&mut lm_data)?;
            lm_vec.push(Lightmap {
                map_type: ty,
                unlit,
                data: lm_data,
            });
        }
    }

    let mut pvs_data = Vec::new();
    if version >= 25 && numpvs > 0 {
        pvs_data = vec![0u8; numpvs as usize];
        cursor.read_exact(&mut pvs_data)?;
    }

    let mut blendmap_data = None;
    if version >= 28 && blendmap_res > 0 {
        // Map version 28 has an engine quirk where it reads an extra int32
        // to dictate the actual resolution before reading the pixel array.
        let actual_res = if version == 28 {
            cursor.read_i32_le()?
        } else {
            blendmap_res
        };

        if actual_res <= 0 || actual_res > MAX_BLENDMAP_RES {
            return Err(OgzError::LimitExceeded("blendmap resolution"));
        }
        let size = (actual_res as usize)
            .checked_mul(actual_res as usize)
            .ok_or(OgzError::LimitExceeded("blendmap size overflow"))?;
        let mut bdata = vec![0u8; size];
        cursor.read_exact(&mut bdata)?;
        blendmap_data = Some(bdata);
        blendmap_res = actual_res; // Sync the resolution so it serializes properly later
    }

    Ok(OgzMap {
        version,
        world_size,
        config,
        game_type,
        eif,
        extra_game_data,
        mru_textures,
        entities,
        vslots,
        octree,
        lightmaps: lm_vec,
        pvs: pvs_data,
        blendmap_resolution: blendmap_res,
        blendmap_data,
    })
}

fn read_cube(cursor: &mut Cursor<&Vec<u8>>, version: i32) -> Result<OctreeNode, OgzError> {
    let octsav = cursor.read_u8()?;
    let ty = octsav & 0x07;

    match ty {
        0 => {
            // OCTSAV_CHILDREN
            let mut children = Vec::with_capacity(8);
            for _ in 0..8 {
                children.push(read_cube(cursor, version)?);
            }
            Ok(OctreeNode::Children(Box::new(children.try_into().unwrap())))
        }
        1..=4 => {
            let mut edges = [0u8; 12];
            if ty == 3 {
                cursor.read_exact(&mut edges)?;
            }

            let mut textures = [0u16; 6];
            for t in &mut textures {
                *t = if version < 14 {
                    cursor.read_u8()? as u16
                } else {
                    cursor.read_u16_le()?
                };
            }
            let surfaces = if version < 7 {
                cursor.read_exact(&mut [0; 3])?; // skip 3 legacy lightmap bytes, no surface data
                Surfaces::None
            } else if version <= 31 {
                Surfaces::Legacy(read_legacy_surfaces(cursor, version, octsav)?)
            } else {
                Surfaces::Modern(read_modern_surfaces(cursor, version, octsav)?)
            };

            let info = CubeInfo { textures, surfaces };

            match ty {
                1 => Ok(OctreeNode::Empty(info)),
                2 => Ok(OctreeNode::Solid(info)),
                3 => Ok(OctreeNode::Normal { info, edges }),
                4 => {
                    let mut children = Vec::with_capacity(8);
                    for _ in 0..8 {
                        children.push(read_cube(cursor, version)?);
                    }
                    Ok(OctreeNode::LodCube {
                        info,
                        children: Box::new(children.try_into().unwrap()),
                    })
                }
                _ => unreachable!(),
            }
        }
        _ => Err(OgzError::InvalidOctreeType(ty)),
    }
}

fn read_legacy_surfaces(
    cursor: &mut Cursor<&Vec<u8>>,
    version: i32,
    octsav: u8,
) -> Result<LegacySurfaceData, OgzError> {
    let mask = cursor.read_u8()?;
    let material = if (mask & 0x80) != 0 {
        Some(cursor.read_u8()?)
    } else {
        None
    };

    let mut surfaces = Vec::new();
    if (mask & 0x3F) != 0 {
        let mut numsurfs = 6;
        let mut i = 0;
        while i < numsurfs {
            if i >= 6 || (mask & (1 << i)) != 0 {
                let mut compat = [0u8; 16];
                cursor.read_exact(&mut compat)?;
                let normals = if i < 6 && (mask & 0x40) != 0 {
                    let mut n = [0u8; 12];
                    cursor.read_exact(&mut n)?;
                    Some(n)
                } else {
                    None
                };

                if (compat[15] & 2) != 0 {
                    numsurfs += 1;
                    if numsurfs > 12 {
                        return Err(OgzError::LimitExceeded("numsurfs"));
                    }
                }
                surfaces.push(LegacySurface { compat, normals });
            }
            i += 1;
        }
    }

    let merges = if version >= 20 && (octsav & 0x80) != 0 {
        let merged = cursor.read_u8()?;
        let mut merge_mask = 0;
        let mut compat = Vec::new();
        if (merged & 0x80) != 0 {
            merge_mask = cursor.read_u8()?;
            for i in 0..6 {
                if (merge_mask & (1 << i)) != 0 {
                    let mut arr = [0u16; 4];
                    for val in &mut arr {
                        *val = cursor.read_u16_le()?;
                    }
                    compat.push(arr);
                }
            }
        }
        Some(LegacyMerge {
            merged,
            merge_mask,
            compat,
        })
    } else {
        None
    };

    Ok(LegacySurfaceData {
        mask,
        material,
        surfaces,
        merges,
    })
}

fn read_modern_surfaces(
    cursor: &mut Cursor<&Vec<u8>>,
    version: i32,
    octsav: u8,
) -> Result<ModernSurfaceData, OgzError> {
    let material = if (octsav & 0x40) != 0 {
        Some(if version == 32 {
            cursor.read_u8()? as u16
        } else {
            cursor.read_u16_le()?
        })
    } else {
        None
    };

    let merged = if (octsav & 0x80) != 0 {
        Some(cursor.read_u8()?)
    } else {
        None
    };

    let surfaces = if (octsav & 0x20) != 0 {
        let surfmask = cursor.read_u8()?;
        let totalverts = cursor.read_u8()?;
        let mut faces = Vec::new();
        for i in 0..6 {
            if (surfmask & (1 << i)) != 0 {
                let mut lmid = [0u8; 2];
                cursor.read_exact(&mut lmid)?;
                let vertmask = cursor.read_u8()?;
                let numverts = cursor.read_u8()?;
                let layerverts = numverts & 0x0F;

                let mut hasxyz = (vertmask & 0x04) != 0;
                let mut hasuv = (vertmask & 0x40) != 0;
                let mut hasnorm = (vertmask & 0x80) != 0;

                let mut xyz = None;
                let mut uv = None;
                let mut dup_uv = None;

                if layerverts == 4 {
                    if hasxyz && (vertmask & 0x01) != 0 {
                        xyz = Some([
                            cursor.read_u16_le()?,
                            cursor.read_u16_le()?,
                            cursor.read_u16_le()?,
                            cursor.read_u16_le()?,
                        ]);
                        hasxyz = false;
                    }
                    if hasuv && (vertmask & 0x02) != 0 {
                        uv = Some([
                            cursor.read_u16_le()?,
                            cursor.read_u16_le()?,
                            cursor.read_u16_le()?,
                            cursor.read_u16_le()?,
                        ]);
                        if (numverts & 0x80) != 0 {
                            dup_uv = Some([
                                cursor.read_u16_le()?,
                                cursor.read_u16_le()?,
                                cursor.read_u16_le()?,
                                cursor.read_u16_le()?,
                            ]);
                        }
                        hasuv = false;
                    }
                }

                let norm = if hasnorm && (vertmask & 0x08) != 0 {
                    hasnorm = false;
                    Some(cursor.read_u16_le()?)
                } else {
                    None
                };

                let mut verts = Vec::with_capacity(layerverts as usize);
                if hasxyz || hasuv || hasnorm {
                    for _ in 0..layerverts {
                        let vx = if hasxyz {
                            Some([cursor.read_u16_le()?, cursor.read_u16_le()?])
                        } else {
                            None
                        };
                        let vu = if hasuv {
                            Some([cursor.read_u16_le()?, cursor.read_u16_le()?])
                        } else {
                            None
                        };
                        let vn = if hasnorm {
                            Some(cursor.read_u16_le()?)
                        } else {
                            None
                        };
                        verts.push(ModernVert {
                            xyz: vx,
                            uv: vu,
                            norm: vn,
                        });
                    }
                }

                let mut dup_verts = Vec::new();
                if (numverts & 0x80) != 0 {
                    for _ in 0..layerverts {
                        let vu = if hasuv {
                            Some([cursor.read_u16_le()?, cursor.read_u16_le()?])
                        } else {
                            None
                        };
                        dup_verts.push(ModernDupVert { uv: vu });
                    }
                }

                faces.push(ModernFace {
                    lmid,
                    vertmask,
                    numverts,
                    xyz,
                    uv,
                    dup_uv,
                    norm,
                    verts,
                    dup_verts,
                });
            }
        }
        Some(ModernSurfaces {
            surfmask,
            totalverts,
            faces,
        })
    } else {
        None
    };

    Ok(ModernSurfaceData {
        material,
        merged,
        surfaces,
    })
}
