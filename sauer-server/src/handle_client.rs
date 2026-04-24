use crate::{
    Server,
    plugin::{Context, Plugin},
};
use anyhow::{Result, bail};
use bytes::Bytes;
use enet_rs::PeerReceiver;
use sauer_proto::{
    Chan0Client, Chan1Client, Reader, Session,
    route::{C2S, Chan0, Chan1},
};
use std::{collections::VecDeque, sync::Arc};
use tracing::trace;

/// Dispatches one packet: calls the plugin hook first, then the base handler if not cancelled.
macro_rules! with_base {
    ($ctx:expr, $base:expr) => {
        if !$ctx.cancelled {
            $base;
        }
    };
}

pub(crate) async fn handle_client<P: Plugin>(
    server: &Arc<Server>,
    plugin: &Arc<P>,
    peer: PeerReceiver,
    cn: u32,
) -> Result<()> {
    let mut rx = SauerReceiver::new(peer);
    let mut ctx = Context {
        server: Arc::clone(server),
        cn,
        cancelled: false,
    };

    loop {
        let pkt = rx.recv().await?;

        if pkt.channel() == 1 {
            trace!("received: {pkt:?}");
        }

        ctx.cancelled = false;

        match pkt {
            Packet::Chan0(pkt) => match pkt {
                Chan0Client::Position(p) => {
                    plugin.pos(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_pos(cn, &p));
                }
                Chan0Client::Teleport(p) => {
                    plugin.teleport(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_teleport(cn, &p));
                }
                Chan0Client::JumpPad(p) => {
                    plugin.jumppad(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_jumppad(cn, &p));
                }
            },

            Packet::Chan1(pkt) => match pkt {
                Chan1Client::Connect(p) => {
                    plugin.connect(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_connect(cn, &p));
                }
                Chan1Client::TrySpawn(p) => {
                    plugin.try_spawn(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_try_spawn(cn));
                }
                Chan1Client::SpawnClient(p) => {
                    plugin.spawn_client(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_spawn_client(cn, &p));
                }
                Chan1Client::Suicide(p) => {
                    plugin.suicide(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_suicide(cn));
                }
                Chan1Client::Shoot(p) => {
                    plugin.shoot(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_shoot(cn, &p));
                }
                Chan1Client::Explode(p) => {
                    plugin.explode(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_explode(cn, &p));
                }
                Chan1Client::SpectatorClient(p) => {
                    plugin.spectator(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_spectator(cn, &p));
                }
                Chan1Client::Ping(p) => {
                    plugin.ping(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_ping(cn, &p));
                }
                Chan1Client::ClientPing(p) => {
                    plugin.client_ping(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_client_ping(cn, &p));
                }
                Chan1Client::Text(p) => {
                    plugin.text(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_text(cn, &p));
                }
                Chan1Client::SayTeamClient(p) => {
                    plugin.say_team(&mut ctx, &p).await;
                    // TODO: team chat relay
                }
                Chan1Client::Taunt(p) => {
                    plugin.taunt(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_taunt(cn, &p));
                }
                Chan1Client::Sound(p) => {
                    plugin.sound(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_sound(cn, &p));
                }
                Chan1Client::SwitchName(p) => {
                    plugin.switch_name(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_switch_name(cn, &p));
                }
                Chan1Client::SwitchModel(p) => {
                    plugin.switch_model(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_switch_model(cn, &p));
                }
                Chan1Client::SwitchTeam(p) => {
                    plugin.switch_team(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_switch_team(cn, &p));
                }
                Chan1Client::GetMap(p) => {
                    plugin.get_map(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_get_map(cn));
                }
                Chan1Client::MapVote(p) => {
                    plugin.map_vote(&mut ctx, &p).await;
                    // No default map-vote handler; plugins own this.
                }
                Chan1Client::ItemList(p) => {
                    plugin.item_list(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_item_list(cn, &p));
                }
                Chan1Client::ItemPickup(p) => {
                    plugin.item_pickup(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_item_pickup(cn, &p));
                }
                Chan1Client::NewMap(p) => {
                    plugin.new_map(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_new_map(cn, &p));
                }
                Chan1Client::MasterModeClient(p) => {
                    plugin.master_mode(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_master_mode(cn, &p));
                }

                // ── Coop-edit: relay to other clients in CoopEdit mode ─────────
                Chan1Client::EditMode(p) => {
                    plugin.edit_mode(&mut ctx, &p).await;
                    with_base!(ctx, server.relay_coop(cn, &p));
                }
                Chan1Client::EditFace(p) => {
                    plugin.edit_face(&mut ctx, &p).await;
                    with_base!(ctx, server.relay_coop(cn, &p));
                }
                Chan1Client::EditTex(p) => {
                    plugin.edit_tex(&mut ctx, &p).await;
                    with_base!(ctx, server.relay_coop(cn, &p));
                }
                Chan1Client::EditMat(p) => {
                    plugin.edit_mat(&mut ctx, &p).await;
                    with_base!(ctx, server.relay_coop(cn, &p));
                }
                Chan1Client::EditVSlot(p) => {
                    plugin.edit_v_slot(&mut ctx, &p).await;
                    with_base!(ctx, server.relay_coop(cn, &p));
                }
                Chan1Client::Flip(p) => {
                    plugin.flip(&mut ctx, &p).await;
                    with_base!(ctx, server.relay_coop(cn, &p));
                }
                Chan1Client::Rotate(p) => {
                    plugin.rotate(&mut ctx, &p).await;
                    with_base!(ctx, server.relay_coop(cn, &p));
                }
                Chan1Client::Replace(p) => {
                    plugin.replace(&mut ctx, &p).await;
                    with_base!(ctx, server.relay_coop(cn, &p));
                }
                Chan1Client::DelCube(p) => {
                    plugin.del_cube(&mut ctx, &p).await;
                    with_base!(ctx, server.relay_coop(cn, &p));
                }
                Chan1Client::Remip(p) => {
                    plugin.remip(&mut ctx, &p).await;
                    with_base!(ctx, server.relay_coop(cn, &p));
                }
                Chan1Client::EditVar(p) => {
                    plugin.edit_var(&mut ctx, &p).await;
                    with_base!(ctx, server.relay_coop(cn, &p));
                }
                Chan1Client::EditEnt(p) => {
                    plugin.edit_ent(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_edit_ent(cn, &p));
                }
                Chan1Client::UndoClient(p) => {
                    plugin.undo(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_undo(cn, &p));
                }
                Chan1Client::RedoClient(p) => {
                    plugin.redo(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_redo(cn, &p));
                }
                Chan1Client::CopyClient(p) => {
                    plugin.copy(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_copy(cn, &p));
                }
                Chan1Client::PasteClient(p) => {
                    plugin.paste(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_paste(cn, &p));
                }
                Chan1Client::ClipboardClient(p) => {
                    plugin.clipboard(&mut ctx, &p).await;
                    with_base!(ctx, server.handle_clipboard(cn, &p));
                }

                // ── Unimplemented / pass-through to plugins only ──────────────
                Chan1Client::AuthAnswer(p) => plugin.auth_answer(&mut ctx, &p).await,
                Chan1Client::FromAi(p) => plugin.from_ai(&mut ctx, &p).await,
                Chan1Client::MapCrc(p) => plugin.map_crc(&mut ctx, &p).await,
                Chan1Client::CheckMaps(p) => plugin.check_maps(&mut ctx, &p).await,
                Chan1Client::GunSelect(p) => plugin.gun_select(&mut ctx, &p).await,
                Chan1Client::ClearBans(p) => plugin.clear_bans(&mut ctx, &p).await,
                Chan1Client::Kick(p) => plugin.kick(&mut ctx, &p).await,
                Chan1Client::SetTeamClient(p) => plugin.set_team(&mut ctx, &p).await,
                Chan1Client::ForceIntermission(p) => plugin.force_intermission(&mut ctx, &p).await,
                Chan1Client::RecordDemo(p) => plugin.record_demo(&mut ctx, &p).await,
                Chan1Client::StopDemo(p) => plugin.stop_demo(&mut ctx, &p).await,
                Chan1Client::ClearDemos(p) => plugin.clear_demos(&mut ctx, &p).await,
                Chan1Client::ListDemos(p) => plugin.list_demos(&mut ctx, &p).await,
                Chan1Client::GetDemo(p) => plugin.get_demo(&mut ctx, &p).await,
                Chan1Client::SetMaster(p) => plugin.set_master(&mut ctx, &p).await,
                Chan1Client::AddBot(p) => plugin.add_bot(&mut ctx, &p).await,
                Chan1Client::DelBot(p) => plugin.del_bot(&mut ctx, &p).await,
                Chan1Client::BotLimit(p) => plugin.bot_limit(&mut ctx, &p).await,
                Chan1Client::BotBalance(p) => plugin.bot_balance(&mut ctx, &p).await,
                Chan1Client::AuthTry(p) => plugin.auth_try(&mut ctx, &p).await,
                Chan1Client::AuthKick(p) => plugin.auth_kick(&mut ctx, &p).await,
                Chan1Client::PauseGameClient(p) => plugin.pause_game(&mut ctx, &p).await,
                Chan1Client::GameSpeedClient(p) => plugin.game_speed(&mut ctx, &p).await,
                Chan1Client::ServCmd(p) => plugin.serv_cmd(&mut ctx, &p).await,
                Chan1Client::BasesClient(p) => plugin.bases(&mut ctx, &p).await,
                Chan1Client::RepAmmoClient(p) => plugin.rep_ammo(&mut ctx, &p).await,
                Chan1Client::TryDropFlag(p) => plugin.try_drop_flag(&mut ctx, &p).await,
                Chan1Client::TakeFlagClient(p) => plugin.take_flag(&mut ctx, &p).await,
                Chan1Client::InitFlagsClient(p) => plugin.init_flags(&mut ctx, &p).await,
                Chan1Client::InitTokensClient(p) => plugin.init_tokens(&mut ctx, &p).await,
                Chan1Client::TakeTokenClient(p) => plugin.take_token(&mut ctx, &p).await,
                Chan1Client::DepositTokensClient(p) => plugin.deposit_tokens(&mut ctx, &p).await,
            },

            Packet::Chan2(data) => {
                plugin.map(&mut ctx, &data).await;
                with_base!(ctx, server.handle_map_upload(cn, &data));
            }
        }
    }
}

