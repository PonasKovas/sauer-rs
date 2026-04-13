// ─── SauerString ─────────────────────────────────────────────────────────────

use std::fmt;

/// A byte string in Sauerbraten's encoding (not necessarily UTF-8).
///
/// Sauerbraten stores map variable names, values, shader parameter names, and
/// the game-type string as raw byte sequences that may contain arbitrary 8-bit
/// values.  This newtype preserves the original bytes and provides lossy
/// ASCII/UTF-8 `Display`/`Debug` implementations for human-readable output.
#[derive(Clone, PartialEq, Eq, Hash, Default)]
pub struct SauerString(Vec<u8>);

impl SauerString {
    pub fn from_bytes(bytes: Vec<u8>) -> Self { Self(bytes) }
    pub fn as_bytes(&self) -> &[u8] { &self.0 }
    pub fn into_bytes(self) -> Vec<u8> { self.0 }
    pub fn len(&self) -> usize { self.0.len() }
    pub fn is_empty(&self) -> bool { self.0.is_empty() }
}

impl fmt::Debug for SauerString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "b\"")?;
        for &b in &self.0 {
            match b {
                b'\\' => write!(f, "\\\\")?,
                b'"'  => write!(f, "\\\"")?,
                0x20..=0x7e => write!(f, "{}", b as char)?,
                _ => write!(f, "\\x{:02x}", b)?,
            }
        }
        write!(f, "\"")
    }
}

impl fmt::Display for SauerString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", String::from_utf8_lossy(&self.0))
    }
}

impl From<Vec<u8>> for SauerString {
    fn from(v: Vec<u8>) -> Self { Self(v) }
}

impl From<&[u8]> for SauerString {
    fn from(v: &[u8]) -> Self { Self(v.to_vec()) }
}

impl From<&str> for SauerString {
    fn from(s: &str) -> Self { Self(s.as_bytes().to_vec()) }
}

// ─────────────────────────────────────────────────────────────────────────────

/// The fully parsed Cube 2: Sauerbraten map (version 33).
///
/// All counts that appear in the on-disk header (`num_ents`, `num_vars`, etc.) are
/// derived from the lengths of the corresponding `Vec` fields when writing, so there
/// is exactly one source of truth for each datum.
#[derive(Debug, Clone)]
pub struct Map {
    /// Side-length of the cubic world; must be a positive power of 2 (e.g. 1024).
    pub world_size: i32,
    pub variables: Vec<MapVariable>,
    pub game_type: GameTypeData,
    pub entities: Vec<Entity>,
    /// Flat array of visual texture-slot variants.
    ///
    /// The octree geometry references entries in this array by index (via
    /// `textures: [u16; 6]` in each leaf node).  Indices therefore must not be
    /// reordered without also updating every referencing octree node.
    pub vslots: Vec<VSlot>,
    pub octree_root: OctreeNode,
    pub lightmaps: Vec<Lightmap>,
    pub pvs: Option<PvsData>,
    pub blendmap: Option<BlendmapNode>,
}

// ─── map variables ────────────────────────────────────────────────────────────

/// A typed value stored in a [`MapVariable`].
#[derive(Debug, Clone)]
pub enum MapVariableValue {
    Int(i32),
    Float(f32),
    String(SauerString),
}

/// An engine variable overridden by this map (e.g. `"fog"`, `"skybox"`).
///
/// These correspond to Sauerbraten's `VAR`/`FVAR`/`SVAR` declarations and are
/// applied by the engine when the map is loaded.
#[derive(Debug, Clone)]
pub struct MapVariable {
    pub name: SauerString,
    pub value: MapVariableValue,
}

// ─── game type / extras ───────────────────────────────────────────────────────

/// The game module that created this map.
///
/// Each compiled Sauerbraten game module exposes a hardcoded `gameident()` string.
/// The standard distribution ships two:
///
/// - `"fps"` — the default first-person-shooter mode.
/// - `"rpg"` — the RPG mode.
///
/// When the engine loads a map whose game type differs from the running module,
/// it emits a warning and ignores all entities beyond lights and map-models.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameType {
    /// Standard first-person-shooter module (`"fps"`).
    Fps,
    /// RPG module (`"rpg"`).
    Rpg,
    /// An unrecognised game module identifier.
    Unknown(SauerString),
}

