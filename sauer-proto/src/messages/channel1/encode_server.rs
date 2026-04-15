use super::*;
use crate::{
    data::{EditVarValue, FlagStateHoldTeam, FlagStateOwner},
    internal::{constants::*, EncodeCore, WriterCore},
    route::{Chan1, S2C},
    Encode,
};

// =====================================================================
// Individual struct Encode impls for S2C<Chan1>
// =====================================================================

impl Encode<S2C<Chan1>> for DemoPacket {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_DEMOPACKET);
    }
}

impl Encode<S2C<Chan1>> for ServInfo {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_SERVINFO);
        w.put_int(self.client_num);
        w.put_int(self.protocol);
        w.put_int(self.session_id);
        w.put_bool(self.has_password);
        w.put_string(&self.server_desc);
        w.put_string(&self.server_auth);
    }
}

impl Encode<S2C<Chan1>> for Welcome {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_WELCOME);
    }
}

impl Encode<S2C<Chan1>> for PauseGameServer {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_PAUSEGAME);
        w.put_bool(self.is_paused);
        w.put_int(self.client_num);
    }
}

impl Encode<S2C<Chan1>> for GameSpeedServer {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_GAMESPEED);
        w.put_int(self.speed);
        w.put_int(self.client_num);
    }
}

impl Encode<S2C<Chan1>> for SayTeamServer {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_SAYTEAM);
        w.put_int(self.client_num);
        w.put_string(&self.text);
    }
}

impl Encode<S2C<Chan1>> for MapChange {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_MAPCHANGE);
        w.put_string(&self.map_name);
        w.put_int(self.mode as i32);
        w.put_bool(self.has_items);
    }
}

impl Encode<S2C<Chan1>> for ForceDeath {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_FORCEDEATH);
        w.put_int(self.client_num);
    }
}

impl Encode<S2C<Chan1>> for ItemList {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_ITEMLIST);
        for item in &self.items {
            w.put_int(item.item_index);
            w.put_int(item.item_type as i32);
        }
        w.put_int(-1);
    }
}

impl Encode<S2C<Chan1>> for InitClient {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_INITCLIENT);
        w.put_int(self.client_num);
        w.put_string(&self.name);
        w.put_string(&self.team);
        w.put_int(self.player_model as i32);
    }
}

impl Encode<S2C<Chan1>> for ClientDisconnected {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_CDIS);
        w.put_int(self.client_num);
    }
}

impl Encode<S2C<Chan1>> for SpawnState {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_SPAWNSTATE);
        w.put_int(self.client_num);
        EncodeCore::encode(&self.state, w);
    }
}

impl Encode<S2C<Chan1>> for ShotFx {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_SHOTFX);
        w.put_int(self.client_num);
        w.put_int(self.gun as i32);
        w.put_int(self.id);
        w.put_int((&self.from.x * DMF) as i32);
        w.put_int((&self.from.y * DMF) as i32);
        w.put_int((&self.from.z * DMF) as i32);
        w.put_int((&self.to.x * DMF) as i32);
        w.put_int((&self.to.y * DMF) as i32);
        w.put_int((&self.to.z * DMF) as i32);
    }
}

impl Encode<S2C<Chan1>> for ExplodeFx {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_EXPLODEFX);
        w.put_int(self.client_num);
        w.put_int(self.gun as i32);
        w.put_int(self.id);
    }
}

impl Encode<S2C<Chan1>> for Damage {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_DAMAGE);
        w.put_int(self.target_cn);
        w.put_int(self.actor_cn);
        w.put_int(self.damage);
        w.put_int(self.armour);
        w.put_int(self.health);
    }
}

impl Encode<S2C<Chan1>> for HitPush {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_HITPUSH);
        w.put_int(self.target_cn);
        w.put_int(self.gun as i32);
        w.put_int(self.damage);
        w.put_int((&self.dir.x * DMF) as i32);
        w.put_int((&self.dir.y * DMF) as i32);
        w.put_int((&self.dir.z * DMF) as i32);
    }
}

impl Encode<S2C<Chan1>> for Died {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_DIED);
        w.put_int(self.victim_cn);
        w.put_int(self.actor_cn);
        w.put_int(self.actor_frags);
        w.put_int(self.team_frags);
    }
}

impl Encode<S2C<Chan1>> for TeamInfo {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_TEAMINFO);
        for team in &self.teams {
            w.put_string(&team.team);
            w.put_int(team.frags);
        }
        w.put_byte(0);
    }
}

