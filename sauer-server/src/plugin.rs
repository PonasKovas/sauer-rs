use crate::Server;
use sauer_proto::{channel0, channel1, channel2};
use std::{ops::ControlFlow, sync::Arc};

// if only Try was stable
pub type Control = ControlFlow<()>;
pub const PASS: Control = ControlFlow::Continue(());
pub const BLOCK: Control = ControlFlow::Break(());

pub trait Plugin: Send + Sync + 'static {
    #![allow(unused)]

    fn init(&self, server: &Arc<Server>) -> impl Future<Output = ()> + Send + Sync {
        async move { () }
    }

    /// BLOCKING will disconnect the peer
    fn connected(&self, ctx: Context<'_>) -> impl Future<Output = Control> + Send + Sync {
        async move { PASS }
    }
    fn disconnected(&self, ctx: Context<'_>) -> impl Future<Output = Control> + Send + Sync {
        async move { PASS }
    }
    fn n_pos(
        &self,
        ctx: Context<'_>,
        packet: &channel0::Position,
    ) -> impl Future<Output = Control> + Send + Sync {
        async move { PASS }
    }
}

#[derive(Copy, Clone)]
pub struct Context<'a> {
    pub server: &'a Arc<Server>,
    pub cn: u32,
}
impl<'a> Context<'a> {
    pub(crate) fn new(server: &'a Arc<Server>, cn: u32) -> Self {
        Self { server, cn }
    }
}

impl Plugin for () {}

impl<P1: Plugin, P2: Plugin> Plugin for (P1, P2) {
    async fn connected(&self, ctx: Context<'_>) -> Control {
        // run the left (previous) plugin
        self.0.connected(ctx).await?;
        self.1.connected(ctx).await
    }
    async fn disconnected(&self, ctx: Context<'_>) -> Control {
        // run the left (previous) plugin
        self.0.disconnected(ctx).await?;
        self.1.disconnected(ctx).await
    }
    async fn n_pos(&self, ctx: Context<'_>, packet: &channel0::Position) -> Control {
        // run the left (previous) plugin
        self.0.n_pos(ctx, packet).await?;
        self.1.n_pos(ctx, packet).await
    }
}
