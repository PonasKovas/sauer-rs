use crate::Server;
use bytes::Bytes;
use sauer_proto::{channel0, channel1};
use std::{ops::ControlFlow, sync::Arc};

// if only Try was stable
pub type Control = ControlFlow<()>;
pub const PASS: Control = ControlFlow::Continue(());
pub const BLOCK: Control = ControlFlow::Break(());

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

macro_rules! make_packet_handlers {
    (
        $( [$msg_id:ident] $method_name:ident ($packet_struct:path) ),* $(,)?
    ) => {
        $(
            #[doc = stringify!($msg_id packet handler)]
            fn $method_name(
                &self,
                ctx: Context<'_>,
                packet: & $packet_struct,
            ) -> impl Future<Output = Control> + Send + Sync {
                async move { PASS }
            }
        )*
    };
}

pub trait Plugin: Send + Sync + 'static {
    #![allow(unused)]

    /// On server initialization
    fn init(&self, server: &Arc<Server>) -> impl Future<Output = ()> + Send + Sync {
        async move { () }
    }

    /// On new peer connection
    ///
    /// BLOCKING will disconnect the peer immediatelly
    fn connected(&self, ctx: Context<'_>) -> impl Future<Output = Control> + Send + Sync {
        async move { PASS }
    }
    /// On peer disconnection
    fn disconnected(&self, ctx: Context<'_>) -> impl Future<Output = Control> + Send + Sync {
        async move { PASS }
    }

    /// `CHANNEL_2` (Maps) raw map data from the client
    fn map(&self, ctx: Context<'_>, packet: &Bytes) -> impl Future<Output = Control> + Send + Sync {
        async move { PASS }
    }

    make_packet_handlers! {
        [N_POS] pos(channel0::Position),
        [N_TELEPORT] teleport(channel0::Teleport),
        [N_JUMPPAD] jumppad(channel0::JumpPad),

        [N_CONNECT] connect(channel1::Connect),
        [N_AUTHANS] auth_answer(channel1::AuthAnswer),
        [N_PING] ping(channel1::Ping),
        [N_FROMAI] from_ai(channel1::FromAi),
        [N_SOUND] sound(channel1::Sound),
        [N_EDITMODE] edit_mode(channel1::EditModeClient),
        [N_MAPCRC] map_crc(channel1::MapCrc),
        [N_CHECKMAPS] check_maps(channel1::CheckMaps),
        [N_TRYSPAWN] try_spawn(channel1::TrySpawn),
        [N_GUNSELECT] gun_select(channel1::GunSelect),
        [N_SPAWN] spawn_client(channel1::SpawnClient),
        [N_SUICIDE] suicide(channel1::Suicide),
        [N_SHOOT] shoot(channel1::Shoot),
        [N_EXPLODE] explode(channel1::Explode),
        [N_ITEMPICKUP] item_pickup(channel1::ItemPickup),
        [N_TEXT] text(channel1::Text),
        [N_SAYTEAM] say_team(channel1::SayTeamClient),
        [N_SWITCHNAME] switch_name(channel1::SwitchName),
        [N_SWITCHMODEL] switch_model(channel1::SwitchModel),
        [N_SWITCHTEAM] switch_team(channel1::SwitchTeam),
        [N_MAPVOTE] map_vote(channel1::MapVote),
        [N_ITEMLIST] item_list(channel1::ItemList),
        [N_EDITENT] edit_ent(channel1::EditEntClient),
        [N_EDITVAR] edit_var(channel1::EditVar),
        [N_CLIENTPING] client_ping(channel1::ClientPing),
        [N_MASTERMODE] master_mode(channel1::MasterModeClient),
        [N_CLEARBANS] clear_bans(channel1::ClearBans),
        [N_KICK] kick(channel1::Kick),
        [N_SPECTATOR] spectator(channel1::SpectatorClient),
        [N_SETTEAM] set_team(channel1::SetTeamClient),
        [N_FORCEINTERMISSION] force_intermission(channel1::ForceIntermission),
        [N_RECORDDEMO] record_demo(channel1::RecordDemo),
        [N_STOPDEMO] stop_demo(channel1::StopDemo),
        [N_CLEARDEMOS] clear_demos(channel1::ClearDemos),
        [N_LISTDEMOS] list_demos(channel1::ListDemos),
        [N_GETDEMO] get_demo(channel1::GetDemo),
        [N_GETMAP] get_map(channel1::GetMap),
        [N_NEWMAP] new_map(channel1::NewMap),
        [N_SETMASTER] set_master(channel1::SetMaster),
        [N_ADDBOT] add_bot(channel1::AddBot),
        [N_DELBOT] del_bot(channel1::DelBot),
        [N_BOTLIMIT] bot_limit(channel1::BotLimit),
        [N_BOTBALANCE] bot_balance(channel1::BotBalance),
        [N_AUTHTRY] auth_try(channel1::AuthTry),
        [N_AUTHKICK] auth_kick(channel1::AuthKick),
        [N_PAUSEGAME] pause_game(channel1::PauseGameClient),
        [N_GAMESPEED] game_speed(channel1::GameSpeedClient),
        [N_COPY] copy(channel1::CopyClient),
        [N_PASTE] paste(channel1::PasteClient),
        [N_CLIPBOARD] clipboard(channel1::ClipboardClient),
        [N_UNDO] undo(channel1::UndoClient),
        [N_REDO] redo(channel1::RedoClient),
        [N_EDITF] edit_face(channel1::EditFace),
        [N_EDITT] edit_tex(channel1::EditTex),
        [N_EDITM] edit_mat(channel1::EditMat),
        [N_FLIP] flip(channel1::Flip),
        [N_ROTATE] rotate(channel1::Rotate),
        [N_REPLACE] replace(channel1::Replace),
        [N_DELCUBE] del_cube(channel1::DelCube),
        [N_EDITVSLOT] edit_v_slot(channel1::EditVSlot),
        [N_REMIP] remip(channel1::Remip),
        [N_SERVCMD] serv_cmd(channel1::ServCmd),
        [N_BASES] bases(channel1::BasesClient),
        [N_REPAMMO] rep_ammo(channel1::RepAmmoClient),
        [N_TRYDROPFLAG] try_drop_flag(channel1::TryDropFlag),
        [N_TAKEFLAG] take_flag(channel1::TakeFlagClient),
        [N_INITFLAGS] init_flags(channel1::InitFlagsClient),
        [N_INITTOKENS] init_tokens(channel1::InitTokensClient),
        [N_TAKETOKEN] take_token(channel1::TakeTokenClient),
        [N_DEPOSITTOKENS] deposit_tokens(channel1::DepositTokensClient),
        [N_TAUNT] taunt(channel1::Taunt),
    }
}