// ── Packet buffering / receive loop ──────────────────────────────────────────

struct SauerReceiver {
    rx: PeerReceiver,
    session: Session,
    buffer: VecDeque<Packet>,
}

#[derive(Debug)]
enum Packet {
    Chan0(Chan0Client),
    Chan1(Chan1Client),
    Chan2(Bytes),
}

impl Packet {
    fn channel(&self) -> u8 {
        match self {
            Packet::Chan0(_) => 0,
            Packet::Chan1(_) => 1,
            Packet::Chan2(_) => 2,
        }
    }
}

impl SauerReceiver {
    fn new(peer: PeerReceiver) -> Self {
        Self {
            rx: peer,
            session: Session::new(),
            buffer: VecDeque::new(),
        }
    }

    async fn recv(&mut self) -> Result<Packet> {
        loop {
            if let Some(msg) = self.buffer.pop_front() {
                return Ok(msg);
            }

            let pkt = match self.rx.rx.recv().await {
                Some(Ok(x)) => x,
                Some(Err(e)) => bail!("enet connection error: {e:?}"),
                None => bail!("enet packet read channel lost sender"),
            };

            match pkt.channel {
                sauer_proto::CHANNEL_POSITIONS => {
                    let mut reader = Reader::<C2S<Chan0>>::new(&pkt.data, &mut self.session);
                    while let Some(msg) = reader.next() {
                        self.buffer.push_back(Packet::Chan0(msg?));
                    }
                }
                sauer_proto::CHANNEL_MESSAGES => {
                    let mut reader = Reader::<C2S<Chan1>>::new(&pkt.data, &mut self.session);
                    while let Some(msg) = reader.next() {
                        self.buffer.push_back(Packet::Chan1(msg?));
                    }
                }
                sauer_proto::CHANNEL_MAPS => {
                    self.buffer.push_back(Packet::Chan2(pkt.data));
                }
                _ => panic!("unexpected ENet channel"),
            }
        }
    }
}
