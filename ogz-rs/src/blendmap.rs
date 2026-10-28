//! The blendmap is a quadtree over the map's XY plane (engine `blend.cpp`).
//! `ogz-rs` keeps it as the raw serialized tree; this module knows its layout.

use crate::error::OgzError;
use byteorder::ReadBytesExt;
use std::io::Read;

pub const BM_BRANCH: u8 = 0;
pub const BM_SOLID: u8 = 1;
pub const BM_IMAGE: u8 = 2;
const BM_IMAGE_BYTES: usize = 64 * 64;
const MAX_DEPTH: usize = 32;

/// Copies one serialized blendmap node (and its children) from `r` into `out`.
pub(crate) fn read_node<R: Read>(
    r: &mut R,
    out: &mut Vec<u8>,
    depth: usize,
) -> Result<(), OgzError> {
    if depth > MAX_DEPTH {
        return Err(OgzError::LimitExceeded("blendmap depth"));
    }
    let ty = r.read_u8()?;
    out.push(ty);
    match ty {
        BM_SOLID => out.push(r.read_u8()?),
        BM_IMAGE => {
            let start = out.len();
            out.resize(start + BM_IMAGE_BYTES, 0);
            r.read_exact(&mut out[start..])?;
        }
        BM_BRANCH => {
            for _ in 0..4 {
                read_node(r, out, depth + 1)?;
            }
        }
        _ => return Err(OgzError::InvalidBlendmapNode(ty)),
    }
    Ok(())
}

/// Mirrors the engine's `enlargeblendmap`: the old tree becomes the first
/// quadrant of a new root, the other three are untouched (solid 0xFF).
pub fn enlarged(data: &[u8]) -> Vec<u8> {
    if data.first() == Some(&BM_SOLID) {
        return data.to_vec();
    }
    let mut out = Vec::with_capacity(data.len() + 7);
    out.push(BM_BRANCH);
    out.extend_from_slice(data);
    for _ in 0..3 {
        out.extend_from_slice(&[BM_SOLID, 0xFF]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        read_node(&mut &data[..], &mut out, 0).unwrap();
        out
    }

    #[test]
    fn reads_whole_tree() {
        let mut tree = vec![BM_BRANCH, BM_SOLID, 7, BM_IMAGE];
        tree.extend(std::iter::repeat_n(3, BM_IMAGE_BYTES));
        tree.extend_from_slice(&[BM_SOLID, 0xFF, BM_SOLID, 0]);
        let mut with_trailer = tree.clone();
        with_trailer.push(0xAB);
        assert_eq!(parse(&with_trailer), tree);
    }

    #[test]
    fn enlarge_wraps_tree_and_stays_parseable() {
        let tree = vec![
            BM_BRANCH, BM_SOLID, 1, BM_SOLID, 2, BM_SOLID, 3, BM_SOLID, 4,
        ];
        let big = enlarged(&tree);
        assert_eq!(big[0], BM_BRANCH);
        assert_eq!(&big[1..1 + tree.len()], &tree[..]);
        assert_eq!(parse(&big), big);
        assert_eq!(enlarged(&[BM_SOLID, 0xFF]), vec![BM_SOLID, 0xFF]);
    }
}
