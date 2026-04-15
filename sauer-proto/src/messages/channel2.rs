use crate::ParseError;
use crate::error::ParseResultExt;
use crate::internal::macros::{make_packet_enum, make_struct};
use crate::internal::{SauerRead, SauerWrite, check_tag, constants::*};
use crate::route::{Chan2, S2C};

make_packet_enum!(
/// Server -> Client (Channel 0)
[S2C<Chan2>] Chan2Server {
    /// `N_DEMOPACKET` Demo playback marker.
    [N_DEMOPACKET] DemoPacket,
    /// `N_SENDDEMO` Stream a recorded demo file to the client.
    [N_SENDDEMO] SendDemo,
    /// `N_SENDMAP` Stream a cooperative edit map file to the client.
    [N_SENDMAP] SendMap,
});

make_struct!(
/// `N_DEMOPACKET` Demo playback marker.
[N_DEMOPACKET] DemoPacket {}
);

/// `N_SENDDEMO` Sent over Channel 2 to stream a recorded demo file to the client.
#[derive(Clone, Debug, PartialEq)]
pub struct SendDemo {
    pub tag: i32,
    pub data: Vec<u8>,
}

/// `N_SENDMAP` Sent over Channel 2 to stream a cooperative edit map file to the client.
#[derive(Clone, Debug, PartialEq)]
pub struct SendMap {
    pub data: Vec<u8>,
}

impl SauerWrite for SendDemo {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_SENDDEMO.sauer_write(out);
        self.tag.sauer_write(out);
        out.extend_from_slice(&self.data);
    }
}
impl SauerRead for SendDemo {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_SENDDEMO)?;

        Ok(Self {
            tag: i32::sauer_read(reader)
                .in_field("tag")
                .in_struct("N_SENDDEMO")?,
            data: reader.to_vec(),
        })
    }
}

impl SauerWrite for SendMap {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_SENDMAP.sauer_write(out);
        out.extend_from_slice(&self.data);
    }
}
impl SauerRead for SendMap {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_SENDMAP)?;

        Ok(Self {
            data: reader.to_vec(),
        })
    }
}