impl Encode<S2C<Chan1>> for Resume {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_RESUME);
        for c in &self.clients {
            w.put_int(c.client_num);
            w.put_int(c.state as i32);
            w.put_int(c.frags);
            w.put_int(c.flags);
            w.put_int(c.deaths);
            w.put_int(c.quad_millis);
            EncodeCore::encode(&c.player_state, w);
        }
        w.put_int(-1);
    }
}

impl Encode<S2C<Chan1>> for ItemSpawn {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_ITEMSPAWN);
        w.put_int(self.item_index);
    }
}

impl Encode<S2C<Chan1>> for ItemAck {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_ITEMACC);
        w.put_int(self.item_index);
        w.put_int(self.client_num);
    }
}

impl Encode<S2C<Chan1>> for ClipboardServer {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_CLIPBOARD);
        w.put_int(self.client_num);
        w.put_int(self.unpack_len);
        w.put_int(self.pack_len);
        w.put_bytes(&self.data);
    }
}

impl Encode<S2C<Chan1>> for UndoServer {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_UNDO);
        w.put_int(self.client_num);
        w.put_int(self.unpack_len);
        w.put_int(self.pack_len);
        w.put_bytes(&self.data);
    }
}

impl Encode<S2C<Chan1>> for RedoServer {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_REDO);
        w.put_int(self.client_num);
        w.put_int(self.unpack_len);
        w.put_int(self.pack_len);
        w.put_bytes(&self.data);
    }
}

impl Encode<S2C<Chan1>> for Pong {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_PONG);
        w.put_int(self.ping);
    }
}

impl Encode<S2C<Chan1>> for TimeUp {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_TIMEUP);
        w.put_int(self.seconds_remaining);
    }
}

impl Encode<S2C<Chan1>> for ServMsg {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_SERVMSG);
        w.put_string(&self.text);
    }
}

impl Encode<S2C<Chan1>> for SendDemoList {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_SENDDEMOLIST);
        w.put_int(self.demos.len() as i32);
        for demo in &self.demos {
            w.put_string(demo);
        }
    }
}

impl Encode<S2C<Chan1>> for DemoPlayback {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_DEMOPLAYBACK);
        w.put_bool(self.is_on);
        w.put_int(self.client_num);
    }
}

impl Encode<S2C<Chan1>> for CurrentMaster {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_CURRENTMASTER);
        w.put_int(self.master_mode as i32);
        for c in &self.clients {
            w.put_int(c.client_num);
            w.put_int(c.privilege as i32);
        }
        w.put_int(-1);
    }
}

impl Encode<S2C<Chan1>> for MasterModeServer {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_MASTERMODE);
        w.put_int(self.master_mode as i32);
    }
}

impl Encode<S2C<Chan1>> for SpectatorServer {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_SPECTATOR);
        w.put_int(self.client_num);
        w.put_bool(self.is_spectator);
    }
}

impl Encode<S2C<Chan1>> for SetTeamServer {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_SETTEAM);
        w.put_int(self.client_num);
        w.put_string(&self.team);
        w.put_int(self.reason);
    }
}

impl Encode<S2C<Chan1>> for Announce {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_ANNOUNCE);
        w.put_int(self.item_type as i32);
    }
}

impl Encode<S2C<Chan1>> for NewMap {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_NEWMAP);
        w.put_int(self.size);
    }
}

impl Encode<S2C<Chan1>> for ReqAuth {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_REQAUTH);
        w.put_string(&self.auth_domain);
    }
}

impl Encode<S2C<Chan1>> for AuthChal {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_AUTHCHAL);
        w.put_string(&self.desc);
        w.put_int(self.id as i32);
        w.put_string(&self.challenge);
    }
}

impl Encode<S2C<Chan1>> for InitAi {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_INITAI);
        w.put_int(self.bot_cn);
        w.put_int(self.owner_cn);
        w.put_int(self.ai_type as i32);
        w.put_int(self.skill);
        w.put_int(self.player_model as i32);
        w.put_string(&self.name);
        w.put_string(&self.team);
    }
}

impl Encode<S2C<Chan1>> for ServCmd {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_SERVCMD);
        w.put_string(&self.cmd);
    }
}

impl Encode<S2C<Chan1>> for BasesServer {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_BASES);
        w.put_int(self.bases.len() as i32);
        for b in &self.bases {
            w.put_int(b.ammotype.map_or(0, |t| t as i32));
            w.put_string(&b.owner);
            w.put_string(&b.enemy);
            w.put_int(if b.converted { 1 } else { 0 });
            w.put_int(b.ammo);
        }
    }
}

