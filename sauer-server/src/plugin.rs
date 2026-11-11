use crate::Server;
use bytes::Bytes;
use cube_string::CubeString;
use sauer_proto::{
    data::{GameMode, PlayerState},
    server_query::ServerQueryRequest,
};
use std::{net::SocketAddr, sync::Arc};

/// What a hook says should happen next.
#[must_use]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Flow {
    /// Run the rest of the plugin chain, and the server's own handling.
    #[default]
    Continue,
    /// Stop here: later plugins and the server's own handling don't run.
    Cancel,
}

pub struct ServerQueryContext {
    pub(crate) server: Arc<Server>,
    pub(crate) addr: SocketAddr,
}

impl ServerQueryContext {
    pub fn server(&self) -> &Arc<Server> {
        &self.server
    }
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}

pub struct Context {
    pub(crate) server: Arc<Server>,
    pub(crate) id: u64,
}

impl Context {
    pub fn server(&self) -> &Arc<Server> {
        &self.server
    }
    pub fn id(&self) -> u64 {
        self.id
    }
}

/// Calls `$m!` with every packet a client can send, as
/// `[N_ID] hook(Packet) Variant,` lines grouped by channel, where `Variant`
/// is the packet's `Chan0Client`/`Chan1Client` variant. The `Plugin` trait,
/// its tuple impl and the server's dispatch all expand from this list, so a
/// new packet is added in one place. Tokens after the macro name are passed
/// along first, in parentheses, so `$m` can use the caller's variables.
#[macro_export]
macro_rules! for_each_packet_hook {
    ($m:ident $(, $($args:tt)*)?) => {
        $m! {
            ($($($args)*)?)
            chan0 {
            [N_POS] pos($crate::sauer_proto::channel0::Position) Position,
            [N_TELEPORT] teleport($crate::sauer_proto::channel0::Teleport) Teleport,
            [N_JUMPPAD] jumppad($crate::sauer_proto::channel0::JumpPad) JumpPad,
            }
            chan1 {
            [N_CONNECT] connect($crate::sauer_proto::channel1::Connect) Connect,
            [N_AUTHANS] auth_answer($crate::sauer_proto::channel1::AuthAnswer) AuthAnswer,
            [N_PING] ping($crate::sauer_proto::channel1::Ping) Ping,
            [N_FROMAI] from_ai($crate::sauer_proto::channel1::FromAi) FromAi,
            [N_SOUND] sound($crate::sauer_proto::channel1::Sound) Sound,
            [N_EDITMODE] edit_mode($crate::sauer_proto::channel1::EditMode) EditMode,
            [N_MAPCRC] map_crc($crate::sauer_proto::channel1::MapCrc) MapCrc,
            [N_CHECKMAPS] check_maps($crate::sauer_proto::channel1::CheckMaps) CheckMaps,
            [N_TRYSPAWN] try_spawn($crate::sauer_proto::channel1::TrySpawn) TrySpawn,
            [N_GUNSELECT] gun_select($crate::sauer_proto::channel1::GunSelect) GunSelect,
            [N_SPAWN] spawn_client($crate::sauer_proto::channel1::SpawnClient) SpawnClient,
            [N_SUICIDE] suicide($crate::sauer_proto::channel1::Suicide) Suicide,
            [N_SHOOT] shoot($crate::sauer_proto::channel1::Shoot) Shoot,
            [N_EXPLODE] explode($crate::sauer_proto::channel1::Explode) Explode,
            [N_ITEMPICKUP] item_pickup($crate::sauer_proto::channel1::ItemPickup) ItemPickup,
            [N_TEXT] text($crate::sauer_proto::channel1::Text) Text,
            [N_SAYTEAM] say_team($crate::sauer_proto::channel1::SayTeamClient) SayTeamClient,
            [N_SWITCHNAME] switch_name($crate::sauer_proto::channel1::SwitchName) SwitchName,
            [N_SWITCHMODEL] switch_model($crate::sauer_proto::channel1::SwitchModel) SwitchModel,
            [N_SWITCHTEAM] switch_team($crate::sauer_proto::channel1::SwitchTeam) SwitchTeam,
            [N_MAPVOTE] map_vote($crate::sauer_proto::channel1::MapVote) MapVote,
            [N_ITEMLIST] item_list($crate::sauer_proto::channel1::ItemList) ItemList,
            [N_EDITENT] edit_ent($crate::sauer_proto::channel1::EditEnt) EditEnt,
            [N_EDITVAR] edit_var($crate::sauer_proto::channel1::EditVar) EditVar,
            [N_CLIENTPING] client_ping($crate::sauer_proto::channel1::ClientPing) ClientPing,
            [N_MASTERMODE] master_mode($crate::sauer_proto::channel1::MasterModeClient) MasterModeClient,
            [N_CLEARBANS] clear_bans($crate::sauer_proto::channel1::ClearBans) ClearBans,
            [N_KICK] kick($crate::sauer_proto::channel1::Kick) Kick,
            [N_SPECTATOR] spectator($crate::sauer_proto::channel1::SpectatorClient) SpectatorClient,
            [N_SETTEAM] set_team($crate::sauer_proto::channel1::SetTeamClient) SetTeamClient,
            [N_FORCEINTERMISSION] force_intermission($crate::sauer_proto::channel1::ForceIntermission) ForceIntermission,
            [N_RECORDDEMO] record_demo($crate::sauer_proto::channel1::RecordDemo) RecordDemo,
            [N_STOPDEMO] stop_demo($crate::sauer_proto::channel1::StopDemo) StopDemo,
            [N_CLEARDEMOS] clear_demos($crate::sauer_proto::channel1::ClearDemos) ClearDemos,
            [N_LISTDEMOS] list_demos($crate::sauer_proto::channel1::ListDemos) ListDemos,
            [N_GETDEMO] get_demo($crate::sauer_proto::channel1::GetDemo) GetDemo,
            [N_GETMAP] get_map($crate::sauer_proto::channel1::GetMap) GetMap,
            [N_NEWMAP] new_map($crate::sauer_proto::channel1::NewMap) NewMap,
            [N_SETMASTER] set_master($crate::sauer_proto::channel1::SetMaster) SetMaster,
            [N_ADDBOT] add_bot($crate::sauer_proto::channel1::AddBot) AddBot,
            [N_DELBOT] del_bot($crate::sauer_proto::channel1::DelBot) DelBot,
            [N_BOTLIMIT] bot_limit($crate::sauer_proto::channel1::BotLimit) BotLimit,
            [N_BOTBALANCE] bot_balance($crate::sauer_proto::channel1::BotBalance) BotBalance,
            [N_AUTHTRY] auth_try($crate::sauer_proto::channel1::AuthTry) AuthTry,
            [N_AUTHKICK] auth_kick($crate::sauer_proto::channel1::AuthKick) AuthKick,
            [N_PAUSEGAME] pause_game($crate::sauer_proto::channel1::PauseGameClient) PauseGameClient,
            [N_GAMESPEED] game_speed($crate::sauer_proto::channel1::GameSpeedClient) GameSpeedClient,
            [N_COPY] copy($crate::sauer_proto::channel1::CopyClient) CopyClient,
            [N_PASTE] paste($crate::sauer_proto::channel1::PasteClient) PasteClient,
            [N_CLIPBOARD] clipboard($crate::sauer_proto::channel1::ClipboardClient) ClipboardClient,
            [N_UNDO] undo($crate::sauer_proto::channel1::UndoClient) UndoClient,
            [N_REDO] redo($crate::sauer_proto::channel1::RedoClient) RedoClient,
            [N_EDITF] edit_face($crate::sauer_proto::channel1::EditFace) EditFace,
            [N_EDITT] edit_tex($crate::sauer_proto::channel1::EditTex) EditTex,
            [N_EDITM] edit_mat($crate::sauer_proto::channel1::EditMat) EditMat,
            [N_FLIP] flip($crate::sauer_proto::channel1::Flip) Flip,
            [N_ROTATE] rotate($crate::sauer_proto::channel1::Rotate) Rotate,
            [N_REPLACE] replace($crate::sauer_proto::channel1::Replace) Replace,
            [N_DELCUBE] del_cube($crate::sauer_proto::channel1::DelCube) DelCube,
            [N_EDITVSLOT] edit_v_slot($crate::sauer_proto::channel1::EditVSlot) EditVSlot,
            [N_REMIP] remip($crate::sauer_proto::channel1::Remip) Remip,
            [N_SERVCMD] serv_cmd($crate::sauer_proto::channel1::ServCmd) ServCmd,
            [N_BASES] bases($crate::sauer_proto::channel1::BasesClient) BasesClient,
            [N_REPAMMO] rep_ammo($crate::sauer_proto::channel1::RepAmmoClient) RepAmmoClient,
            [N_TRYDROPFLAG] try_drop_flag($crate::sauer_proto::channel1::TryDropFlag) TryDropFlag,
            [N_TAKEFLAG] take_flag($crate::sauer_proto::channel1::TakeFlagClient) TakeFlagClient,
            [N_INITFLAGS] init_flags($crate::sauer_proto::channel1::InitFlagsClient) InitFlagsClient,
            [N_INITTOKENS] init_tokens($crate::sauer_proto::channel1::InitTokensClient) InitTokensClient,
            [N_TAKETOKEN] take_token($crate::sauer_proto::channel1::TakeTokenClient) TakeTokenClient,
            [N_DEPOSITTOKENS] deposit_tokens($crate::sauer_proto::channel1::DepositTokensClient) DepositTokensClient,
            [N_TAUNT] taunt($crate::sauer_proto::channel1::Taunt) Taunt,
            }
        }
    };
}

