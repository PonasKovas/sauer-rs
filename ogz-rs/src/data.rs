use std::fmt;

/// Represents a custom 8-bit character string used by Cube 2.
/// This format does not strictly map to UTF-8.
#[derive(Clone, PartialEq, Eq, PartialOrd)]
pub struct CubeString(pub Vec<u8>);

impl CubeString {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        CubeString(bytes.to_vec())
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for CubeString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\"{}\"", String::from_utf8_lossy(&self.0))
    }
}

/// The top-level representation of an `.ogz` map.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct OgzMap {
    pub version: i32,
    pub world_size: i32,
    pub config: MapConfig,

    // Game Extras
    pub game_type: Option<CubeString>,
    pub eif: u16, // Extra entity info size
    pub extra_game_data: Vec<u8>,

    pub mru_textures: Vec<u16>,
    pub entities: Vec<Entity>,
    pub vslots: Vec<Option<VSlot>>,
    pub octree: OctreeNode,

    pub lightmaps: Vec<Lightmap>,
    pub pvs: Vec<u8>,

    pub blendmap_resolution: i32,
    pub blendmap_data: Option<Vec<u8>>,
}

impl OgzMap {
    /// Deserializes a map from a raw byte slice (the contents of the `.ogz` file).
    pub fn from_bytes(data: &[u8]) -> Result<Self, crate::error::OgzError> {
        crate::de::parse_map(data)
    }

    /// Serializes the map to the modern Version 33 `.ogz` byte array.
    pub fn to_bytes(&self) -> Result<Vec<u8>, crate::error::OgzError> {
        crate::ser::serialize_map(self)
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum MapConfig {
    Modern {
        variables: Vec<Variable>,
    },
    Legacy {
        lightprecision: i32,
        lighterror: i32,
        lightlod: i32,
        ambient: u8,
        watercolour: [u8; 3],
        blendmap_mode: u8,
        lerpangle: u8,
        lerpsubdiv: u8,
        lerpsubdivsize: u8,
        bumperror: u8,
        skylight: [u8; 3],
        lavacolour: [u8; 3],
        waterfallcolour: [u8; 3],
        maptitle: CubeString,
    },
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum Variable {
    Int { name: CubeString, value: i32 },
    Float { name: CubeString, value: f32 },
    String { name: CubeString, value: CubeString },
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Entity {
    pub position: [f32; 3],
    pub attrs: [i16; 5],
    pub ent_type: u8,
    pub extra_data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct VSlotParam {
    pub name: CubeString,
    pub values: [f32; 4],
}

#[derive(Debug, Clone, Default, PartialEq, PartialOrd)]
pub struct VSlot {
    pub prev: i32,
    pub params: Vec<VSlotParam>,
    pub scale: Option<f32>,
    pub rotation: Option<i32>,
    pub offset: Option<(i32, i32)>,
    pub scroll: Option<(f32, f32)>,
    pub layer: Option<i32>,
    pub alpha: Option<(f32, f32)>,
    pub color: Option<(f32, f32, f32)>,
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

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct LegacySurfaceData {
    pub mask: u8,
    pub material: Option<u8>,
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
    pub material: Option<u16>,
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