impl Encode<S2C<Chan1>> for BaseInfo {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_BASEINFO);
        w.put_int(self.base);
        w.put_string(&self.owner);
        w.put_string(&self.enemy);
        w.put_bool(self.converted);
        w.put_int(self.ammo);
    }
}

impl Encode<S2C<Chan1>> for BaseScore {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_BASESCORE);
        w.put_int(self.base);
        w.put_string(&self.team);
        w.put_int(self.total);
    }
}

impl Encode<S2C<Chan1>> for RepAmmoServer {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_REPAMMO);
        w.put_int(self.client_num);
        w.put_int(self.ammotype as i32);
    }
}

impl Encode<S2C<Chan1>> for BaseRegen {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_BASEREGEN);
        w.put_int(self.client_num);
        w.put_int(self.health);
        w.put_int(self.armour);
        w.put_int(self.ammotype as i32);
        w.put_int(self.ammo);
    }
}

impl Encode<S2C<Chan1>> for InitFlagsServer {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_INITFLAGS);
        w.put_int(self.scores[0]);
        w.put_int(self.scores[1]);
        w.put_int(self.flags.len() as i32);
        for f in &self.flags {
            w.put_int(f.version);
            w.put_int(f.spawn_index);
            if let FlagStateOwner::Owned { owner } = &f.owner {
                w.put_int(*owner);
            } else {
                w.put_int(-1);
            }
            w.put_int(if f.invis { 1 } else { 0 });
            if let Some(hold) = &f.hold {
                match hold {
                    FlagStateHoldTeam::None => {
                        w.put_int(-1);
                    }
                    FlagStateHoldTeam::Team { team, hold_time } => {
                        w.put_int(*team);
                        w.put_int(*hold_time);
                    }
                }
            }
        }
    }
}

impl Encode<S2C<Chan1>> for DropFlag {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_DROPFLAG);
        w.put_int(self.client_num);
        w.put_int(self.flag);
        w.put_int(self.version);
        w.put_int((&self.drop_loc.x * DMF) as i32);
        w.put_int((&self.drop_loc.y * DMF) as i32);
        w.put_int((&self.drop_loc.z * DMF) as i32);
    }
}

impl Encode<S2C<Chan1>> for ScoreFlag {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_SCOREFLAG);
        w.put_int(self.client_num);
        w.put_int(self.relay_flag);
        w.put_int(self.relay_version);
        w.put_int(self.goal_flag);
        w.put_int(self.goal_version);
        w.put_int(self.goal_spawn);
        w.put_int(self.team);
        w.put_int(self.score);
        w.put_int(self.oflags);
    }
}

impl Encode<S2C<Chan1>> for ReturnFlag {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_RETURNFLAG);
        w.put_int(self.client_num);
        w.put_int(self.flag);
        w.put_int(self.version);
    }
}

impl Encode<S2C<Chan1>> for TakeFlagServer {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_TAKEFLAG);
        w.put_int(self.client_num);
        w.put_int(self.flag);
        w.put_int(self.version);
    }
}

impl Encode<S2C<Chan1>> for ResetFlag {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_RESETFLAG);
        w.put_int(self.flag);
        w.put_int(self.version);
        w.put_int(self.spawn_index);
        w.put_int(self.team);
        w.put_int(self.score);
    }
}

impl Encode<S2C<Chan1>> for InvisFlag {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_INVISFLAG);
        w.put_int(self.flag);
        w.put_bool(self.invis);
    }
}

impl Encode<S2C<Chan1>> for InitTokensServer {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_INITTOKENS);
        w.put_int(self.scores[0]);
        w.put_int(self.scores[1]);
        w.put_int(self.tokens.len() as i32);
        for t in &self.tokens {
            w.put_int(t.id);
            w.put_int(t.team);
            w.put_int(t.yaw);
            w.put_int((t.pos.x * DMF) as i32);
            w.put_int((t.pos.y * DMF) as i32);
            w.put_int((t.pos.z * DMF) as i32);
        }
        for ct in &self.client_tokens {
            w.put_int(ct.client_num);
            w.put_int(ct.tokens);
        }
        w.put_int(-1);
    }
}

impl Encode<S2C<Chan1>> for TakeTokenServer {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_TAKETOKEN);
        w.put_int(self.client_num);
        w.put_int(self.id);
        w.put_int(self.total);
    }
}