impl GameType {
    /// The on-disk byte representation of this game type.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Fps => b"fps",
            Self::Rpg => b"rpg",
            Self::Unknown(s) => s.as_bytes(),
        }
    }
}

impl From<SauerString> for GameType {
    fn from(s: SauerString) -> Self {
        match s.as_bytes() {
            b"fps" => Self::Fps,
            b"rpg" => Self::Rpg,
            _ => Self::Unknown(s),
        }
    }
}

/// Game-type identity string and per-module extras stored after the header.
#[derive(Debug, Clone)]
pub struct GameTypeData {
    /// Which game module saved this map.
    pub game_type: GameType,
    /// Number of extra bytes appended to each entity's 24-byte on-disk struct.
    ///
    /// Set by the game module's `extraentinfosize()` function.  For the standard
    /// FPS game this is always **0**.  A non-zero value means each [`Entity`]'s
    /// `extra_payload` carries that many game-specific bytes.
    pub extra_entity_info_size: u16,
    /// Opaque game-module-specific blob written by `game::writegamedata()`.
    ///
    /// For the standard FPS game this is always empty (`writegamedata` is a no-op).
    /// Third-party modules may store arbitrary binary data here.
    pub extra_game_data: Vec<u8>,
    /// Most-recently-used texture order: indices into the map's [`VSlot`] array.
    ///
    /// The editor palette uses this list to show the most recently applied
    /// textures first.  Each entry is a valid index into `Map::vslots`.
    pub texture_mru: Vec<u16>,
}

// ─── entities ─────────────────────────────────────────────────────────────────

/// Engine-defined entity types (IDs 0–7) plus FPS-game-specific types (IDs 8–30).
///
/// When loading a map from a different game module, the engine preserves only
/// `Light` and `MapModel` entities; all others are ignored.
///
/// The meaning of each `attrs` slot in [`Entity`] is type-specific; see the
/// doc comment on each variant for details.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityType {
    // ── Engine types (0–7, always meaningful) ────────────────────────────────
    /// Unused slot.
    Empty,
    /// Point light source.
    ///
    /// - `attrs[0]` = radius
    /// - `attrs[1]` = intensity
    Light,
    /// External 3D map-model.
    ///
    /// - `attrs[0]` = yaw angle (0–360)
    /// - `attrs[1]` = model index (into the `mapmodel` list)
    MapModel,
    /// Player spawn point.
    ///
    /// - `attrs[0]` = yaw angle
    /// - `attrs[1]` = team (0 = neutral)
    PlayerStart,
    /// Environment-map (reflection probe).
    ///
    /// - `attrs[0]` = radius
    EnvMap,
    /// Particle emitter.
    Particles,
    /// Positional ambient sound.
    Sound,
    /// Directional spotlight (requires a paired `Light`).
    SpotLight,

    // ── FPS-game-specific types (8–30) ───────────────────────────────────────
    /// Shotgun shells pickup.
    Shells,
    /// Machine-gun bullets pickup.
    Bullets,
    /// Rocket-launcher ammo pickup.
    Rockets,
    /// Sniper-rifle rounds pickup.
    Rounds,
    /// Grenade-launcher ammo pickup.
    Grenades,
    /// Pistol cartridges pickup.
    Cartridges,
    /// Health pack pickup.
    Health,
    /// Health boost pickup.
    Boost,
    /// Green armour pickup (20 % damage reduction).
    GreenArmour,
    /// Yellow armour pickup (40 % damage reduction).
    YellowArmour,
    /// Quad-damage power-up pickup.
    Quad,
    /// Teleporter entrance.
    ///
    /// - `attrs[0]` = destination index (matching `TeleDest.attrs[1]`)
    /// - `attrs[1]` = model index
    /// - `attrs[2]` = tag
    Teleport,
    /// Teleporter exit/destination.
    ///
    /// - `attrs[0]` = yaw angle
    /// - `attrs[1]` = teleport index (links back to `Teleport.attrs[0]`)
    TeleDest,
    /// AI monster.
    ///
    /// - `attrs[0]` = yaw angle
    /// - `attrs[1]` = monster type ID
    Monster,
    /// Bonus collectible ("carrot").
    ///
    /// - `attrs[0]` = tag
    /// - `attrs[1]` = type
    Carrot,
    /// Jump-pad that launches the player.
    ///
    /// - `attrs[0]` = Z velocity
    /// - `attrs[1]` = Y velocity
    /// - `attrs[2]` = X velocity
    JumpPad,
    /// Capture-the-flag / capture base.
    Base,
    /// Secondary respawn point.
    RespawnPoint,
    /// Interactive physics box.
    ///
    /// - `attrs[0]` = yaw angle
    /// - `attrs[1]` = model index
    /// - `attrs[2]` = weight
    Box,
    /// Destructible barrel.
    ///
    /// - `attrs[0]` = yaw angle
    /// - `attrs[1]` = model index
    /// - `attrs[2]` = weight
    /// - `attrs[3]` = health points
    Barrel,
    /// Moving platform.
    ///
    /// - `attrs[0]` = yaw angle
    /// - `attrs[1]` = model index
    /// - `attrs[2]` = trigger tag
    /// - `attrs[3]` = speed
    Platform,
    /// Moving elevator.
    ///
    /// - `attrs[0]` = yaw angle
    /// - `attrs[1]` = model index
    /// - `attrs[2]` = trigger tag
    /// - `attrs[3]` = speed
    Elevator,
    /// Capture-the-flag flag.
    ///
    /// - `attrs[0]` = yaw angle
    /// - `attrs[1]` = team
    Flag,
    /// An unrecognised or game-specific entity type not listed above.
    Unknown(u8),
}

