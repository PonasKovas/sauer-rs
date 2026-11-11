//! Route marker types for the Sauerbraten protocol.
//!
//! A **route** uniquely identifies a direction + channel combination.
//! The generic parameter on [`Reader`](crate::Reader) / [`Writer`](crate::Writer)
//! and on the public [`Encode`](crate::Encode) / [`Decode`](crate::Decode) traits
//! restricts operations to packets that are valid for the given route.
//!
//! | Route              | Meaning                                     |
//! |--------------------|---------------------------------------------|
//! | `S2C<Chan0>`       | Server -> Client, channel 0 (positions)     |
//! | `C2S<Chan0>`       | Client -> Server, channel 0 (positions)     |
//! | `S2C<Chan1>`       | Server -> Client, channel 1 (game messages) |
//! | `C2S<Chan1>`       | Client -> Server, channel 1 (game messages) |
//! | `S2C<Chan2>`       | Server -> Client, channel 2 (file xfer)     |

use std::marker::PhantomData;

/// Channel 0 — high-frequency physics / movement.
pub struct Chan0;
/// Channel 1 — reliable game-state messages, chat, events.
pub struct Chan1;
/// Channel 2 — bulk file transfers (demo, map).
pub struct Chan2;

/// Server-to-Client direction marker.
pub struct S2C<C>(PhantomData<C>);
/// Client-to-Server direction marker.
pub struct C2S<C>(PhantomData<C>);

/// An ENet channel, and its number.
pub trait Channel {
    const ID: u8;
}

impl Channel for Chan0 {
    const ID: u8 = crate::CHANNEL_POSITIONS;
}
impl Channel for Chan1 {
    const ID: u8 = crate::CHANNEL_MESSAGES;
}
impl Channel for Chan2 {
    const ID: u8 = crate::CHANNEL_MAPS;
}
