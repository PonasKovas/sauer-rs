use crate::{
    Chan0Client,
    internal::{Encode, WriterCore, constants::*},
};

impl Encode for Chan0Client {
    fn encode(&self, w: &mut WriterCore) {
        match self {
            Chan0Client::Position(v) => {
                w.put_int(N_POS);
                v.pos.encode(w);
            }
            Chan0Client::Teleport(v) => {
                w.put_int(N_TELEPORT);
                w.put_int(v.client_num);
                w.put_int(v.tp);
                w.put_int(v.td);
            }
            Chan0Client::JumpPad(v) => {
                w.put_int(N_JUMPPAD);
                w.put_int(v.client_num);
                w.put_int(v.jp);
            }
        }
    }
}