impl EntityType {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0  => Self::Empty,
            1  => Self::Light,
            2  => Self::MapModel,
            3  => Self::PlayerStart,
            4  => Self::EnvMap,
            5  => Self::Particles,
            6  => Self::Sound,
            7  => Self::SpotLight,
            8  => Self::Shells,
            9  => Self::Bullets,
            10 => Self::Rockets,
            11 => Self::Rounds,
            12 => Self::Grenades,
            13 => Self::Cartridges,
            14 => Self::Health,
            15 => Self::Boost,
            16 => Self::GreenArmour,
            17 => Self::YellowArmour,
            18 => Self::Quad,
            19 => Self::Teleport,
            20 => Self::TeleDest,
            21 => Self::Monster,
            22 => Self::Carrot,
            23 => Self::JumpPad,
            24 => Self::Base,
            25 => Self::RespawnPoint,
            26 => Self::Box,
            27 => Self::Barrel,
            28 => Self::Platform,
            29 => Self::Elevator,
            30 => Self::Flag,
            n  => Self::Unknown(n),
        }
    }

    pub fn to_u8(self) -> u8 {
        match self {
            Self::Empty         => 0,
            Self::Light         => 1,
            Self::MapModel      => 2,
            Self::PlayerStart   => 3,
            Self::EnvMap        => 4,
            Self::Particles     => 5,
            Self::Sound         => 6,
            Self::SpotLight     => 7,
            Self::Shells        => 8,
            Self::Bullets       => 9,
            Self::Rockets       => 10,
            Self::Rounds        => 11,
            Self::Grenades      => 12,
            Self::Cartridges    => 13,
            Self::Health        => 14,
            Self::Boost         => 15,
            Self::GreenArmour   => 16,
            Self::YellowArmour  => 17,
            Self::Quad          => 18,
            Self::Teleport      => 19,
            Self::TeleDest      => 20,
            Self::Monster       => 21,
            Self::Carrot        => 22,
            Self::JumpPad       => 23,
            Self::Base          => 24,
            Self::RespawnPoint  => 25,
            Self::Box           => 26,
            Self::Barrel        => 27,
            Self::Platform      => 28,
            Self::Elevator      => 29,
            Self::Flag          => 30,
            Self::Unknown(n)    => n,
        }
    }
}

