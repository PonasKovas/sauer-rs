use crate::internal::{constants::N_DEMOPACKET, macros::make_struct};

pub mod channel0;
pub mod channel1;
pub mod channel2;

make_struct!(
/// `N_DEMOPACKET` Demo playback marker. Any channel can carry it.
[N_DEMOPACKET] DemoPacket {}
);
