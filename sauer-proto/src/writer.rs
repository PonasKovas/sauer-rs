use std::marker::PhantomData;

use crate::internal::{Encode, constants::*};
use crate::{internal::WriterCore, protocol_route::ProtocolRoute};

/// The Sink-based Serializer
pub struct Writer<R: ProtocolRoute> {
    main: WriterCore,
    wrapper: WriterCore,
    current_wrapper_client: Option<i32>,
    _phantom: PhantomData<R>,
}

impl<R: ProtocolRoute> Writer<R> {
    pub fn new() -> Self {
        Self {
            main: WriterCore::new(),
            wrapper: WriterCore::new(),
            current_wrapper_client: None,
            _phantom: PhantomData,
        }
    }

    fn flush_wrapper(&mut self) {
        if let Some(cn) = self.current_wrapper_client
            && !self.wrapper.buffer.is_empty()
        {
            self.main.put_int(N_CLIENT);
            self.main.put_int(cn);
            self.main.put_uint(self.wrapper.buffer.len() as u32);
            self.main.put_bytes(&self.wrapper.buffer);
            self.wrapper.buffer.clear();
        }
        self.current_wrapper_client = None;
    }

    pub fn push(&mut self, msg: &R::Message) {
        let req_client = msg.requires_client_wrapper();

        // If the envelope requirement changed, flush the existing wrapper
        if req_client != self.current_wrapper_client {
            self.flush_wrapper();
            self.current_wrapper_client = req_client;
        }

        // Write to the appropriate buffer
        if self.current_wrapper_client.is_some() {
            msg.encode(&mut self.wrapper);
        } else {
            msg.encode(&mut self.main);
        }
    }

    pub fn consume(mut self) -> Vec<u8> {
        self.flush_wrapper();
        self.main.buffer
    }
}

impl<R: ProtocolRoute> Default for Writer<R> {
    fn default() -> Self {
        Self::new()
    }
}
