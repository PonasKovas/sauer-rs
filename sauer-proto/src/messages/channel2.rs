pub use super::DemoPacket;
use crate::internal::macros::{make_packet_enum, make_struct};
use crate::internal::{
    constants::*,
    primitives::{read_all_remaining, write_all_remaining},
};
use crate::route::{Chan2, S2C};

make_packet_enum!(
/// Server -> Client (Channel 2)
[S2C<Chan2>] Chan2Server {
    /// `N_DEMOPACKET` Demo playback marker.
    [N_DEMOPACKET] DemoPacket,
    /// `N_SENDDEMO` Stream a recorded demo file to the client.
    [N_SENDDEMO] SendDemo,
    /// `N_SENDMAP` Stream a cooperative edit map file to the client.
    [N_SENDMAP] SendMap,
});

make_struct!(
/// `N_SENDDEMO` Sent over Channel 2 to stream a recorded demo file to the client.
[N_SENDDEMO] SendDemo {
    tag: i32,
    data: Vec<u8> => write_all_remaining, read_all_remaining,
});

make_struct!(
/// `N_SENDMAP` Sent over Channel 2 to stream a cooperative edit map file to the client.
[N_SENDMAP] SendMap {
    data: Vec<u8> => write_all_remaining, read_all_remaining,
});
