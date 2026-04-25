use crate::Server;
use bytes::Bytes;
use cube_string::CubeString;
use sauer_proto::{channel0, channel1, data::{GameMode, PlayerState}};
use std::sync::Arc;

pub struct Context {
    pub(crate) server: Arc<Server>,
    pub(crate) cn: u32,
    pub(crate) cancelled: bool,
    /// Set by the base plugin when a map change was performed, to trigger `map_changed` hooks.
    pub(crate) map_changed: Option<(CubeString, GameMode)>,
    /// Set by the base plugin after validating a spawn request, to trigger `on_spawn` hooks.
    pub(crate) pending_spawn: Option<PlayerState>,
}
impl Context {
    pub fn server(&self) -> &Arc<Server> {
        &self.server
    }
    pub fn cn(&self) -> u32 {
        self.cn
    }
    pub fn cancel(&mut self) {
        self.cancelled = true;
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
                ctx: &mut Context,
                packet: & $packet_struct,
            ) -> impl Future<Output = ()> + Send + Sync {
                async move {  }
            }
        )*
    };
}

pub trait Plugin: Send + Sync + 'static {
    #![allow(unused)]

    /// On server initialization
    fn init(&self, server: &mut Server) -> impl Future<Output = ()> + Send + Sync {
        async move {}
    }
    /// On server start
    fn start(&self, server: &Arc<Server>) -> impl Future<Output = ()> + Send + Sync {
        async move {}
    }

    /// On new peer connection
    ///
    /// BLOCKING will disconnect the peer immediatelly
    fn connected(&self, ctx: &mut Context) -> impl Future<Output = ()> + Send + Sync {
        async move {}
    }
    /// On peer disconnection, this cannot be cancelled as it's essential for resource cleanup
    fn disconnected(&self, ctx: &mut Context) -> impl Future<Output = ()> + Send + Sync {
        async move {}
    }

    /// Fired after a map change is committed (vote passed or master forced it).
    /// `cn` is the client who cast the deciding vote.
    fn map_changed(
        &self,
        ctx: &mut Context,
        map_name: &CubeString,
        mode: GameMode,
    ) -> impl Future<Output = ()> + Send + Sync {
        async move {}
    }

    /// Fired when a player is about to spawn (in response to `N_TRYSPAWN`).
    /// `state` holds the default spawn stats; modify it to customise the loadout.
    /// Call `ctx.cancel()` to prevent the spawn entirely (the client stays in Dead state).
    fn on_spawn(
        &self,
        ctx: &mut Context,
        state: &mut PlayerState,
    ) -> impl Future<Output = ()> + Send + Sync {
        async move {}
    }

    /// `CHANNEL_2` (Maps) raw map data from the client
    fn map(&self, ctx: &mut Context, packet: &Bytes) -> impl Future<Output = ()> + Send + Sync {
        async move {}
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
        [N_EDITMODE] edit_mode(channel1::EditMode),
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
        [N_EDITENT] edit_ent(channel1::EditEnt),
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
            async fn $method_name(&self, ctx: &mut Context, packet: & $packet_struct) {
                // run the right (newer) plugin, and the left - older
                self.1.$method_name(ctx, packet).await;
                if ctx.cancelled {
                    return;
                }
                self.0.$method_name(ctx, packet).await;
            }
        )*
    };
}

impl Plugin for () {}

impl<P1: Plugin, P2: Plugin> Plugin for (P1, P2) {
    async fn init(&self, server: &mut Server) {
        // run the right (newer) plugin, and the left - older
        self.1.init(server).await;
        self.0.init(server).await
    }
    async fn start(&self, server: &Arc<Server>) {
        // run the right (newer) plugin, and the left - older
        self.1.start(server).await;
        self.0.start(server).await
    }
    async fn connected(&self, ctx: &mut Context) {
        // run the right (newer) plugin, and the left - older
        self.1.connected(ctx).await;
        if ctx.cancelled {
            return;
        }
        self.0.connected(ctx).await;
    }
    async fn disconnected(&self, ctx: &mut Context) {
        // run the right (newer) plugin, and the left - older
        self.1.disconnected(ctx).await;
        self.0.disconnected(ctx).await;
    }

    async fn map_changed(&self, ctx: &mut Context, map_name: &CubeString, mode: GameMode) {
        self.1.map_changed(ctx, map_name, mode).await;
        if ctx.cancelled {
            return;
        }
        self.0.map_changed(ctx, map_name, mode).await;
    }

    async fn on_spawn(&self, ctx: &mut Context, state: &mut PlayerState) {
        self.1.on_spawn(ctx, state).await;
        if ctx.cancelled {
            return;
        }
        self.0.on_spawn(ctx, state).await;
    }

    async fn map(&self, ctx: &mut Context, p: &Bytes) {
        // run the right (newer) plugin, and the left - older
        self.1.map(ctx, p).await;
        if ctx.cancelled {
            return;
        }
        self.0.map(ctx, p).await;
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
        [N_EDITMODE] edit_mode(channel1::EditMode),
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
        [N_EDITENT] edit_ent(channel1::EditEnt),
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
