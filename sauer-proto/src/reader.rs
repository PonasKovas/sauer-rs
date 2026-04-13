use crate::internal::Decode;
use crate::internal::constants::*;
use crate::{ParseError, internal::ReaderCore, protocol_route::ProtocolRoute};
use std::marker::PhantomData;

/// The Iterator-based Deserializer
pub struct Reader<'a, R: ProtocolRoute> {
    pub(crate) core: ReaderCore<'a>,
    _phantom: PhantomData<R>,
}

impl<'a, R: ProtocolRoute> Reader<'a, R> {
    pub fn new(buffer: &'a [u8]) -> Self {
        Self {
            core: ReaderCore::new(buffer),
            _phantom: PhantomData,
        }
    }
}

impl<'a, R: ProtocolRoute> Iterator for Reader<'a, R> {
    type Item = Result<R::Message, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
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

            // Recurse to read the actual enclosed message
            return self.next();
        }

        Some(R::Message::decode(msg_type, &mut self.core))
    }
}