/// A map entity.
///
/// The meaning of `attrs` is entirely determined by `entity_type`; refer to the
/// documentation on [`EntityType`] for per-type field semantics.
///
/// Entities with IDs beyond `MAXENTS` (10 000) that appear in the file are
/// consumed during parsing but discarded.
#[derive(Debug, Clone)]
pub struct Entity {
    pub position: mint::Vector3<f32>,
    /// Generic attribute slots.  See [`EntityType`] for per-type semantics.
    pub attrs: [i16; 5],
    pub entity_type: EntityType,
    /// Game-module-specific extra bytes.
    ///
    /// Length equals `GameTypeData::extra_entity_info_size`.  For the standard
    /// FPS game this is always empty.
    pub extra_payload: Vec<u8>,
}

// ─── vslots ───────────────────────────────────────────────────────────────────

/// A named parameter passed to a surface shader.
#[derive(Debug, Clone)]
pub struct ShaderParam {
    pub name: SauerString,
    pub val: [f32; 4],
}

/// A visual texture slot (VSlot).
///
/// VSlots are stored as a flat array in [`Map::vslots`].  Each entry is either
/// an [`Empty`](VSlot::Empty) placeholder or a [`Data`](VSlot::Data) record
/// that overrides one or more properties relative to a predecessor slot.
///
/// At runtime the engine chains VSlots into linked lists (one list per base
/// `Slot`), using [`VSlotData::predecessor`] to reconstruct the chain order.
/// The octree geometry refers to VSlots **by their index** in `Map::vslots`,
/// so the array must not be reordered.
///
/// On disk, consecutive `Empty` entries are run-length-encoded as a single
/// negative `changed` integer; this is handled transparently during
/// parsing and serialisation.
#[derive(Debug, Clone)]
pub enum VSlot {
    /// Default slot with no overrides.
    Empty,
    /// A slot that overrides one or more texture properties.
    Data(VSlotData),
}

/// The override data for a non-empty [`VSlot`].
#[derive(Debug, Clone)]
pub struct VSlotData {
    /// Index of the VSlot that should have its runtime `next` pointer directed
    /// at this one, used by the engine to reconstruct the variant linked list
    /// after loading.
    ///
    /// `None` means this VSlot is the head of its variant chain (no predecessor).
    /// This is a file-format-level concept only; the field is not transmitted
    /// over the network.
    pub predecessor: Option<u32>,
    pub shader_params: Vec<ShaderParam>,
    pub scale: Option<f32>,
    pub rotation: Option<i32>,
    pub offset: Option<[i32; 2]>,
    pub scroll: Option<[f32; 2]>,
    pub layer: Option<i32>,
    pub alpha: Option<[f32; 2]>,
    pub color: Option<[f32; 3]>,
}

// ─── octree geometry ──────────────────────────────────────────────────────────

/// Per-vertex data within a surface.
#[derive(Debug, Clone)]
pub struct SurfaceVert {
    /// Local X/Y on the face plane; present when `hasXYZ` flag is set.
    pub xy: Option<[u16; 2]>,
    /// U/V texture coordinates; present when `hasUV` flag is set.
    pub uv: Option<[u16; 2]>,
    /// Packed vertex normal; present when `hasNorm` flag is set.
    pub norm: Option<u16>,
}

impl SurfaceInfo {
    /// Number of top-layer vertices (lower 4 bits of the on-disk `numverts` byte).
    pub fn layer_verts(&self) -> usize {
        self.verts.len()
    }

    /// Whether this surface has a duplicate UV layer.
    pub fn has_dup_layer(&self) -> bool {
        self.uv_plane_dup.is_some() || !self.verts_dup_uv.is_empty()
    }

    /// Reconstruct the on-disk `numverts` byte from the parsed fields.
    pub fn num_verts_raw(&self) -> u8 {
        (self.verts.len() as u8 & 0x0F)
            | if self.layer_top    { 0x20 } else { 0 }  // LAYER_TOP
            | if self.layer_bottom { 0x40 } else { 0 }  // LAYER_BOTTOM
            | if self.has_dup_layer() { 0x80 } else { 0 } // LAYER_DUP
    }
}