macro_rules! packet_hook_decls {
    (
        ()
        chan0 { $($c0:tt)* }
        chan1 { $($c1:tt)* }
    ) => {
        packet_hook_decls! { @each $($c0)* $($c1)* }
    };
    (@each $( [$msg_id:ident] $method:ident ($packet:path) $variant:ident, )*) => {
        $(
            #[doc = concat!("`", stringify!($msg_id), "` from a client.")]
            fn $method(
                &self,
                ctx: &mut Context,
                packet: &$packet,
            ) -> impl Future<Output = Flow> + Send {
                async { Flow::Continue }
            }
        )*
    };
}

pub trait Plugin: Send + Sync + 'static {
    #![allow(unused)]

    /// On server initialization
    fn init(&self, server: &mut Server) -> impl Future<Output = ()> + Send {
        async {}
    }
    /// On server start
    fn start(&self, server: &Arc<Server>) -> impl Future<Output = ()> + Send {
        async {}
    }

    /// On UDP request (standard server info ping or extinfo)
    fn server_query(
        &self,
        ctx: &mut ServerQueryContext,
        request: &ServerQueryRequest,
    ) -> impl Future<Output = Flow> + Send {
        async { Flow::Continue }
    }

    /// On new peer connection. Cancelling disconnects the peer right away.
    fn connected(&self, ctx: &mut Context) -> impl Future<Output = Flow> + Send {
        async { Flow::Continue }
    }
    /// On peer disconnection. Every plugin gets it, since it's where cleanup happens.
    fn disconnected(&self, ctx: &mut Context) -> impl Future<Output = ()> + Send {
        async {}
    }

    /// Fired at the end of the welcome sequence, after the client has received
    /// `N_WELCOME`, `N_MAPCHANGE`, `N_CURRENTMASTER`, and `N_SPAWNSTATE`.
    /// Safe to send custom map data or other state-dependent packets here.
    fn on_client_welcomed(&self, ctx: &mut Context) -> impl Future<Output = ()> + Send {
        async {}
    }

    /// Fired after a map change is committed (vote passed or master forced it).
    /// `ctx.id()` is the client who cast the deciding vote.
    fn map_changed(
        &self,
        ctx: &mut Context,
        map_name: &CubeString,
        mode: GameMode,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }

    /// Fired when a player dies (e.g. from combat, falling into a trap, or suicide).
    fn on_player_died(
        &self,
        ctx: &mut Context,
        victim_id: u64,
        actor_id: u64,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }

    /// Fired when a player is about to be damaged.
    /// Modify `damage` to change the amount, or cancel to prevent it.
    fn on_damage(
        &self,
        ctx: &mut Context,
        target_id: u64,
        actor_id: u64,
        damage: &mut i32,
    ) -> impl Future<Output = Flow> + Send {
        async { Flow::Continue }
    }

    /// Fired when a player is about to spawn (in response to `N_TRYSPAWN`).
    /// `state` holds the default spawn stats; modify it to customise the loadout.
    /// Cancel to prevent the spawn entirely (the client stays dead).
    fn on_spawn(
        &self,
        ctx: &mut Context,
        state: &mut PlayerState,
    ) -> impl Future<Output = Flow> + Send {
        async { Flow::Continue }
    }

    /// Fired when a player sends a chat message (not a `#`-command).
    /// `text` holds the message and may be modified in-place before relay.
    /// Cancel to suppress relaying the message entirely.
    fn on_chat(
        &self,
        ctx: &mut Context,
        text: &mut CubeString,
    ) -> impl Future<Output = Flow> + Send {
        async { Flow::Continue }
    }

    /// Fired when a player sends a team chat message that will reach their
    /// teammates. `text` may be modified in-place before it is sent.
    /// Cancel to drop it.
    fn on_team_chat(
        &self,
        ctx: &mut Context,
        text: &mut CubeString,
    ) -> impl Future<Output = Flow> + Send {
        async { Flow::Continue }
    }

    /// Fired when a player sends a `#`-prefixed command.
    /// `args[0]` is the command name; remaining elements are the arguments.
    /// Quoted arguments have their outer `"` stripped.
    /// Only fires for commands added with `Server::register_command`; unknown ones get an error reply.
    fn on_command(
        &self,
        ctx: &mut Context,
        args: &[CubeString],
    ) -> impl Future<Output = Flow> + Send {
        async { Flow::Continue }
    }

    /// `CHANNEL_2` (Maps) raw map data from the client
    fn map(&self, ctx: &mut Context, packet: &Bytes) -> impl Future<Output = Flow> + Send {
        async { Flow::Continue }
    }

    crate::for_each_packet_hook!(packet_hook_decls);
}

