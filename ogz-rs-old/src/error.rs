use thiserror::Error;

#[derive(Debug, Error)]
pub enum OgzError {
    // ── Parse-time errors ─────────────────────────────────────────────────────

    /// The first 4 bytes are not `b"OCTA"`.
    #[error("invalid magic: expected b\"OCTA\", found {found:?}")]
    InvalidMagic { found: [u8; 4] },

    /// The header version field is not 33.
    #[error("unsupported format version {found}; only version 33 is supported")]
    UnsupportedVersion { found: i32 },

    /// The data ended before all expected bytes could be read.
    #[error("unexpected end of data")]
    Incomplete,

    /// A variable-type discriminant byte was not 0, 1, or 2.
    #[error("unknown variable type byte 0x{0:02X}")]
    UnknownVarType(u8),

    /// An octsav byte contained an unrecognised node-type nibble (bits 0–2).
    #[error("unknown octree node type 0x{0:02X}")]
    UnknownOctreeNodeType(u8),

    /// A blendmap node-type byte was not 0, 1, or 2.
    #[error("unknown blendmap node type 0x{0:02X}")]
    UnknownBlendmapNodeType(u8),

    /// A length-prefixed string declared more bytes than the safety limit.
    #[error("string length {length} exceeds the safety limit of {limit}")]
    StringTooLong { length: usize, limit: usize },

    /// A numeric count or size field from the file is outside its valid range `[0, max]`.
    #[error("field '{field}' has value {value}, which is outside the valid range [0, {max}]")]
    CountOutOfRange { field: &'static str, value: i64, max: i64 },

    /// A surface face declared more than 15 vertices (the engine's `MAXFACEVERTS`).
    #[error("surface face declared {count} vertices; the maximum is 15")]
    TooManyFaceVerts { count: usize },

    /// Recursive parsing exceeded the depth safety limit.
    #[error("{structure} recursion reached depth {depth}, which exceeds the safety limit")]
    DepthLimitExceeded { structure: &'static str, depth: u32 },

    // ── Write / validation errors ─────────────────────────────────────────────

    /// `world_size` is not a positive power of 2.
    #[error("world_size {0} is not a positive power of 2")]
    InvalidWorldSize(i32),

    /// An entity's `extra_payload` length does not match `GameTypeData::extra_entity_info_size`.
    #[error(
        "entity {index} extra_payload length {found} \
         does not match extra_entity_info_size {expected}"
    )]
    EntityPayloadLengthMismatch { index: usize, expected: usize, found: usize },

    /// A string is too long to fit in its on-disk length field.
    #[error("'{field}' string length {length} does not fit in {bits}-bit length prefix")]
    StringOverflow { field: &'static str, length: usize, bits: u8 },

    /// A collection is too large to fit in its on-disk count field.
    #[error("'{field}' count {count} does not fit in a 32-bit signed integer")]
    CountOverflow { field: &'static str, count: usize },
}
