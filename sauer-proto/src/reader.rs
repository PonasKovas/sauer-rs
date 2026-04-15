use crate::internal::constants::*;
use crate::Decode;
use crate::{internal::ReaderCore, ParseError};
use std::marker::PhantomData;

/// Deserializer bound to a specific protocol route.
///
/// The type parameter `R` determines which route's packets can be read.
///
/// - Use [`read`](Reader::read) to decode a *specific* packet struct
///   (returns an error if the wire tag doesn't match).
/// - Use [`next`](Reader::next) to decode the next message as any type
///   that implements `Decode<R>` (typically the channel enum, which
///   handles every valid tag for that route).
pub struct Reader<'a, R> {
    pub(crate) core: ReaderCore<'a>,
    _phantom: PhantomData<R>,
}

impl<'a, R> Reader<'a, R> {
    pub fn new(buffer: &'a [u8]) -> Self {
        Self {
            core: ReaderCore::new(buffer),
            _phantom: PhantomData,
        }
    }

    /// Returns `true` when there are no more bytes to read.
    pub fn is_empty(&self) -> bool {
        self.core.remaining() == 0
    }

    /// Read the next message type tag, transparently unwrapping any
    /// `N_CLIENT` envelope.  Returns `None` at end-of-buffer.
    fn read_msg_type(&mut self) -> Option<Result<i32, ParseError>> {
        if self.core.remaining() == 0 {
            return None;
        }

        let msg_type = match self.core.get_int() {
            Ok(t) => t,
            Err(e) => return Some(Err(e)),
        };

        // Transparently handle N_CLIENT envelopes
        if msg_type == N_CLIENT {
            let client_num = match self.core.get_int() {
                Ok(cn) => cn,
                Err(e) => return Some(Err(e)),
            };
            let length = match self.core.get_uint() {
                Ok(l) => l as usize,
                Err(e) => return Some(Err(e)),
            };

            self.core.set_client_num(client_num, length);

            // Recurse to read the actual enclosed message type
            return self.read_msg_type();
        }

        Some(Ok(msg_type))
    }

    /// Read and decode a specific packet type from the buffer.
    ///
    /// This is useful when you know exactly which packet to expect next
    /// (e.g. during a handshake sequence).  Returns
    /// [`ParseError::WrongMessageType`] if the next message has a
    /// different wire tag, or a decode error if the payload is malformed.
    pub fn read<T: Decode<R>>(&mut self) -> Result<T, ParseError> {
        match self.read_msg_type() {
            None => Err(ParseError::UnexpectedEof { expected: 1 }),
            Some(Err(e)) => Err(e),
            Some(Ok(msg_type)) => T::decode(msg_type, &mut self.core),
        }
    }

    /// Decode the next message as `T`, returning `None` at end-of-buffer.
    ///
    /// Typically `T` is the channel's full enum (e.g. `Chan1Server`) so
    /// that any valid message for the route can be decoded. This is the
    /// primary method for iterating over all messages in a packet:
    ///
    /// ```ignore
    /// let mut reader = Reader::<S2C<Chan1>>::new(raw);
    /// while let Some(result) = reader.next::<Chan1Server>() {
    ///     match result? { /* ... */ }
    /// }
    /// ```
    pub fn next<T: Decode<R>>(&mut self) -> Option<Result<T, ParseError>> {
        match self.read_msg_type() {
            None => None,
            Some(Err(e)) => Some(Err(e)),
            Some(Ok(msg_type)) => Some(T::decode(msg_type, &mut self.core)),
        }
    }
}