/// Runs a hook on the first plugin, then on the second unless the first cancelled.
macro_rules! chain {
    ($self:ident . $method:ident ( $($arg:expr),* )) => {{
        if $self.0.$method($($arg),*).await == Flow::Cancel {
            return Flow::Cancel;
        }
        $self.1.$method($($arg),*).await
    }};
}

macro_rules! packet_hook_chain {
    (
        ()
        chan0 { $($c0:tt)* }
        chan1 { $($c1:tt)* }
    ) => {
        packet_hook_chain! { @each $($c0)* $($c1)* }
    };
    (@each $( [$msg_id:ident] $method:ident ($packet:path) $variant:ident, )*) => {
        $(
            async fn $method(&self, ctx: &mut Context, packet: &$packet) -> Flow {
                chain!(self.$method(ctx, packet))
            }
        )*
    };
}

impl Plugin for () {}

impl<P1: Plugin, P2: Plugin> Plugin for (P1, P2) {
    async fn init(&self, server: &mut Server) {
        self.0.init(server).await;
        self.1.init(server).await;
    }
    async fn start(&self, server: &Arc<Server>) {
        self.0.start(server).await;
        self.1.start(server).await;
    }

    async fn server_query(
        &self,
        ctx: &mut ServerQueryContext,
        request: &ServerQueryRequest,
    ) -> Flow {
        chain!(self.server_query(ctx, request))
    }
    async fn connected(&self, ctx: &mut Context) -> Flow {
        chain!(self.connected(ctx))
    }
    async fn disconnected(&self, ctx: &mut Context) {
        self.0.disconnected(ctx).await;
        self.1.disconnected(ctx).await;
    }