/// Compute the vertex-buffer offset for each surface in a node's surface array.
///
/// Returns an array of 6 offsets (one per face orientation).  Surfaces with
/// zero layer-verts receive offset 0 and do not advance the running counter,
/// matching the engine's runtime vertex-buffer layout.
pub fn surface_vert_offsets(surfaces: &NodeSurfaces) -> [u8; 6] {
    let mut offsets = [0u8; 6];
    let mut offset: u8 = 0;
    for (i, surf) in surfaces.iter().enumerate() {
        if let Some(s) = surf {
            let lv = s.layer_verts() as u8;
            offsets[i] = if lv > 0 { offset } else { 0 };
            offset += lv;
            if s.has_dup_layer() { offset += lv; }
        }
    }
    offsets
}

/// Surface data for a single face of a geometry node.
///
/// `num_verts_raw` is derived from the payload when writing.  `verts_mask` is
/// stored explicitly because bits 0x01/0x02/0x08 (compact-xyz, compact-uv,
/// compact-norm) can be set by the engine as geometry hints even when the
/// corresponding data is absent, so they cannot be derived from the payload
/// alone.
#[derive(Debug, Clone)]
pub struct SurfaceInfo {
    pub lmid: [u8; 2],
    /// Raw `verts` byte from the on-disk `surfaceinfo` struct.
    ///
    /// Bit layout:
    /// - `0x01`: compact XYZ bounding stored (only read when `0x04` is also set and layerverts==4)
    /// - `0x02`: compact UV plane stored (only read when `0x40` is also set and layerverts==4)
    /// - `0x04`: per-vertex XY data present
    /// - `0x08`: compact base-norm stored (only read when `0x80` is also set)
    /// - `0x40`: per-vertex UV data present
    /// - `0x80`: per-vertex norm data present
    pub verts_mask: u8,
    /// Whether the `LAYER_TOP` flag (bit 5, 0x20 of `num_verts_raw`) is set.
    ///
    /// Set by the engine's lightmapper for surfaces that use the primary
    /// lightmap channel.  Must be round-tripped exactly.
    pub layer_top: bool,
    /// Whether the `LAYER_BOTTOM` flag (bit 6, 0x40 of `num_verts_raw`) is set.
    ///
    /// Set when the surface uses both lightmap channels for blending (LAYER_BLEND).
    /// Stored purely as a rendering hint; does not change the on-disk data layout.
    /// Must be round-tripped exactly.
    pub layer_bottom: bool,
    /// Compressed XYZ bounding (4 × u16); only for 4-vert faces with compact XYZ.
    pub xyz_bound: Option<[u16; 4]>,
    /// Compressed UV plane (4 × u16); only for 4-vert faces with compact UV.
    pub uv_plane: Option<[u16; 4]>,
    /// Layer-dup UV plane; only when `verts_dup_uv` is non-empty.
    pub uv_plane_dup: Option<[u16; 4]>,
    /// Shared base normal for all vertices when compact norm flag is set.
    pub base_norm: Option<u16>,
    pub verts: Vec<SurfaceVert>,
    /// Layer-dup per-vertex UV overrides.
    pub verts_dup_uv: Vec<[u16; 2]>,
}

/// Surface data for all 6 faces of a node.  `None` = no surface on that face.
///
/// The `surf_mask` byte is derived from `Option` presence when writing.
pub type NodeSurfaces = [Option<SurfaceInfo>; 6];

