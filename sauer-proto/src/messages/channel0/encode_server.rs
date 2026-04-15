use crate::{
    Chan0Server,
    internal::{Encode, WriterCore, constants::*},
};

impl Encode for Chan0Server {
    fn encode(&self, w: &mut WriterCore) {
        match self {
            Chan0Server::DemoPacket => w.put_int(N_DEMOPACKET),
            Chan0Server::Position(v) => {
                w.put_int(N_POS);
                v.pos.encode(w);
            }
            Chan0Server::Teleport(v) => {
                w.put_int(N_TELEPORT);
                w.put_int(v.client_num);
                w.put_int(v.tp);
                w.put_int(v.td);
            }
            Chan0Server::JumpPad(v) => {
                w.put_int(N_JUMPPAD);
                w.put_int(v.client_num);
                w.put_int(v.jp);
            }
        }
    }
}
