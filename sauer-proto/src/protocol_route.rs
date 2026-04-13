use std::marker::PhantomData;

use crate::{
    Chan0Client, Chan0Server, Chan1Client, Chan1Server, Chan2Server,
    internal::{Decode, Encode},
};

pub struct Chan0;
pub struct Chan1;
pub struct Chan2;

pub struct S2C<C>(PhantomData<C>);
pub struct C2S<C>(PhantomData<C>);

pub trait ProtocolRoute {
    #[allow(private_bounds)]
    type Message: Decode + Encode;
}

impl ProtocolRoute for S2C<Chan0> {
    type Message = Chan0Server;
}
impl ProtocolRoute for C2S<Chan0> {
    type Message = Chan0Client;
}
impl ProtocolRoute for S2C<Chan1> {
    type Message = Chan1Server;
}
impl ProtocolRoute for C2S<Chan1> {
    type Message = Chan1Client;
}
impl ProtocolRoute for S2C<Chan2> {
    type Message = Chan2Server;
}