macro_rules! make_packet_handlers_for_tuple {
    (
        // we dont need the $msg_id here but we leave it so the format is the same
        // as for the other macro above and it can be copy pasted
        $( [$msg_id:ident] $method_name:ident ($packet_struct:path) ),* $(,)?
    ) => {
        $(
            async fn $method_name(&self, ctx: Context<'_>, packet: & $packet_struct) -> Control {
                // run the right (newer) plugin, and the left - older
                self.1.$method_name(ctx, packet).await?;
                self.0.$method_name(ctx, packet).await
            }
        )*
    };
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

    async fn map(&self, ctx: Context<'_>, p: &Bytes) -> Control {
        // run the left (previous) plugin
        self.0.map(ctx, p).await?;
        self.1.map(ctx, p).await
    }

    make_packet_handlers_for_tuple! {
        [N_POS] pos(channel0::Position),
        [N_TELEPORT] teleport(channel0::Teleport),
        [N_JUMPPAD] jumppad(channel0::JumpPad),

        [N_CONNECT] connect(channel1::Connect),
        [N_AUTHANS] auth_answer(channel1::AuthAnswer),
        [N_PING] ping(channel1::Ping),
        [N_FROMAI] from_ai(channel1::FromAi),
        [N_SOUND] sound(channel1::Sound),
        [N_EDITMODE] edit_mode(channel1::EditModeClient),
        [N_MAPCRC] map_crc(channel1::MapCrc),
        [N_CHECKMAPS] check_maps(channel1::CheckMaps),
        [N_TRYSPAWN] try_spawn(channel1::TrySpawn),
        [N_GUNSELECT] gun_select(channel1::GunSelect),
        [N_SPAWN] spawn_client(channel1::SpawnClient),
        [N_SUICIDE] suicide(channel1::Suicide),
        [N_SHOOT] shoot(channel1::Shoot),
        [N_EXPLODE] explode(channel1::Explode),
        [N_ITEMPICKUP] item_pickup(channel1::ItemPickup),
        [N_TEXT] text(channel1::Text),
        [N_SAYTEAM] say_team(channel1::SayTeamClient),
        [N_SWITCHNAME] switch_name(channel1::SwitchName),
        [N_SWITCHMODEL] switch_model(channel1::SwitchModel),
        [N_SWITCHTEAM] switch_team(channel1::SwitchTeam),
        [N_MAPVOTE] map_vote(channel1::MapVote),
        [N_ITEMLIST] item_list(channel1::ItemList),
        [N_EDITENT] edit_ent(channel1::EditEntClient),
        [N_EDITVAR] edit_var(channel1::EditVar),
        [N_CLIENTPING] client_ping(channel1::ClientPing),
        [N_MASTERMODE] master_mode(channel1::MasterModeClient),
        [N_CLEARBANS] clear_bans(channel1::ClearBans),
        [N_KICK] kick(channel1::Kick),
        [N_SPECTATOR] spectator(channel1::SpectatorClient),
        [N_SETTEAM] set_team(channel1::SetTeamClient),
        [N_FORCEINTERMISSION] force_intermission(channel1::ForceIntermission),
        [N_RECORDDEMO] record_demo(channel1::RecordDemo),
        [N_STOPDEMO] stop_demo(channel1::StopDemo),
        [N_CLEARDEMOS] clear_demos(channel1::ClearDemos),
        [N_LISTDEMOS] list_demos(channel1::ListDemos),
        [N_GETDEMO] get_demo(channel1::GetDemo),
        [N_GETMAP] get_map(channel1::GetMap),
        [N_NEWMAP] new_map(channel1::NewMap),
        [N_SETMASTER] set_master(channel1::SetMaster),
        [N_ADDBOT] add_bot(channel1::AddBot),
        [N_DELBOT] del_bot(channel1::DelBot),
        [N_BOTLIMIT] bot_limit(channel1::BotLimit),
        [N_BOTBALANCE] bot_balance(channel1::BotBalance),
        [N_AUTHTRY] auth_try(channel1::AuthTry),
        [N_AUTHKICK] auth_kick(channel1::AuthKick),
        [N_PAUSEGAME] pause_game(channel1::PauseGameClient),
        [N_GAMESPEED] game_speed(channel1::GameSpeedClient),
        [N_COPY] copy(channel1::CopyClient),
        [N_PASTE] paste(channel1::PasteClient),
        [N_CLIPBOARD] clipboard(channel1::ClipboardClient),
        [N_UNDO] undo(channel1::UndoClient),
        [N_REDO] redo(channel1::RedoClient),
        [N_EDITF] edit_face(channel1::EditFace),
        [N_EDITT] edit_tex(channel1::EditTex),
        [N_EDITM] edit_mat(channel1::EditMat),
        [N_FLIP] flip(channel1::Flip),
        [N_ROTATE] rotate(channel1::Rotate),
        [N_REPLACE] replace(channel1::Replace),
        [N_DELCUBE] del_cube(channel1::DelCube),
        [N_EDITVSLOT] edit_v_slot(channel1::EditVSlot),
        [N_REMIP] remip(channel1::Remip),
        [N_SERVCMD] serv_cmd(channel1::ServCmd),
        [N_BASES] bases(channel1::BasesClient),
        [N_REPAMMO] rep_ammo(channel1::RepAmmoClient),
        [N_TRYDROPFLAG] try_drop_flag(channel1::TryDropFlag),
        [N_TAKEFLAG] take_flag(channel1::TakeFlagClient),
        [N_INITFLAGS] init_flags(channel1::InitFlagsClient),
        [N_INITTOKENS] init_tokens(channel1::InitTokensClient),
        [N_TAKETOKEN] take_token(channel1::TakeTokenClient),
        [N_DEPOSITTOKENS] deposit_tokens(channel1::DepositTokensClient),
        [N_TAUNT] taunt(channel1::Taunt),
    }
}
