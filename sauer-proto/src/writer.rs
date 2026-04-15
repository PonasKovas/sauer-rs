use crate::Encode;
use crate::internal::{WriterCore, constants::*};
use std::marker::PhantomData;

/// Sink-based serializer bound to a specific protocol route.
///
/// Push individual packet structs that implement [`Encode<R>`] and then call
/// [`consume`](Writer::consume) to get the final byte buffer.
///
/// The `N_CLIENT` envelope is handled transparently: if a packet's
/// [`requires_client_wrapper`](Encode::requires_client_wrapper) returns
/// `Some(cn)`, it will be wrapped automatically, and consecutive packets for
/// the same client are batched into a single envelope.
pub struct Writer<R> {
    main: WriterCore,
    wrapper: WriterCore,
    current_wrapper_client: Option<i32>,
    _phantom: PhantomData<R>,
}

impl<R> Writer<R> {
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

    /// Serialize a packet and append it to this writer's buffer.
    ///
    /// Accepts any type that implements `Encode<R>` for this writer's route,
    /// so you can push individual packet structs directly — no need to wrap
    /// them in the channel enum first.
    pub fn push<T: Encode<R>>(&mut self, msg: &T) {
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

    /// Finalize and return the raw byte buffer.
    pub fn consume(mut self) -> Vec<u8> {
        self.flush_wrapper();
        self.main.buffer
    }
}

impl<R> Default for Writer<R> {
    fn default() -> Self {
        Self::new()
    }
}
