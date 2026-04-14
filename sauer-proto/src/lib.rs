//! `sauer-proto` — Sauerbraten network protocol codec.
//!
//! This crate provides typed, zero-copy serialisation and deserialisation for
//! the [Sauerbraten](http://sauerbraten.org/) (Cube 2) multiplayer network
//! protocol.  It covers all three ENet channels used by the game as well as the
//! out-of-band ExtInfo UDP protocol.
//!
//! # Channels
//!
//! | Constant               | Value | Purpose                              |
//! |------------------------|-------|--------------------------------------|
//! | [`CHANNEL_POSITIONS`]  | `0`   | High-frequency physics / movement    |
//! | [`CHANNEL_MESSAGES`]   | `1`   | Game state, events, chat (reliable)  |
//! | [`CHANNEL_MAPS`]       | `2`   | Bulk file transfers (demo, map)      |
//!
//! # Protocol Routes
//!
//! The [`protocol_route`] module exposes four concrete route types that tie a
//! channel and direction together:
//!
//! * [`protocol_route::S2C`]`<`[`protocol_route::Chan0`]`>` – server → client, channel 0
//! * [`protocol_route::C2S`]`<`[`protocol_route::Chan0`]`>` – client → server, channel 0
//! * [`protocol_route::S2C`]`<`[`protocol_route::Chan1`]`>` – server → client, channel 1
//! * [`protocol_route::C2S`]`<`[`protocol_route::Chan1`]`>` – client → server, channel 1
//! * [`protocol_route::S2C`]`<`[`protocol_route::Chan2`]`>` – server → client, channel 2
//!
//! # Decoding
//!
//! Use [`Reader`] as an iterator over messages in a raw ENet packet buffer:
//!
//! ```rust
//! use sauer_proto::{Reader, Chan1Server, protocol_route::{S2C, Chan1}};
//!
//! fn handle_channel1_packet(raw: &[u8]) {
//!     let reader = Reader::<S2C<Chan1>>::new(raw);
//!     for msg in reader {
//!         match msg.expect("parse error") {
//!             Chan1Server::ServMsg { text } => println!("server says: {text}"),
//!             Chan1Server::MapChange { map_name, .. } => println!("map: {map_name}"),
//!             _ => {}
//!         }
//!     }
//! }
//! ```
//!
//! # Encoding
//!
//! Use [`Writer`] to build an outgoing packet:
//!
//! ```rust
//! use sauer_proto::{Writer, Chan1Client, protocol_route::{C2S, Chan1}};
//! use cube_string::CubeString;
//!
//! fn make_chat_packet(text: &str) -> Vec<u8> {
//!     let mut writer = Writer::<C2S<Chan1>>::new();
//!     writer.push(&Chan1Client::Text {
//!         text: CubeString::from_str_lossy(text),
//!     });
//!     writer.consume()
//! }
//! ```

pub mod data;
mod error;
pub mod extinfo;
mod internal;
mod messages;
pub mod protocol_route;
mod reader;
mod writer;

pub use error::ParseError;
pub use messages::{channel0::*, channel1::*, channel2::*};
pub use reader::Reader;
pub use writer::Writer;

pub use cube_string;

/// Total number of ENet channels used by the Sauerbraten protocol.
pub const CHANNEL_COUNT: usize = 3;

/// ENet channel 0 — high-frequency physics and movement updates.
pub const CHANNEL_POSITIONS: u8 = 0;
/// ENet channel 1 — reliable game-state messages, chat, and events.
pub const CHANNEL_MESSAGES: u8 = 1;
/// ENet channel 2 — bulk file transfers (demo downloads, map sends).
pub const CHANNEL_MAPS: u8 = 2;

/// Wire protocol version number sent in the initial [`Chan1Server::ServInfo`] handshake.
pub const PROTOCOL_VERSION: i32 = 260;
