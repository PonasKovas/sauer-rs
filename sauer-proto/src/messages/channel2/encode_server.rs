use crate::{
    Chan2Server,
    internal::{Encode, WriterCore, constants::*},
};

impl Encode for Chan2Server {
    fn encode(&self, w: &mut WriterCore) {
        match self {
            Chan2Server::DemoPacket => w.put_int(N_DEMOPACKET),
            Chan2Server::SendDemo { tag, data } => {
                w.put_int(N_SENDDEMO);
                w.put_int(*tag);
                w.put_bytes(data); // Raw push of bytes
            }
            Chan2Server::SendMap { data } => {
                w.put_int(N_SENDMAP);
                w.put_bytes(data);
            }
        }
    }
}
