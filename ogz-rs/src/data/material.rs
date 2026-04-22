use crate::data::{Clip, Volume};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Material {
    pub volume: Volume,
    pub clip: Clip,
    pub death: bool,
    pub alpha: bool,
}

impl Material {
    pub fn from_bits(bits: u16) -> Self {
        Self {
            volume: Volume::from((bits as u8) & 0b11), // Fixed offset
            clip: Clip::from((bits >> 5) as u8 & 0b11),
            death: (bits & (1 << 8)) != 0,
            alpha: (bits & (1 << 10)) != 0,
        }
    }

    pub fn to_bits(self) -> u16 {
        (self.volume as u16) // Fixed offset
            | ((self.clip as u16) << 5)
            | ((self.death as u16) << 8)
            | ((self.alpha as u16) << 10)
    }
}

impl From<u16> for Material {
    fn from(bits: u16) -> Self {
        Self::from_bits(bits)
    }
}

impl From<Material> for u16 {
    fn from(mat: Material) -> Self {
        mat.to_bits()
    }
}

impl fmt::Display for Material {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.volume)?;

        if self.clip != Clip::None {
            write!(f, "|{:?}", self.clip)?;
        }

        if self.death {
            write!(f, "|Death")?;
        }

        if self.alpha {
            write!(f, "|Alpha")?;
        }

        Ok(())
    }
}