    async fn on_client_welcomed(&self, ctx: &mut Context) {
        self.0.on_client_welcomed(ctx).await;
        self.1.on_client_welcomed(ctx).await;
    }

    async fn map_changed(&self, ctx: &mut Context, map_name: &CubeString, mode: GameMode) {
        self.0.map_changed(ctx, map_name, mode).await;
        self.1.map_changed(ctx, map_name, mode).await;
    }

    async fn on_player_died(&self, ctx: &mut Context, victim_id: u64, actor_id: u64) {
        self.0.on_player_died(ctx, victim_id, actor_id).await;
        self.1.on_player_died(ctx, victim_id, actor_id).await;
    }

    async fn on_damage(
        &self,
        ctx: &mut Context,
        target_id: u64,
        actor_id: u64,
        damage: &mut i32,
    ) -> Flow {
        chain!(self.on_damage(ctx, target_id, actor_id, damage))
    }

    async fn on_spawn(&self, ctx: &mut Context, state: &mut PlayerState) -> Flow {
        chain!(self.on_spawn(ctx, state))
    }

    async fn on_chat(&self, ctx: &mut Context, text: &mut CubeString) -> Flow {
        chain!(self.on_chat(ctx, text))
    }

    async fn on_team_chat(&self, ctx: &mut Context, text: &mut CubeString) -> Flow {
        chain!(self.on_team_chat(ctx, text))
    }

