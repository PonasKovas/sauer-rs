use crate::{
    Chan0Client,
    internal::{Encode, WriterCore, constants::*},
};

impl Encode for Chan0Client {
    fn encode(&self, w: &mut WriterCore) {
        match self {
            Chan0Client::Position { pos } => {
                w.put_int(N_POS);
                pos.encode(w);
            }
            Chan0Client::Teleport { client_num, tp, td } => {
                w.put_int(N_TELEPORT);
                w.put_int(*client_num);
                w.put_int(*tp);
                w.put_int(*td);
            }
            Chan0Client::JumpPad { client_num, jp } => {
                w.put_int(N_JUMPPAD);
                w.put_int(*client_num);
                w.put_int(*jp);
            }
        }
    }
}