impl Encode<S2C<Chan1>> for ExpireTokens {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_EXPIRETOKENS);
        for &id in &self.tokens {
            w.put_int(id);
        }
        w.put_int(-1);
    }
}

impl Encode<S2C<Chan1>> for DropTokens {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_DROPTOKENS);
        w.put_int(self.client_num);
        w.put_int((&self.drop_loc.x * DMF) as i32);
        w.put_int((&self.drop_loc.y * DMF) as i32);
        w.put_int((&self.drop_loc.z * DMF) as i32);
        for t in &self.tokens {
            w.put_int(t.id);
            w.put_int(t.team);
            w.put_int(t.yaw);
        }
        w.put_int(-1);
    }
}

impl Encode<S2C<Chan1>> for StealTokens {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_STEALTOKENS);
        w.put_int(self.client_num);
        w.put_int(self.team);
        w.put_int(self.basenum);
        w.put_int(self.enemy_team);
        w.put_int(self.score);
        w.put_int((&self.drop_loc.x * DMF) as i32);
        w.put_int((&self.drop_loc.y * DMF) as i32);
        w.put_int((&self.drop_loc.z * DMF) as i32);
        for t in &self.tokens {
            w.put_int(t.id);
            w.put_int(t.yaw);
        }
        w.put_int(-1);
    }
}

impl Encode<S2C<Chan1>> for DepositTokensServer {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_DEPOSITTOKENS);
        w.put_int(self.client_num);
        w.put_int(self.base);
        w.put_int(self.deposited);
        w.put_int(self.team);
        w.put_int(self.score);
        w.put_int(self.flags);
    }
}

// Client-wrapped packet: the Client struct gets an Encode that wraps it
impl Encode<S2C<Chan1>> for Client {
    fn encode(&self, w: &mut WriterCore) {
        encode_client_packet(&self.packet, w);
    }

    fn requires_client_wrapper(&self) -> Option<i32> {
        Some(self.client_num)
    }
}

// =====================================================================
// Chan1Server enum Encode (delegates to individual impls)
// =====================================================================

impl Encode<S2C<Chan1>> for Chan1Server {
    fn requires_client_wrapper(&self) -> Option<i32> {
        match self {
            Chan1Server::Client(c) => Encode::<S2C<Chan1>>::requires_client_wrapper(c),
            _ => None,
        }
    }