    async fn on_command(&self, ctx: &mut Context, args: &[CubeString]) -> Flow {
        chain!(self.on_command(ctx, args))
    }

    async fn map(&self, ctx: &mut Context, p: &Bytes) -> Flow {
        chain!(self.map(ctx, p))
    }

    crate::for_each_packet_hook!(packet_hook_chain);
}

macro_rules! packet_hook_option {
    (
        ()
        chan0 { $($c0:tt)* }
        chan1 { $($c1:tt)* }
    ) => {
        packet_hook_option! { @each $($c0)* $($c1)* }
    };
    (@each $( [$msg_id:ident] $method:ident ($packet:path) $variant:ident, )*) => {
        $(
            async fn $method(&self, ctx: &mut Context, packet: &$packet) -> Flow {
                match self {
                    Some(p) => p.$method(ctx, packet).await,
                    None => Flow::Continue,
                }
            }
        )*
    };
}

/// A plugin that may be turned off: `None` does nothing.
impl<P: Plugin> Plugin for Option<P> {
    async fn init(&self, server: &mut Server) {
        if let Some(p) = self {
            p.init(server).await;
        }
    }
    async fn start(&self, server: &Arc<Server>) {
        if let Some(p) = self {
            p.start(server).await;
        }
    }
    async fn server_query(
        &self,
        ctx: &mut ServerQueryContext,
        request: &ServerQueryRequest,
    ) -> Flow {
        match self {
            Some(p) => p.server_query(ctx, request).await,
            None => Flow::Continue,
        }
    }
    async fn connected(&self, ctx: &mut Context) -> Flow {
        match self {
            Some(p) => p.connected(ctx).await,
            None => Flow::Continue,
        }
    }
    async fn disconnected(&self, ctx: &mut Context) {
        if let Some(p) = self {
            p.disconnected(ctx).await;
        }
    }
    async fn on_client_welcomed(&self, ctx: &mut Context) {
        if let Some(p) = self {
            p.on_client_welcomed(ctx).await;
        }
    }
    async fn map_changed(&self, ctx: &mut Context, map_name: &CubeString, mode: GameMode) {
        if let Some(p) = self {
            p.map_changed(ctx, map_name, mode).await;
        }
    }
    async fn on_player_died(&self, ctx: &mut Context, victim_id: u64, actor_id: u64) {
        if let Some(p) = self {
            p.on_player_died(ctx, victim_id, actor_id).await;
        }
    }
    async fn on_damage(
        &self,
        ctx: &mut Context,
        target_id: u64,
        actor_id: u64,
        damage: &mut i32,
    ) -> Flow {
        match self {
            Some(p) => p.on_damage(ctx, target_id, actor_id, damage).await,
            None => Flow::Continue,
        }
    }
    async fn on_spawn(&self, ctx: &mut Context, state: &mut PlayerState) -> Flow {
        match self {
            Some(p) => p.on_spawn(ctx, state).await,
            None => Flow::Continue,
        }
    }
    async fn on_chat(&self, ctx: &mut Context, text: &mut CubeString) -> Flow {
        match self {
            Some(p) => p.on_chat(ctx, text).await,
            None => Flow::Continue,
        }
    }
    async fn on_team_chat(&self, ctx: &mut Context, text: &mut CubeString) -> Flow {
        match self {
            Some(p) => p.on_team_chat(ctx, text).await,
            None => Flow::Continue,
        }
    }
    async fn on_command(&self, ctx: &mut Context, args: &[CubeString]) -> Flow {
        match self {
            Some(p) => p.on_command(ctx, args).await,
            None => Flow::Continue,
        }
    }
    async fn map(&self, ctx: &mut Context, data: &Bytes) -> Flow {
        match self {
            Some(p) => p.map(ctx, data).await,
            None => Flow::Continue,
        }
    }

    crate::for_each_packet_hook!(packet_hook_option);
}
