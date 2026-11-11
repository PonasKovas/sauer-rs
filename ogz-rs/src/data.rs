use cube_string::CubeString;

pub use sauer_core::{
    Clip, Entity, EntityType, ItemType, Material, VSlot, VSlotParam, VSlotProps, Volume,
};

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum Variable {
    Int { name: CubeString, value: i32 },
    Float { name: CubeString, value: f32 },
    String { name: CubeString, value: CubeString },
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum OctreeNode {
    Children(Box<[OctreeNode; 8]>),
    Empty(CubeInfo),
    Solid(CubeInfo),
    Normal {
        info: CubeInfo,
        edges: [u8; 12],
    },
    LodCube {
        info: CubeInfo,
        children: Box<[OctreeNode; 8]>,
    },
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct CubeInfo {
    pub textures: [u16; 6],
    pub surfaces: Surfaces,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum Surfaces {
    None,
    Legacy(LegacySurfaceData),
    Modern(ModernSurfaceData),
}

impl Surfaces {
    pub fn material(&self) -> Option<Material> {
        match self {
            Self::Modern(m) => m.material,
            Self::Legacy(l) => l.material,
            Self::None => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct LegacySurfaceData {
    pub mask: u8,
    pub material: Option<Material>,
    pub surfaces: Vec<LegacySurface>,
    pub merges: Option<LegacyMerge>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct LegacySurface {
    pub compat: [u8; 16],
    pub normals: Option<[u8; 12]>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct LegacyMerge {
    pub merged: u8,
    pub merge_mask: u8,
    pub compat: Vec<[u16; 4]>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct ModernSurfaceData {
    pub material: Option<Material>,
    pub merged: Option<u8>,
    pub surfaces: Option<ModernSurfaces>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct ModernSurfaces {
    pub surfmask: u8,
    pub totalverts: u8,
    pub faces: Vec<ModernFace>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct ModernFace {
    pub lmid: [u8; 2],
    pub vertmask: u8,
    pub numverts: u8,
    pub xyz: Option<[u16; 4]>,
    pub uv: Option<[u16; 4]>,
    pub dup_uv: Option<[u16; 4]>,
    pub norm: Option<u16>,
    pub verts: Vec<ModernVert>,
    pub dup_verts: Vec<ModernDupVert>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct ModernVert {
    pub xyz: Option<[u16; 2]>,
    pub uv: Option<[u16; 2]>,
    pub norm: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct ModernDupVert {
    pub uv: Option<[u16; 2]>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Lightmap {
    pub map_type: u8,
    pub unlit: Option<(u16, u16)>,
    pub data: Vec<u8>,
}
