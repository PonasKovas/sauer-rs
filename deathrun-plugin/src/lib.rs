mod map;
mod recreate;

use crate::map::{MapState, get_map_state};
use bytes::Bytes;
use cube_string::CubeString;
use sauer_proto::{
    Client,
    channel1::{self, EditMode, ServMsg},
    channel2,
};
use sauer_server::{
    Server,
    plugin::{Context, Plugin},
};
use std::sync::{Arc, RwLock};
use tracing::{error, info, warn};

pub struct DeathRun {}

impl DeathRun {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for DeathRun {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to reduce boilerplate when mutating MapState
fn with_map_state_mut<F>(ctx: &Context, f: F)
where
    F: FnOnce(&mut MapState),
{
    if let Some(state) = get_map_state(ctx.server()).write().unwrap().as_mut() {
        f(state);
    }
}

fn send_map_to(server: &Server, cn: u32) -> bool {
    let bytes = {
        let lock = get_map_state(server).read().unwrap();
        match lock.as_ref() {
            None => return false,
            Some(state) => match state.to_ogz_bytes() {
                Ok(b) => b,
                Err(e) => {
                    error!("failed to serialize map state: {e}");
                    return false;
                }
            },
        }
    };
    server.send_chan2(cn, &channel2::SendMap { data: bytes });
    true
}

impl Plugin for DeathRun {
    async fn init(&self, server: &mut Server) {
        server.extra.insert(RwLock::new(None::<MapState>));
    }

    async fn text(&self, ctx: &mut Context, packet: &channel1::Text) {
        let text = packet.text.to_utf8();
        let parts: Vec<&str> = text.trim().split_whitespace().collect();
        if parts.is_empty() {
            return;
        }

        if parts[0] == "#recreate" {
            let mut divisions_power = 3;
            let mut delay_ms = 1;

            if parts.len() > 1 {
                if let Ok(d) = parts[1].parse::<u32>() {
                    divisions_power = d.clamp(1, 8);
                }
            }
            if parts.len() > 2 {
                if let Ok(t) = parts[2].parse::<u64>() {
                    delay_ms = t;
                }
            }

            let divisions = 1 << divisions_power;
            crate::recreate::spawn_recreate_map(
                Arc::clone(ctx.server()),
                ctx.cn(),
                divisions,
                delay_ms,
            );
        }
    }

    async fn edit_mode(&self, ctx: &mut Context, _packet: &channel1::EditMode) {
        if get_map_state(ctx.server()).read().unwrap().is_none() {
            ctx.server().send_chan1(
                ctx.cn(),
                &Client {
                    client_num: ctx.cn() as i32,
                    packet: EditMode { is_editing: false },
                },
            );
            ctx.server().send_chan1(
                ctx.cn(),
                &ServMsg {
                    text: CubeString::builder()
                        .color(cube_string::Color::Yellow)
                        .push_str("Server doesnt have the map yet. /sendmap before editing")
                        .build(),
                },
            );
            ctx.cancel();
        }
    }

    async fn map(&self, ctx: &mut Context, data: &Bytes) {
        ctx.cancel();
        let cn = ctx.cn();
        let server = ctx.server();

        match MapState::from_bytes(data) {
            Ok(state) => {
                let world_size = state.world.size;
                state.update_server_items(server);
                *get_map_state(server).write().unwrap() = Some(state);
                info!("cn {cn} uploaded a map (world_size={world_size}); state loaded");
            }
            Err(e) => warn!("cn {cn} sent an invalid map: {e}"),
        }
    }

    async fn connect(&self, ctx: &mut Context, _packet: &channel1::Connect) {
        let cn = ctx.cn();
        if send_map_to(ctx.server(), cn) {
            info!("sent map state to cn {cn} on connect");
        }
    }

    async fn get_map(&self, ctx: &mut Context, _packet: &channel1::GetMap) {
        let cn = ctx.cn();
        if send_map_to(ctx.server(), cn) {
            info!("sent map state to cn {cn} on N_GETMAP");
            ctx.cancel();
        }
    }

    // ── Coop-edit tracking ────────────────────────────────────────────────────

    async fn edit_var(&self, ctx: &mut Context, packet: &channel1::EditVar) {
        with_map_state_mut(ctx, |state| state.handle_edit_var(packet));
    }

    async fn edit_tex(&self, ctx: &mut Context, packet: &channel1::EditTex) {
        with_map_state_mut(ctx, |state| state.handle_edit_tex(packet));
    }

    async fn edit_mat(&self, ctx: &mut Context, packet: &channel1::EditMat) {
        with_map_state_mut(ctx, |state| state.handle_edit_mat(packet));
    }

    async fn del_cube(&self, ctx: &mut Context, packet: &channel1::DelCube) {
        with_map_state_mut(ctx, |state| state.handle_del_cube(packet));
    }

    async fn replace(&self, ctx: &mut Context, packet: &channel1::Replace) {
        with_map_state_mut(ctx, |state| state.handle_replace(packet));
    }

    async fn edit_face(&self, ctx: &mut Context, packet: &channel1::EditFace) {
        with_map_state_mut(ctx, |state| state.handle_edit_face(packet));
    }

    async fn edit_ent(&self, ctx: &mut Context, packet: &channel1::EditEnt) {
        with_map_state_mut(ctx, |state| state.handle_edit_ent(packet));
    }
}
