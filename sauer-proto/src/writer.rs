use crate::Encode;
use crate::internal::{SauerWrite, constants::*};
use std::marker::PhantomData;

/// Sink-based serializer bound to a specific protocol route.
///
/// Push individual packet structs that implement [`Encode<R>`] and then call
/// [`consume`](Writer::consume) to get the final byte buffer.
///
/// The `N_CLIENT` envelope is handled transparently.
pub struct Writer<R> {
    main: Vec<u8>,
    wrapper: Vec<u8>,
    current_wrapper_client: Option<i32>,
    _phantom: PhantomData<R>,
}

impl<R> Writer<R> {
    pub fn new() -> Self {
        Self {
            main: Vec::new(),
            wrapper: Vec::new(),
            current_wrapper_client: None,
            _phantom: PhantomData,
        }
    }

    fn flush_wrapper(&mut self) {
        if let Some(cn) = self.current_wrapper_client
            && !self.wrapper.is_empty()
        {
            N_CLIENT.sauer_write(&mut self.main);
            cn.sauer_write(&mut self.main);
            let len = self.wrapper.len() as u32;
            len.sauer_write(&mut self.main);
            self.main.extend_from_slice(&self.wrapper);
            self.wrapper.clear();
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
        self.main
    }
}

impl<R> Default for Writer<R> {
    fn default() -> Self {
        Self::new()
    }
}
