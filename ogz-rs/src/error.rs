use thiserror::Error;

/// Defines all possible errors during OGZ parsing or serialization.
#[derive(Debug, Error)]
pub enum OgzError {
    #[error("I/O Error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Not a valid/supported GZIP file")]
    InvalidGzip,

    #[error("Invalid magic identifier, expected 'OCTA'")]
    InvalidMagic,

    #[error("Unsupported map version: {0}")]
    UnsupportedVersion(i32),

    #[error("Invalid octree node type: {0}")]
    InvalidOctreeType(u8),

    #[error("Cannot serialize map with legacy surfaces to v33. Convert to modern format first.")]
    CannotSerializeLegacy,
}
