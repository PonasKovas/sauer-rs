use crate::{
    Chan0Server,
    internal::{Encode, WriterCore, constants::*},
};

impl Encode for Chan0Server {
    fn encode(&self, w: &mut WriterCore) {
        match self {
            Chan0Server::DemoPacket => w.put_int(N_DEMOPACKET),
            Chan0Server::Position { pos } => {
                w.put_int(N_POS);
                pos.encode(w);
            }
            Chan0Server::Teleport { client_num, tp, td } => {
                w.put_int(N_TELEPORT);
                w.put_int(*client_num);
                w.put_int(*tp);
                w.put_int(*td);
            }
            Chan0Server::JumpPad { client_num, jp } => {
                w.put_int(N_JUMPPAD);
                w.put_int(*client_num);
                w.put_int(*jp);
            }
        }
    }
}
