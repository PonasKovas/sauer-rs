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
    is_reliable: bool,
    _phantom: PhantomData<R>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SendMode {
    Reliable,
    Unreliable,
}

impl<R> Writer<R> {
    pub fn new() -> Self {
        Self {
            main: Vec::new(),
            wrapper: Vec::new(),
            current_wrapper_client: None,
            is_reliable: true,
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
        // If ANY message pushed to this writer is reliable, the whole packet becomes reliable
        if msg.is_reliable() {
            self.is_reliable = true;
        }

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
    pub fn consume(mut self) -> (Vec<u8>, SendMode) {
        self.flush_wrapper();

        let mode = if self.is_reliable {
            SendMode::Reliable
        } else {
            SendMode::Unreliable // (Remember, this is sequenced!)
        };

        (self.main, mode)
    }
}

impl<R> Default for Writer<R> {
    fn default() -> Self {
        Self::new()
    }
}

pub trait PacketSequence<R> {
    fn push_to(&self, writer: &mut Writer<R>);
}

/// Returns the human-readable names of all packets in a sequence (e.g. `"N_TEXT"`).
///
/// Implemented for all types that also implement [`PacketSequence`]. Used by
/// the server's packet-level trace logging to identify packets before they are
/// serialised into raw bytes.
pub trait PacketDescriptor {
    fn packet_names(&self) -> Vec<&'static str>;
}

impl<T: PacketDescriptor + ?Sized> PacketDescriptor for &T {
    fn packet_names(&self) -> Vec<&'static str> {
        (*self).packet_names()
    }
}

impl<T: PacketDescriptor> PacketDescriptor for [T] {
    fn packet_names(&self) -> Vec<&'static str> {
        self.iter().flat_map(|t| t.packet_names()).collect()
    }
}

impl<T: PacketDescriptor, const N: usize> PacketDescriptor for [T; N] {
    fn packet_names(&self) -> Vec<&'static str> {
        self.iter().flat_map(|t| t.packet_names()).collect()
    }
}

impl<T: PacketDescriptor> PacketDescriptor for Vec<T> {
    fn packet_names(&self) -> Vec<&'static str> {
        self.iter().flat_map(|t| t.packet_names()).collect()
    }
}

macro_rules! impl_tuple_descriptor {
    ( $($name:ident)+ ) => {
        impl<$($name: PacketDescriptor),+> PacketDescriptor for ($($name,)+) {
            fn packet_names(&self) -> Vec<&'static str> {
                #[allow(non_snake_case)]
                let ($($name,)+) = self;
                let mut v = Vec::new();
                $(v.extend($name.packet_names());)+
                v
            }
        }
    };
}

impl_tuple_descriptor!(A);
impl_tuple_descriptor!(A B);
impl_tuple_descriptor!(A B C);
impl_tuple_descriptor!(A B C D);
impl_tuple_descriptor!(A B C D E);
impl_tuple_descriptor!(A B C D E F);
impl_tuple_descriptor!(A B C D E F G);
impl_tuple_descriptor!(A B C D E F G H);
impl_tuple_descriptor!(A B C D E F G H I);
impl_tuple_descriptor!(A B C D E F G H I J);
impl_tuple_descriptor!(A B C D E F G H I J K);
impl_tuple_descriptor!(A B C D E F G H I J K L);
impl_tuple_descriptor!(A B C D E F G H I J K L M);
impl_tuple_descriptor!(A B C D E F G H I J K L M N);
impl_tuple_descriptor!(A B C D E F G H I J K L M N O);
impl_tuple_descriptor!(A B C D E F G H I J K L M N O P);

// Blanket implementation for references allows us to preserve the old &T API seamlessly
impl<R, T: PacketSequence<R> + ?Sized> PacketSequence<R> for &T {
    fn push_to(&self, writer: &mut Writer<R>) {
        (*self).push_to(writer);
    }
}

// Support for homogenously typed slices
impl<R, T: PacketSequence<R>> PacketSequence<R> for [T] {
    fn push_to(&self, writer: &mut Writer<R>) {
        for item in self {
            item.push_to(writer);
        }
    }
}

// Support for arrays
impl<R, T: PacketSequence<R>, const N: usize> PacketSequence<R> for [T; N] {
    fn push_to(&self, writer: &mut Writer<R>) {
        for item in self {
            item.push_to(writer);
        }
    }
}

// Support for standard Vecs
impl<R, T: PacketSequence<R>> PacketSequence<R> for Vec<T> {
    fn push_to(&self, writer: &mut Writer<R>) {
        for item in self {
            item.push_to(writer);
        }
    }
}

// Macro to implement PacketSequence for heterogenous Tuples
macro_rules! impl_tuple_seq {
    ( $($name:ident)+ ) => {
        impl<R, $($name: PacketSequence<R>),+> PacketSequence<R> for ($($name,)+) {
            fn push_to(&self, writer: &mut Writer<R>) {
                #[allow(non_snake_case)]
                let ($($name,)+) = self;
                $($name.push_to(writer);)+
            }
        }
    };
}

impl_tuple_seq!(A);
impl_tuple_seq!(A B);
impl_tuple_seq!(A B C);
impl_tuple_seq!(A B C D);
impl_tuple_seq!(A B C D E);
impl_tuple_seq!(A B C D E F);
impl_tuple_seq!(A B C D E F G);
impl_tuple_seq!(A B C D E F G H);
impl_tuple_seq!(A B C D E F G H I);
impl_tuple_seq!(A B C D E F G H I J);
impl_tuple_seq!(A B C D E F G H I J K);
impl_tuple_seq!(A B C D E F G H I J K L);
impl_tuple_seq!(A B C D E F G H I J K L M);
impl_tuple_seq!(A B C D E F G H I J K L M N);
impl_tuple_seq!(A B C D E F G H I J K L M N O);
impl_tuple_seq!(A B C D E F G H I J K L M N O P);
