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

pub(crate) async fn handle_client<P: Plugin>(
    server: &Arc<Server>,
    plugin: &Arc<P>,
    peer: PeerReceiver,
    cn: u32,
) -> Result<()> {
    let mut rx = SauerReceiver::new(peer);

    loop {
        let pkt = rx.recv().await?;

        if pkt.channel() == 1 {
            trace!("received: {pkt:?}");
        }

        let ctx = Context::new(server, cn);

        let _ = match pkt {
            Packet::Chan0(pkt) => match pkt {
                Chan0Client::Position(p) => plugin.pos(ctx, &p).await,
                Chan0Client::Teleport(p) => plugin.teleport(ctx, &p).await,
                Chan0Client::JumpPad(p) => plugin.jumppad(ctx, &p).await,
            },
            Packet::Chan1(pkt) => match pkt {
                Chan1Client::Connect(p) => plugin.connect(ctx, &p).await,
                Chan1Client::AuthAnswer(p) => plugin.auth_answer(ctx, &p).await,
                Chan1Client::Ping(p) => plugin.ping(ctx, &p).await,
                Chan1Client::FromAi(p) => plugin.from_ai(ctx, &p).await,
                Chan1Client::Sound(p) => plugin.sound(ctx, &p).await,
                Chan1Client::EditModeClient(p) => plugin.edit_mode(ctx, &p).await,
                Chan1Client::MapCrc(p) => plugin.map_crc(ctx, &p).await,
                Chan1Client::CheckMaps(p) => plugin.check_maps(ctx, &p).await,
                Chan1Client::TrySpawn(p) => plugin.try_spawn(ctx, &p).await,
                Chan1Client::GunSelect(p) => plugin.gun_select(ctx, &p).await,
                Chan1Client::SpawnClient(p) => plugin.spawn_client(ctx, &p).await,
                Chan1Client::Suicide(p) => plugin.suicide(ctx, &p).await,
                Chan1Client::Shoot(p) => plugin.shoot(ctx, &p).await,
                Chan1Client::Explode(p) => plugin.explode(ctx, &p).await,
                Chan1Client::ItemPickup(p) => plugin.item_pickup(ctx, &p).await,
                Chan1Client::Text(p) => plugin.text(ctx, &p).await,
                Chan1Client::SayTeamClient(p) => plugin.say_team(ctx, &p).await,
                Chan1Client::SwitchName(p) => plugin.switch_name(ctx, &p).await,
                Chan1Client::SwitchModel(p) => plugin.switch_model(ctx, &p).await,
                Chan1Client::SwitchTeam(p) => plugin.switch_team(ctx, &p).await,
                Chan1Client::MapVote(p) => plugin.map_vote(ctx, &p).await,
                Chan1Client::ItemList(p) => plugin.item_list(ctx, &p).await,
                Chan1Client::EditEntClient(p) => plugin.edit_ent(ctx, &p).await,
                Chan1Client::EditVar(p) => plugin.edit_var(ctx, &p).await,
                Chan1Client::ClientPing(p) => plugin.client_ping(ctx, &p).await,
                Chan1Client::MasterModeClient(p) => plugin.master_mode(ctx, &p).await,
                Chan1Client::ClearBans(p) => plugin.clear_bans(ctx, &p).await,
                Chan1Client::Kick(p) => plugin.kick(ctx, &p).await,
                Chan1Client::SpectatorClient(p) => plugin.spectator(ctx, &p).await,
                Chan1Client::SetTeamClient(p) => plugin.set_team(ctx, &p).await,
                Chan1Client::ForceIntermission(p) => plugin.force_intermission(ctx, &p).await,
                Chan1Client::RecordDemo(p) => plugin.record_demo(ctx, &p).await,
                Chan1Client::StopDemo(p) => plugin.stop_demo(ctx, &p).await,
                Chan1Client::ClearDemos(p) => plugin.clear_demos(ctx, &p).await,
                Chan1Client::ListDemos(p) => plugin.list_demos(ctx, &p).await,
                Chan1Client::GetDemo(p) => plugin.get_demo(ctx, &p).await,
                Chan1Client::GetMap(p) => plugin.get_map(ctx, &p).await,
                Chan1Client::NewMap(p) => plugin.new_map(ctx, &p).await,
                Chan1Client::SetMaster(p) => plugin.set_master(ctx, &p).await,
                Chan1Client::AddBot(p) => plugin.add_bot(ctx, &p).await,
                Chan1Client::DelBot(p) => plugin.del_bot(ctx, &p).await,
                Chan1Client::BotLimit(p) => plugin.bot_limit(ctx, &p).await,
                Chan1Client::BotBalance(p) => plugin.bot_balance(ctx, &p).await,
                Chan1Client::AuthTry(p) => plugin.auth_try(ctx, &p).await,
                Chan1Client::AuthKick(p) => plugin.auth_kick(ctx, &p).await,
                Chan1Client::PauseGameClient(p) => plugin.pause_game(ctx, &p).await,
                Chan1Client::GameSpeedClient(p) => plugin.game_speed(ctx, &p).await,
                Chan1Client::CopyClient(p) => plugin.copy(ctx, &p).await,
                Chan1Client::PasteClient(p) => plugin.paste(ctx, &p).await,
                Chan1Client::ClipboardClient(p) => plugin.clipboard(ctx, &p).await,
                Chan1Client::UndoClient(p) => plugin.undo(ctx, &p).await,
                Chan1Client::RedoClient(p) => plugin.redo(ctx, &p).await,
                Chan1Client::EditFace(p) => plugin.edit_face(ctx, &p).await,
                Chan1Client::EditTex(p) => plugin.edit_tex(ctx, &p).await,
                Chan1Client::EditMat(p) => plugin.edit_mat(ctx, &p).await,
                Chan1Client::Flip(p) => plugin.flip(ctx, &p).await,
                Chan1Client::Rotate(p) => plugin.rotate(ctx, &p).await,
                Chan1Client::Replace(p) => plugin.replace(ctx, &p).await,
                Chan1Client::DelCube(p) => plugin.del_cube(ctx, &p).await,
                Chan1Client::EditVSlot(p) => plugin.edit_v_slot(ctx, &p).await,
                Chan1Client::Remip(p) => plugin.remip(ctx, &p).await,
                Chan1Client::ServCmd(p) => plugin.serv_cmd(ctx, &p).await,
                Chan1Client::BasesClient(p) => plugin.bases(ctx, &p).await,
                Chan1Client::RepAmmoClient(p) => plugin.rep_ammo(ctx, &p).await,
                Chan1Client::TryDropFlag(p) => plugin.try_drop_flag(ctx, &p).await,
                Chan1Client::TakeFlagClient(p) => plugin.take_flag(ctx, &p).await,
                Chan1Client::InitFlagsClient(p) => plugin.init_flags(ctx, &p).await,
                Chan1Client::InitTokensClient(p) => plugin.init_tokens(ctx, &p).await,
                Chan1Client::TakeTokenClient(p) => plugin.take_token(ctx, &p).await,
                Chan1Client::DepositTokensClient(p) => plugin.deposit_tokens(ctx, &p).await,
                Chan1Client::Taunt(p) => plugin.taunt(ctx, &p).await,
            },
            Packet::Chan2(pkt) => plugin.map(ctx, &pkt).await,
        };
    }
}

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
    fn msg_type(&self) -> &'static str {
        match self {
            Packet::Chan0(v) => v.msg_type(),
            Packet::Chan1(v) => v.msg_type(),
            Packet::Chan2(_) => "CHANNEL2_MAP",
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
            // If we have buffered messages from the last ENet packet, return them first.
            if let Some(msg) = self.buffer.pop_front() {
                return Ok(msg);
            }

            // Wait for the next ENet packet
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
                    // Channel 2 (File/Map transfer): Just raw bytes, no packet wrappers.
                    self.buffer.push_back(Packet::Chan2(pkt.data));
                }
                _ => panic!(),
            }
        }
    }
}