    fn encode(&self, w: &mut WriterCore) {
        match self {
            Chan1Server::DemoPacket(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::ServInfo(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::Welcome(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::PauseGame(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::GameSpeed(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::SayTeam(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::MapChange(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::ForceDeath(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::ItemList(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::InitClient(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::ClientDisconnected(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::SpawnState(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::ShotFx(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::ExplodeFx(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::Damage(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::HitPush(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::Died(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::TeamInfo(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::Resume(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::ItemSpawn(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::ItemAck(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::Clipboard(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::Undo(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::Redo(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::Pong(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::TimeUp(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::ServMsg(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::SendDemoList(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::DemoPlayback(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::CurrentMaster(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::MasterMode(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::Spectator(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::SetTeam(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::Announce(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::NewMap(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::ReqAuth(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::AuthChal(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::InitAi(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::ServCmd(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::Bases(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::BaseInfo(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::BaseScore(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::RepAmmo(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::BaseRegen(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::InitFlags(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::DropFlag(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::ScoreFlag(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::ReturnFlag(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::TakeFlag(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::ResetFlag(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::InvisFlag(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::InitTokens(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::TakeToken(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::ExpireTokens(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::DropTokens(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::StealTokens(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::DepositTokens(v) => Encode::<S2C<Chan1>>::encode(v, w),
            Chan1Server::Client(v) => Encode::<S2C<Chan1>>::encode(v, w),
        }
    }
}

// =====================================================================
// Helper: encode a ClientPacket (shared between Client struct and enum)
// =====================================================================

fn encode_client_packet(packet: &ClientPacket, w: &mut WriterCore) {
    match packet {
        ClientPacket::Sound(v) => {
            w.put_int(N_SOUND);
            w.put_int(v.sound_id);
        }
        ClientPacket::Text(v) => {
            w.put_int(N_TEXT);
            w.put_string(&v.text);
        }
        ClientPacket::SwitchName(v) => {
            w.put_int(N_SWITCHNAME);
            w.put_string(&v.name);
        }
        ClientPacket::SwitchModel(v) => {
            w.put_int(N_SWITCHMODEL);
            w.put_int(v.model as i32);
        }
        ClientPacket::Spawn(v) => {
            w.put_int(N_SPAWN);
            EncodeCore::encode(&v.state, w);
        }
        ClientPacket::GunSelect(v) => {
            w.put_int(N_GUNSELECT);
            w.put_int(v.gun as i32);
        }
        ClientPacket::Taunt(_) => {
            w.put_int(N_TAUNT);
        }
        ClientPacket::EditFace(v) => {
            w.put_int(N_EDITF);
            EncodeCore::encode(&v.sel, w);
            w.put_int(v.dir);
            w.put_int(v.mode as i32);
        }
        ClientPacket::EditTex(v) => {
            w.put_int(N_EDITT);
            EncodeCore::encode(&v.sel, w);
            w.put_int(v.tex);
            w.put_bool(v.all_faces);
            let extra = v.extra_data.len() as u16;
            w.put_byte((extra & 0xFF) as u8);
            w.put_byte((extra >> 8) as u8);
            w.put_bytes(&v.extra_data);
        }
        ClientPacket::EditMat(v) => {
            w.put_int(N_EDITM);
            EncodeCore::encode(&v.sel, w);
            w.put_int(v.mat);
            w.put_int(v.filter);
        }
        ClientPacket::Flip(v) => {
            w.put_int(N_FLIP);
            EncodeCore::encode(&v.sel, w);
        }
        ClientPacket::Copy(v) => {
            w.put_int(N_COPY);
            EncodeCore::encode(&v.sel, w);
        }
        ClientPacket::Paste(v) => {
            w.put_int(N_PASTE);
            EncodeCore::encode(&v.sel, w);
        }
        ClientPacket::Rotate(v) => {
            w.put_int(N_ROTATE);
            EncodeCore::encode(&v.sel, w);
            w.put_int(v.dir);
        }
        ClientPacket::Replace(v) => {
            w.put_int(N_REPLACE);
            EncodeCore::encode(&v.sel, w);
            w.put_int(v.old_tex);
            w.put_int(v.new_tex);
            w.put_int(v.in_sel);
            let extra = v.extra_data.len() as u16;
            w.put_byte((extra & 0xFF) as u8);
            w.put_byte((extra >> 8) as u8);
            w.put_bytes(&v.extra_data);
        }
        ClientPacket::DelCube(v) => {
            w.put_int(N_DELCUBE);
            EncodeCore::encode(&v.sel, w);
        }
        ClientPacket::EditVSlot(v) => {
            w.put_int(N_EDITVSLOT);
            EncodeCore::encode(&v.sel, w);
            w.put_int(v.delta);
            w.put_bool(v.all_faces);
            let extra = v.extra_data.len() as u16;
            w.put_byte((extra & 0xFF) as u8);
            w.put_byte((extra >> 8) as u8);
            w.put_bytes(&v.extra_data);
        }
        ClientPacket::Remip(_) => {
            w.put_int(N_REMIP);
        }
        ClientPacket::EditEnt(v) => {
            w.put_int(N_EDITENT);
            w.put_int(v.ent_id);
            w.put_int((&v.pos.x * DMF) as i32);
            w.put_int((&v.pos.y * DMF) as i32);
            w.put_int((&v.pos.z * DMF) as i32);
            w.put_int(v.type_ as i32);
            w.put_int(v.attr1);
            w.put_int(v.attr2);
            w.put_int(v.attr3);
            w.put_int(v.attr4);
            w.put_int(v.attr5);
        }
        ClientPacket::EditVar(v) => {
            w.put_int(N_EDITVAR);
            match &v.val {
                EditVarValue::Int(i) => {
                    w.put_int(0);
                    w.put_string(&v.name);
                    w.put_int(*i);
                }
                EditVarValue::Float(f) => {
                    w.put_int(1);
                    w.put_string(&v.name);
                    w.put_float(*f);
                }
                EditVarValue::String(s) => {
                    w.put_int(2);
                    w.put_string(&v.name);
                    w.put_string(s);
                }
            }
        }
        ClientPacket::ClientPing(v) => {
            w.put_int(N_CLIENTPING);
            w.put_int(v.ping);
        }
        ClientPacket::EditMode(v) => {
            w.put_int(N_EDITMODE);
            w.put_bool(v.is_editing);
        }
        ClientPacket::NewMap(v) => {
            w.put_int(N_NEWMAP);
            w.put_int(v.size);
        }
    }
}
