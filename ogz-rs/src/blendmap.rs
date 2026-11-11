//! The blendmap is a quadtree over the map's XY plane (engine `blend.cpp`).

use crate::error::OgzError;
use byteorder::{ReadBytesExt, WriteBytesExt};
use std::io::{Read, Write};

const BM_BRANCH: u8 = 0;
const BM_SOLID: u8 = 1;
const BM_IMAGE: u8 = 2;
pub const IMAGE_SIZE: usize = 64;
const MAX_DEPTH: usize = 32;

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum BlendNode {
    /// Quadrants in x, then y order.
    Branch(Box<[BlendNode; 4]>),
    /// One blend value over the whole node.
    Solid(u8),
    /// A 64x64 grid of blend values, rows along x.
    Image(Box<[u8; IMAGE_SIZE * IMAGE_SIZE]>),
}

impl BlendNode {
    /// Mirrors the engine's `enlargeblendmap`: the old tree becomes the first
    /// quadrant of a new root, the other three are untouched (solid 0xFF).
    pub fn enlarged(self) -> Self {
        if let Self::Solid(_) = self {
            return self;
        }
        Self::Branch(Box::new([
            self,
            Self::Solid(0xFF),
            Self::Solid(0xFF),
            Self::Solid(0xFF),
        ]))
    }
}

pub(crate) fn read<R: Read>(r: &mut R, depth: usize) -> Result<BlendNode, OgzError> {
    if depth > MAX_DEPTH {
        return Err(OgzError::LimitExceeded("blendmap depth"));
    }
    match r.read_u8()? {
        BM_SOLID => Ok(BlendNode::Solid(r.read_u8()?)),
        BM_IMAGE => {
            let mut image = Box::new([0; IMAGE_SIZE * IMAGE_SIZE]);
            r.read_exact(&mut image[..])?;
            Ok(BlendNode::Image(image))
        }
        BM_BRANCH => Ok(BlendNode::Branch(Box::new([
            read(r, depth + 1)?,
            read(r, depth + 1)?,
            read(r, depth + 1)?,
            read(r, depth + 1)?,
        ]))),
        ty => Err(OgzError::InvalidBlendmapNode(ty)),
    }
}

pub(crate) fn write<W: Write>(w: &mut W, node: &BlendNode) -> Result<(), OgzError> {
    match node {
        BlendNode::Branch(children) => {
            w.write_u8(BM_BRANCH)?;
            for child in children.iter() {
                write(w, child)?;
            }
        }
        BlendNode::Solid(v) => {
            w.write_u8(BM_SOLID)?;
            w.write_u8(*v)?;
        }
        BlendNode::Image(image) => {
            w.write_u8(BM_IMAGE)?;
            w.write_all(&image[..])?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_whole_tree() {
        let mut tree = vec![BM_BRANCH, BM_SOLID, 7, BM_IMAGE];
        tree.extend(std::iter::repeat_n(3, IMAGE_SIZE * IMAGE_SIZE));
        tree.extend_from_slice(&[BM_SOLID, 0xFF, BM_SOLID, 0]);
        let mut with_trailer = tree.clone();
        with_trailer.push(0xAB);
        let node = read(&mut &with_trailer[..], 0).unwrap();
        let mut out = Vec::new();
        write(&mut out, &node).unwrap();
        assert_eq!(out, tree);
    }

    #[test]
    fn enlarge_wraps_tree() {
        let tree = BlendNode::Branch(Box::new([1, 2, 3, 4].map(BlendNode::Solid)));
        let BlendNode::Branch(big) = tree.clone().enlarged() else {
            panic!("not a branch");
        };
        assert_eq!(big[0], tree);
        assert_eq!(big[1], BlendNode::Solid(0xFF));
        assert_eq!(BlendNode::Solid(0xFF).enlarged(), BlendNode::Solid(0xFF));
    }
}
