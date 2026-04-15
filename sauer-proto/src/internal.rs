use crate::ParseError;

pub(crate) mod constants;
mod primitives;
mod reader_core;
mod writer_core;

pub(crate) trait SauerWrite {
    fn sauer_write(&self, out: &mut Vec<u8>);
}

pub(crate) trait SauerRead: Sized {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError>;
}

// convenience method
pub(crate) fn read_slice<'a>(slice: &mut &'a [u8], n: usize) -> Result<&'a [u8], ParseError> {
    if slice.len() < n {
        return Err(ParseError::UnexpectedEof { expected: n });
    }

    let (lh, rh) = std::mem::take(slice).split_at(n);
    *slice = rh;

    Ok(lh)
}
