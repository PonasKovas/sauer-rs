//! `sauer-proto` -- Sauerbraten network protocol codec.
//!
//! This crate provides typed serialisation and deserialisation for the
//! [Sauerbraten](http://sauerbraten.org/) (Cube 2) multiplayer network
//! protocol.  It covers all three ENet channels used by the game as well as
//! the out-of-band ExtInfo UDP protocol.
//!
//! # Decoding
//!
//! Use [`Reader`] to read a raw ENet packet buffer:
//!
//! ```rust
//! use sauer_proto::{Session, Reader, Chan1Server, route::{S2C, Chan1}};
//!
//! fn handle_channel1_packet(raw: &[u8]) {
//!     let mut session = Session::new();
//!     let mut reader = Reader::<S2C<Chan1>>::new(raw, &mut session);
//!     while let Some(msg) = reader.next() {
//!         match msg.expect("parse error") {
//!             Chan1Server::ServMsg(servmsg) => println!("server says: {}", servmsg.text),
//!             Chan1Server::MapChange(map_change) => println!("map: {}", map_change.map_name),
//!             _ => {}
//!         }
//!     }
//! }
//! ```
//!
//! # Encoding
//!
//! Use [`Writer`] to build an outgoing packet.  You can push individual
//! packet structs directly -- no need to wrap in the channel enum:
//!
//! ```rust
//! use sauer_proto::{SendMode, Writer, route::{C2S, Chan1}};
//! use sauer_proto::channel1::Text;
//! use cube_string::CubeString;
//!
//! fn make_chat_packet(text: &str) -> (Vec<u8>, SendMode) {
//!     let mut writer = Writer::<C2S<Chan1>>::new();
//!     writer.push(&Text {
//!         text: CubeString::from_str_lossy(text),
//!     });
//!     writer.consume()
//! }
//! ```

mod error;
mod internal;
mod messages;
mod reader;
mod session;
mod writer;

pub mod data;
pub mod route;
pub mod server_query;

pub use error::ParseError;
pub use messages::{
    channel0::{self, Chan0Client, Chan0Server},
    channel1::{self, Chan1Client, Chan1Server, Client},
    channel2::{self, Chan2Server},
};
pub use reader::Reader;
pub use session::Session;
pub use writer::{PacketDescriptor, PacketSequence, SendMode, Writer};

pub use cube_string;

/// Total number of ENet channels used by the Sauerbraten protocol.
pub const CHANNEL_COUNT: usize = 3;

/// ENet channel 0 -- high-frequency physics and movement updates.
pub const CHANNEL_POSITIONS: u8 = 0;
/// ENet channel 1 -- reliable game-state messages, chat, and events.
pub const CHANNEL_MESSAGES: u8 = 1;
/// ENet channel 2 -- bulk file transfers (demo downloads, map sends).
pub const CHANNEL_MAPS: u8 = 2;

/// Wire protocol version number sent in the initial [`Chan1Server::ServInfo`] handshake.
pub const PROTOCOL_VERSION: i32 = 260;

/// Serialize a packet onto a wire-format buffer for route `R`.
///
/// Do not use this trait directly, and instead use the [`Writer`].
pub trait Encode<R> {
    #[doc(hidden)]
    fn encode(&self, out: &mut Vec<u8>);
    /// If this packet must be wrapped in an `N_CLIENT` envelope on the wire,
    /// return `Some(client_num)`.  The [`Writer`] uses this to
    /// transparently batch client-wrapped messages.
    #[doc(hidden)]
    fn requires_client_wrapper(&self) -> Option<i32> {
        None
    }

    #[doc(hidden)]
    fn is_reliable(&self) -> bool;
}

/// Deserialize a packet from wire-format bytes for route `R`.
///
/// Do not use this trait directly, and instead use the [`Reader`].
pub trait Decode<R>: Sized {
    #[doc(hidden)]
    fn decode(
        reader: &mut &[u8],
        client_number: Option<i32>,
        session: &mut Session,
    ) -> Result<Self, ParseError>;
}