/// A node in the world octree.
///
/// The root node is at `Map::octree_root` and represents the full
/// `(0,0,0)..(world_size, world_size, world_size)` cube.  Children are ordered
/// by `i = (z<<2) | (y<<1) | x` where x, y, z ∈ {0, 1}.
#[derive(Debug, Clone)]
pub enum OctreeNode {
    /// Subdivided into 8 child nodes (OCTSAV_CHILDREN).
    Children {
        children: Box<[OctreeNode; 8]>,
        material: Option<u16>,
        merged: Option<u8>,
    },
    /// Empty (air) space (OCTSAV_EMPTY).
    Empty {
        textures: [u16; 6],
        material: Option<u16>,
        merged: Option<u8>,
        surfaces: Option<Box<NodeSurfaces>>,
    },
    /// Entirely solid geometry (OCTSAV_SOLID).
    Solid {
        textures: [u16; 6],
        material: Option<u16>,
        merged: Option<u8>,
        surfaces: Option<Box<NodeSurfaces>>,
    },
    /// Partially solid with edge-based clipping (OCTSAV_NORMAL).
    ///
    /// The 12 `edges` bytes encode the clip shape of the cube along each axis.
    Normal {
        textures: [u16; 6],
        edges: [u8; 12],
        material: Option<u16>,
        merged: Option<u8>,
        surfaces: Option<Box<NodeSurfaces>>,
    },
    /// Level-of-detail cube: rendered as the children at close range (OCTSAV_LODCUBE).
    ///
    /// Unlike `Children`, a `LodCube` still carries its own textures and optional
    /// surface data (they are read from the file before the children, just as for
    /// `Empty`/`Solid`/`Normal`).
    LodCube {
        textures: [u16; 6],
        children: Box<[OctreeNode; 8]>,
        material: Option<u16>,
        merged: Option<u8>,
        surfaces: Option<Box<NodeSurfaces>>,
    },
}

// ─── lightmaps ────────────────────────────────────────────────────────────────

/// A pre-baked lightmap texture (512 × 512 pixels).
#[derive(Debug, Clone)]
pub struct Lightmap {
    /// Lower 7 bits encode the type:
    /// - `0` = diffuse lightmap (3 or 4 BPP)
    /// - `1` = bumpmap layer 0
    /// - `2` = bumpmap layer 1
    ///
    /// Bit 6 (`0x10`) selects 4 BPP (RGBA) when set and type is not 2,
    /// otherwise 3 BPP (RGB).
    ///
    /// The `has_unlit` flag (bit 7, `0x80`) is derived from `unlit_coords`.
    pub lm_type: u8,
    /// Pixel coordinates of the "unlit" sentinel texel, if present.
    pub unlit_coords: Option<[u16; 2]>,
    /// Raw pixel data: 512 × 512 × BPP bytes (BPP = 3 or 4).
    pub data: Vec<u8>,
}

// ─── PVS ──────────────────────────────────────────────────────────────────────

/// A node in the PVS viewcell octree.
///
/// Parsed trees always have a [`Branch`](ViewcellsNode::Branch) at the root.
/// [`Leaf`](ViewcellsNode::Leaf) nodes only appear as children of a branch and
/// store an index into [`PvsData::block_lengths`].
#[derive(Debug, Clone)]
pub enum ViewcellsNode {
    Leaf(i32),
    Branch(Box<[ViewcellsNode; 8]>),
}

/// Potentially Visible Set data.
///
/// The PVS accelerates rendering by recording, for each viewcell, which other
/// cells are potentially visible.
#[derive(Debug, Clone)]
pub struct PvsData {
    /// Water-plane heights used for underwater visibility culling.
    pub water_heights: Vec<i32>,
    /// Per-block byte-lengths; its length equals the `num_pvs` field in the
    /// header, which is the number of distinct PVS blocks.
    pub block_lengths: Vec<u16>,
    /// Flat binary PVS buffer referenced by [`ViewcellsNode::Leaf`] indices.
    pub buffer: Vec<u8>,
    /// Root of the viewcell octree.  Always a [`Branch`](ViewcellsNode::Branch).
    pub viewcells: ViewcellsNode,
}

// ─── blendmap ─────────────────────────────────────────────────────────────────

/// A node in the terrain-blending quadtree.
///
/// The blendmap controls alpha-blending between terrain texture layers.
#[derive(Debug, Clone)]
pub enum BlendmapNode {
    /// Subdivided into 4 quadrant children.
    Branch(Box<[BlendmapNode; 4]>),
    /// Entire quadrant shares one blend value (0–255).
    Solid(u8),
    /// High-resolution 64×64 blend grid (4 096 raw bytes).
    Image(Box<[u8; 4096]>),
}
