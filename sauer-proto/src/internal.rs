use crate::ParseError;

pub(crate) mod constants;
pub(crate) mod macros;
mod msg_id_to_name;
pub(crate) mod primitives;

pub(crate) use msg_id_to_name::msg_id_to_name;

pub(crate) trait SauerWrite {
    fn sauer_write(&self, out: &mut Vec<u8>);
}

pub(crate) trait SauerRead: Sized {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError>;
}

// convenience methods
pub(crate) fn read_slice<'a>(slice: &mut &'a [u8], n: usize) -> Result<&'a [u8], ParseError> {
    if slice.len() < n {
        return Err(ParseError::UnexpectedEof { expected: n });
    }

    let (lh, rh) = std::mem::take(slice).split_at(n);
    *slice = rh;

    Ok(lh)
}

pub(crate) fn check_tag(slice: &mut &[u8], tag: i32) -> Result<(), ParseError> {
    let t = i32::sauer_read(slice)?;

    if t != tag {
        return Err(crate::ParseError::WrongMessageType {
            expected: tag,
            expected_name: crate::internal::msg_id_to_name(tag),
            actual: t,
            actual_name: crate::internal::msg_id_to_name(t),
        });
    }

    Ok(())
}
