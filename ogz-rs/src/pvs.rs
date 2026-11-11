//! Precomputed visibility (engine `pvs.cpp`): which parts of the map each
//! view cell can see, and an octree saying which view cell covers a spot.

use crate::error::OgzError;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{Read, Write};

/// The engine's `MAXWATERPVS`.
const MAX_WATER_PLANES: u32 = 32;
const MAX_BYTES: u32 = 256 * 1024 * 1024;
const MAX_DEPTH: usize = 32;

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Pvs {
    /// Heights of the water surfaces the PVS tracks separately.
    pub water_planes: Vec<i32>,
    /// What each view cell sees. A water bit field comes first in each.
    pub cells: Vec<Vec<u8>>,
    /// The root of the view cell octree.
    pub view_cells: Box<[ViewCell; 8]>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum ViewCell {
    /// An index into [`Pvs::cells`], or -1 for none.
    Leaf(i32),
    Branch(Box<[ViewCell; 8]>),
}

/// Reads the PVS of a map whose header says it has `num_cells` view cells.
pub(crate) fn read<R: Read>(r: &mut R, num_cells: usize) -> Result<Pvs, OgzError> {
    let mut total = r.read_u32::<LittleEndian>()?;
    let mut water_planes = Vec::new();
    if total & 0x8000_0000 != 0 {
        total &= !0x8000_0000;
        let n = r.read_u32::<LittleEndian>()?;
        if n > MAX_WATER_PLANES {
            return Err(OgzError::LimitExceeded("pvs water planes"));
        }
        for _ in 0..n {
            water_planes.push(r.read_i32::<LittleEndian>()?);
        }
    }
    if total > MAX_BYTES {
        return Err(OgzError::LimitExceeded("pvs size"));
    }
    let mut lens = Vec::with_capacity(num_cells);
    for _ in 0..num_cells {
        lens.push(r.read_u16::<LittleEndian>()?);
    }
    // The engine packs the cells back to back.
    if lens.iter().map(|&l| u32::from(l)).sum::<u32>() != total {
        return Err(OgzError::InvalidPvs);
    }
    let mut cells = Vec::with_capacity(num_cells);
    for len in lens {
        let mut cell = vec![0; len as usize];
        r.read_exact(&mut cell)?;
        cells.push(cell);
    }
    let view_cells = read_node(r, 0)?;
    Ok(Pvs {
        water_planes,
        cells,
        view_cells,
    })
}

fn read_node<R: Read>(r: &mut R, depth: usize) -> Result<Box<[ViewCell; 8]>, OgzError> {
    if depth > MAX_DEPTH {
        return Err(OgzError::LimitExceeded("pvs depth"));
    }
    let leaf_mask = r.read_u8()?;
    let mut children = Vec::with_capacity(8);
    for i in 0..8 {
        children.push(if leaf_mask & (1 << i) != 0 {
            ViewCell::Leaf(r.read_i32::<LittleEndian>()?)
        } else {
            ViewCell::Branch(read_node(r, depth + 1)?)
        });
    }
    Ok(Box::new(
        children.try_into().expect("8 elements pushed to 8-element array"),
    ))
}

pub(crate) fn write<W: Write>(w: &mut W, pvs: &Pvs) -> Result<(), OgzError> {
    let total: usize = pvs.cells.iter().map(Vec::len).sum();
    let mut header = u32::try_from(total)
        .ok()
        .filter(|&t| t < 0x8000_0000)
        .ok_or(OgzError::LimitExceeded("pvs size"))?;
    if !pvs.water_planes.is_empty() {
        header |= 0x8000_0000;
    }
    w.write_u32::<LittleEndian>(header)?;
    if !pvs.water_planes.is_empty() {
        w.write_u32::<LittleEndian>(pvs.water_planes.len() as u32)?;
        for &h in &pvs.water_planes {
            w.write_i32::<LittleEndian>(h)?;
        }
    }
    for cell in &pvs.cells {
        let len = u16::try_from(cell.len()).map_err(|_| OgzError::LimitExceeded("pvs cell"))?;
        w.write_u16::<LittleEndian>(len)?;
    }
    for cell in &pvs.cells {
        w.write_all(cell)?;
    }
    write_node(w, &pvs.view_cells)
}

fn write_node<W: Write>(w: &mut W, children: &[ViewCell; 8]) -> Result<(), OgzError> {
    let leaf_mask = children
        .iter()
        .enumerate()
        .filter(|(_, c)| matches!(c, ViewCell::Leaf(_)))
        .fold(0u8, |m, (i, _)| m | (1 << i));
    w.write_u8(leaf_mask)?;
    for child in children {
        match child {
            ViewCell::Leaf(i) => w.write_i32::<LittleEndian>(*i)?,
            ViewCell::Branch(c) => write_node(w, c)?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let mut inner: [ViewCell; 8] = std::array::from_fn(|i| ViewCell::Leaf(i as i32 % 2));
        inner[7] = ViewCell::Leaf(-1);
        let mut root: [ViewCell; 8] = std::array::from_fn(|_| ViewCell::Leaf(0));
        root[3] = ViewCell::Branch(Box::new(inner));
        let pvs = Pvs {
            water_planes: vec![512, 640],
            cells: vec![vec![1, 2, 3], vec![4]],
            view_cells: Box::new(root),
        };
        let mut bytes = Vec::new();
        write(&mut bytes, &pvs).unwrap();
        assert_eq!(read(&mut &bytes[..], 2).unwrap(), pvs);
    }
}
