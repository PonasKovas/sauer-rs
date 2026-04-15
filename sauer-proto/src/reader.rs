use crate::Decode;
use crate::ParseError;
use crate::error::ParseResultExt;
use crate::internal::SauerRead;
use crate::internal::constants::*;
use std::marker::PhantomData;

/// Deserializer bound to a specific protocol route.
///
/// The type parameter `R` determines which route's packets can be read.
///
/// - Use [`next`](Reader::next) to decode the next message as any type
///   that implements `Decode<R>` (typically the channel enum, which
///   handles every valid tag for that route).
pub struct Reader<'a, R> {
    pub(crate) slice: &'a [u8],
    active_wrapper: Option<ClientWrapperState>,
    _phantom: PhantomData<R>,
}

/// Contains the active state of an N_CLIENT wrapper during deserialization
#[derive(Copy, Clone)]
struct ClientWrapperState {
    client_num: i32,
    end_addr: usize, // absolute memory address, since we reduce the slice as we read it
}

impl<'a, R> Reader<'a, R> {
    pub fn new(slice: &'a [u8]) -> Self {
        Self {
            slice,
            active_wrapper: None,
            _phantom: PhantomData,
        }
    }

    /// Returns `true` when there are no more bytes to read.
    pub fn is_empty(&self) -> bool {
        self.slice.len() == 0
    }

    fn read_message<T: Decode<R>>(&mut self) -> Result<T, ParseError> {
        // remove N_CLIENT wrapper if expired
        if let Some(wrapper) = self.active_wrapper {
            let addr = self.slice.as_ptr() as usize;
            if addr >= wrapper.end_addr {
                self.active_wrapper = None;
            }
        }

        // peek the message tag
        let mut temp_slice = self.slice;
        let msg_type = i32::sauer_read(&mut temp_slice)?;

        if msg_type == N_CLIENT {
            if self.active_wrapper.is_some() {
                return Err(ParseError::DoubleClientWrapper);
            }

            let cn = i32::sauer_read(&mut temp_slice)
                .in_field("client_num")
                .in_struct("N_CLIENT")?;
            let len = u32::sauer_read(&mut temp_slice)
                .in_field("length")
                .in_struct("N_CLIENT")?;

            // Only allow the envelope to extend up to the end of our current slice
            let clamped_len = (len as usize).min(temp_slice.len());
            let end_addr = temp_slice.as_ptr() as usize + clamped_len as usize;

            self.active_wrapper = Some(ClientWrapperState {
                client_num: cn,
                end_addr,
            });

            self.slice = temp_slice;

            return self.read_message();
        }

        let msg = T::decode(&mut self.slice, self.active_wrapper.map(|w| w.client_num))?;

        Ok(msg)
    }

    /// Decode the next message as `T`, returning `None` at end-of-buffer.
    pub fn next<T: Decode<R>>(&mut self) -> Option<Result<T, ParseError>> {
        if self.slice.is_empty() {
            return None;
        }

        Some(self.read_message())
    }
}
