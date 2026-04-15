#![allow(private_interfaces)]

use crate::data;
use crate::data::*;
use cube_string::CubeString;

// =====================================================================
// CHANNEL 1: GAME STATE AND EVENTS (Reliable)
// =====================================================================

mod decode_client;
mod decode_server;
mod encode_client;
mod encode_server;

macro_rules! impl_from {
    ($struct:ident, $enum:ident) => {
        impl From<$struct> for $enum {
            fn from(v: $struct) -> Self {
                Self::$struct(v)
            }
        }
    };
}

macro_rules! impl_from_variant {
    ($struct:ident, $enum:ident, $variant:ident) => {
        impl From<$struct> for $enum {
            fn from(v: $struct) -> Self {
                Self::$variant(v)
            }
        }
    };
}

macro_rules! impl_client_packet {
    ($struct:ident, $variant:ident) => {
        impl From<$struct> for ClientPacket {
            fn from(v: $struct) -> Self {
                Self::$variant(v)
            }
        }
    };
}

// =====================================================================
// SHARED STRUCTS (Server & Client identical)
// =====================================================================

/// `N_SOUND` Plays a sound.
#[derive(Debug, Clone, PartialEq)]
pub struct Sound {
    pub sound_id: i32,
}
impl_client_packet!(Sound, Sound);
impl_from!(Sound, Chan1Client);

/// `N_TEXT` Emits a chat message.
#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    pub text: CubeString,
}
impl_client_packet!(Text, Text);
impl_from!(Text, Chan1Client);

/// `N_SWITCHNAME` Requests/Informs name change.
#[derive(Debug, Clone, PartialEq)]
pub struct SwitchName {
    pub name: CubeString,
}
impl_client_packet!(SwitchName, SwitchName);
impl_from!(SwitchName, Chan1Client);

/// `N_SWITCHMODEL` Requests/Informs model change.
#[derive(Debug, Clone, PartialEq)]
pub struct SwitchModel {
    pub model: PlayerModel,
}
impl_client_packet!(SwitchModel, SwitchModel);
impl_from!(SwitchModel, Chan1Client);

/// `N_GUNSELECT` Acknowledges or indicates a player changed their current weapon selection.
#[derive(Debug, Clone, PartialEq)]
pub struct GunSelect {
    pub gun: Gun,
}
impl_client_packet!(GunSelect, GunSelect);
impl_from!(GunSelect, Chan1Client);

/// `N_EDITVAR` Coop Edit: A map variable was modified.
#[derive(Debug, Clone, PartialEq)]
pub struct EditVar {
    pub name: CubeString,
    pub val: EditVarValue,
}
impl_client_packet!(EditVar, EditVar);
impl_from!(EditVar, Chan1Client);

/// `N_CLIENTPING` Reports/Updates the scoreboard ping value.
#[derive(Debug, Clone, PartialEq)]
pub struct ClientPing {
    pub ping: i32,
}
impl_client_packet!(ClientPing, ClientPing);
impl_from!(ClientPing, Chan1Client);

/// `N_EDITF` Coop Edit: Face Modification.
#[derive(Debug, Clone, PartialEq)]
pub struct EditFace {
    pub sel: SelInfo,
    pub dir: i32,
    pub mode: EditFaceMode,
}
impl_client_packet!(EditFace, EditFace);
impl_from!(EditFace, Chan1Client);

/// `N_EDITT` Coop Edit: Texture Modification.
#[derive(Debug, Clone, PartialEq)]
pub struct EditTex {
    pub sel: SelInfo,
    pub tex: i32,
    pub all_faces: bool,
    pub extra_data: Vec<u8>,
}
impl_client_packet!(EditTex, EditTex);
impl_from!(EditTex, Chan1Client);

/// `N_EDITM` Coop Edit: Material Modification.
#[derive(Debug, Clone, PartialEq)]
pub struct EditMat {
    pub sel: SelInfo,
    pub mat: i32,
    pub filter: i32,
}
impl_client_packet!(EditMat, EditMat);
impl_from!(EditMat, Chan1Client);

/// `N_FLIP` Coop Edit: Flip geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct Flip {
    pub sel: SelInfo,
}
impl_client_packet!(Flip, Flip);
impl_from!(Flip, Chan1Client);

/// `N_ROTATE` Coop Edit: Rotate geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct Rotate {
    pub sel: SelInfo,
    pub dir: i32,
}
impl_client_packet!(Rotate, Rotate);
impl_from!(Rotate, Chan1Client);

/// `N_REPLACE` Coop Edit: Replace texture.
#[derive(Debug, Clone, PartialEq)]
pub struct Replace {
    pub sel: SelInfo,
    pub old_tex: i32,
    pub new_tex: i32,
    pub in_sel: i32,
    pub extra_data: Vec<u8>,
}
impl_client_packet!(Replace, Replace);
impl_from!(Replace, Chan1Client);

/// `N_DELCUBE` Coop Edit: Delete a cube.
#[derive(Debug, Clone, PartialEq)]
pub struct DelCube {
    pub sel: SelInfo,
}
impl_client_packet!(DelCube, DelCube);
impl_from!(DelCube, Chan1Client);

/// `N_EDITVSLOT` Coop Edit: Modify texture V-Slots.
#[derive(Debug, Clone, PartialEq)]
pub struct EditVSlot {
    pub sel: SelInfo,
    pub delta: i32,
    pub all_faces: bool,
    pub extra_data: Vec<u8>,
}
impl_client_packet!(EditVSlot, EditVSlot);
impl_from!(EditVSlot, Chan1Client);

/// `N_REMIP` Coop Edit: Lighting/mips recalculation.
#[derive(Debug, Clone, PartialEq)]
pub struct Remip;
impl_client_packet!(Remip, Remip);
impl_from!(Remip, Chan1Client);

/// `N_NEWMAP` Generates a new empty map of the specified size.
#[derive(Debug, Clone, PartialEq)]
pub struct NewMap {
    pub size: i32,
}
impl_client_packet!(NewMap, NewMap);
impl_from!(NewMap, Chan1Client);

/// `N_ITEMLIST` Bulk sends the items available on a map (synchronization).
#[derive(Debug, Clone, PartialEq)]
pub struct ItemList {
    pub items: Vec<data::ItemSpawn>,
}
impl_from!(ItemList, Chan1Server);
impl_from!(ItemList, Chan1Client);

/// `N_SERVCMD` Raw server command.
#[derive(Debug, Clone, PartialEq)]
pub struct ServCmd {
    pub cmd: CubeString,
}
impl_from!(ServCmd, Chan1Server);
impl_from!(ServCmd, Chan1Client);

// =====================================================================
// SERVER-SPECIFIC STRUCTS
// =====================================================================

/// `N_CLIENT` Wrapper
#[derive(Debug, Clone, PartialEq)]
pub struct Client {
    pub client_num: i32,
    pub packet: ClientPacket,
}
impl_from!(Client, Chan1Server);

/// `N_DEMOPACKET` Demo playback marker.
#[derive(Debug, Clone, PartialEq)]
pub struct DemoPacket;
impl_from!(DemoPacket, Chan1Server);

/// `N_SERVINFO` Initial server information sent when a client starts connecting.
#[derive(Debug, Clone, PartialEq)]
pub struct ServInfo {
    pub client_num: i32,
    pub protocol: i32,
    pub session_id: i32,
    pub has_password: bool,
    pub server_desc: CubeString,
    pub server_auth: CubeString,
}
impl_from!(ServInfo, Chan1Server);

/// `N_WELCOME` The client is fully welcomed and joined into the game.
#[derive(Debug, Clone, PartialEq)]
pub struct Welcome;
impl_from!(Welcome, Chan1Server);

/// `N_PAUSEGAME` Sets or removes the paused state of the game.
#[derive(Debug, Clone, PartialEq)]
pub struct PauseGameServer {
    pub is_paused: bool,
    pub client_num: i32,
}
impl_from_variant!(PauseGameServer, Chan1Server, PauseGame);

/// `N_GAMESPEED` Modifies the speed of the game.
#[derive(Debug, Clone, PartialEq)]
pub struct GameSpeedServer {
    pub speed: i32,
    pub client_num: i32,
}
impl_from_variant!(GameSpeedServer, Chan1Server, GameSpeed);

/// `N_SAYTEAM` Emits a team chat message.
#[derive(Debug, Clone, PartialEq)]
pub struct SayTeamServer {
    pub client_num: i32,
    pub text: CubeString,
}
impl_from_variant!(SayTeamServer, Chan1Server, SayTeam);

/// `N_MAPCHANGE` Notifies clients to change the current map and gamemode.
#[derive(Debug, Clone, PartialEq)]
pub struct MapChange {
    pub map_name: CubeString,
    pub mode: GameMode,
    pub has_items: bool,
}
impl_from!(MapChange, Chan1Server);

/// `N_FORCEDEATH` Forces a client to die instantly.
#[derive(Debug, Clone, PartialEq)]
pub struct ForceDeath {
    pub client_num: i32,
}
impl_from!(ForceDeath, Chan1Server);

/// `N_INITCLIENT` A new client has connected.
#[derive(Debug, Clone, PartialEq)]
pub struct InitClient {
    pub client_num: i32,
    pub name: CubeString,
    pub team: CubeString,
    pub player_model: PlayerModel,
}
impl_from!(InitClient, Chan1Server);

/// `N_CDIS` A client has fully disconnected.
#[derive(Debug, Clone, PartialEq)]
pub struct ClientDisconnected {
    pub client_num: i32,
}
impl_from!(ClientDisconnected, Chan1Server);

/// `N_SPAWNSTATE` Confirms the specific state (health, ammo, etc.) of a client who just spawned.
#[derive(Debug, Clone, PartialEq)]
pub struct SpawnState {
    pub client_num: i32,
    pub state: PlayerState,
}
impl_from!(SpawnState, Chan1Server);

/// `N_SHOTFX` Replicates weapon firing effects across clients.
#[derive(Debug, Clone, PartialEq)]
pub struct ShotFx {
    pub client_num: i32,
    pub gun: Gun,
    pub id: i32,
    pub from: Vector3<f32>,
    pub to: Vector3<f32>,
}
impl_from!(ShotFx, Chan1Server);

/// `N_EXPLODEFX` Replicates projectile explosion effects across clients.
#[derive(Debug, Clone, PartialEq)]
pub struct ExplodeFx {
    pub client_num: i32,
    pub gun: Gun,
    pub id: i32,
}
impl_from!(ExplodeFx, Chan1Server);

/// `N_DAMAGE` Applies damage to a target client.
#[derive(Debug, Clone, PartialEq)]
pub struct Damage {
    pub target_cn: i32,
    pub actor_cn: i32,
    pub damage: i32,
    pub armour: i32,
    pub health: i32,
}
impl_from!(Damage, Chan1Server);

/// `N_HITPUSH` Applies physical knockback from weapon hits to a client.
#[derive(Debug, Clone, PartialEq)]
pub struct HitPush {
    pub target_cn: i32,
    pub gun: Gun,
    pub damage: i32,
    pub dir: Vector3<f32>,
}
impl_from!(HitPush, Chan1Server);

/// `N_DIED` Notifies that a player was killed by an actor.
#[derive(Debug, Clone, PartialEq)]
pub struct Died {
    pub victim_cn: i32,
    pub actor_cn: i32,
    pub actor_frags: i32,
    pub team_frags: i32,
}
impl_from!(Died, Chan1Server);

/// `N_TEAMINFO` Transmits team scores (frags).
#[derive(Debug, Clone, PartialEq)]
pub struct TeamInfo {
    pub teams: Vec<crate::data::TeamInfo>,
}
impl_from!(TeamInfo, Chan1Server);

/// `N_RESUME` Sent during a late join to tell the connecting client about the state of all players.
#[derive(Debug, Clone, PartialEq)]
pub struct Resume {
    pub clients: Vec<ClientResumeState>,
}
impl_from!(Resume, Chan1Server);

/// `N_ITEMSPAWN` Notifies that an item has spawned on the map.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemSpawn {
    pub item_index: i32,
}
impl_from!(ItemSpawn, Chan1Server);

/// `N_ITEMACC` Acknowledges an item pickup.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemAck {
    pub item_index: i32,
    pub client_num: i32,
}
impl_from!(ItemAck, Chan1Server);

/// `N_CLIPBOARD` Pushes map clipboard data (for cooperative editing) to the client.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipboardServer {
    pub client_num: i32,
    pub unpack_len: i32,
    pub pack_len: i32,
    pub data: Vec<u8>,
}
impl_from_variant!(ClipboardServer, Chan1Server, Clipboard);

/// `N_UNDO` Sends geometry undo data to the client.
#[derive(Debug, Clone, PartialEq)]
pub struct UndoServer {
    pub client_num: i32,
    pub unpack_len: i32,
    pub pack_len: i32,
    pub data: Vec<u8>,
}
impl_from_variant!(UndoServer, Chan1Server, Undo);

/// `N_REDO` Sends geometry redo data to the client.
#[derive(Debug, Clone, PartialEq)]
pub struct RedoServer {
    pub client_num: i32,
    pub unpack_len: i32,
    pub pack_len: i32,
    pub data: Vec<u8>,
}
impl_from_variant!(RedoServer, Chan1Server, Redo);

/// `N_PONG` Basic ping packet sent to measure round-trip time.
#[derive(Debug, Clone, PartialEq)]
pub struct Pong {
    pub ping: i32,
}
impl_from!(Pong, Chan1Server);

/// `N_TIMEUP` Indicates that the current match time limit has been updated.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeUp {
    pub seconds_remaining: i32,
}
impl_from!(TimeUp, Chan1Server);

/// `N_SERVMSG` Displays a direct system/server message in the client's console.
#[derive(Debug, Clone, PartialEq)]
pub struct ServMsg {
    pub text: CubeString,
}
impl_from!(ServMsg, Chan1Server);

/// `N_SENDDEMOLIST` Replicates the list of available demos back to the client.
#[derive(Debug, Clone, PartialEq)]
pub struct SendDemoList {
    pub demos: Vec<CubeString>,
}
impl_from!(SendDemoList, Chan1Server);

/// `N_DEMOPLAYBACK` Informs the client that the server has started or stopped demo playback.
#[derive(Debug, Clone, PartialEq)]
pub struct DemoPlayback {
    pub is_on: bool,
    pub client_num: i32,
}
impl_from!(DemoPlayback, Chan1Server);

/// `N_CURRENTMASTER` Notifies clients of the current master mode and list of clients with privileges.
#[derive(Debug, Clone, PartialEq)]
pub struct CurrentMaster {
    pub master_mode: MasterMode,
    pub clients: Vec<MasterClient>,
}
impl_from!(CurrentMaster, Chan1Server);

/// `N_MASTERMODE` Updates the global Master Mode.
#[derive(Debug, Clone, PartialEq)]
pub struct MasterModeServer {
    pub master_mode: MasterMode,
}
impl_from_variant!(MasterModeServer, Chan1Server, MasterMode);

/// `N_SPECTATOR` Forces a player into or out of Spectator Mode.
#[derive(Debug, Clone, PartialEq)]
pub struct SpectatorServer {
    pub client_num: i32,
    pub is_spectator: bool,
}
impl_from_variant!(SpectatorServer, Chan1Server, Spectator);

/// `N_SETTEAM` Moves a player to a specific team.
#[derive(Debug, Clone, PartialEq)]
pub struct SetTeamServer {
    pub client_num: i32,
    pub team: CubeString,
    pub reason: i32,
}
impl_from_variant!(SetTeamServer, Chan1Server, SetTeam);

/// `N_ANNOUNCE` Server announces important item spawns natively (e.g., Quad Damage).
#[derive(Debug, Clone, PartialEq)]
pub struct Announce {
    pub item_type: ItemType,
}
impl_from!(Announce, Chan1Server);

/// `N_REQAUTH` Initiates an authentication request from the server.
#[derive(Debug, Clone, PartialEq)]
pub struct ReqAuth {
    pub auth_domain: CubeString,
}
impl_from!(ReqAuth, Chan1Server);

/// `N_AUTHCHAL` Sends the cryptographic challenge down to the client.
#[derive(Debug, Clone, PartialEq)]
pub struct AuthChal {
    pub desc: CubeString,
    pub id: u32,
    pub challenge: CubeString,
}
impl_from!(AuthChal, Chan1Server);

/// `N_INITAI` Spawns a server-side AI Bot into the client's game world.
#[derive(Debug, Clone, PartialEq)]
pub struct InitAi {
    pub bot_cn: i32,
    pub owner_cn: i32,
    pub ai_type: AiType,
    pub skill: i32,
    pub player_model: PlayerModel,
    pub name: CubeString,
    pub team: CubeString,
}
impl_from!(InitAi, Chan1Server);

/// `N_BASES` Sends the list of capture bases for the current map (Capture / Regen Capture).
#[derive(Debug, Clone, PartialEq)]
pub struct BasesServer {
    pub bases: Vec<data::BaseInfo>,
}
impl_from_variant!(BasesServer, Chan1Server, Bases);

/// `N_BASEINFO` Updates the state of a single capture base.
#[derive(Debug, Clone, PartialEq)]
pub struct BaseInfo {
    pub base: i32,
    pub owner: CubeString,
    pub enemy: CubeString,
    pub converted: bool,
    pub ammo: i32,
}
impl_from!(BaseInfo, Chan1Server);

/// `N_BASESCORE` Updates the score for a team on a capture base.
#[derive(Debug, Clone, PartialEq)]
pub struct BaseScore {
    pub base: i32,
    pub team: CubeString,
    pub total: i32,
}
impl_from!(BaseScore, Chan1Server);

/// `N_REPAMMO` Replenishes a player's ammo (Regen Capture).
#[derive(Debug, Clone, PartialEq)]
pub struct RepAmmoServer {
    pub client_num: i32,
    pub ammotype: ItemType,
}
impl_from_variant!(RepAmmoServer, Chan1Server, RepAmmo);

/// `N_BASEREGEN` Regenerates a player's health and armour at a capture base.
#[derive(Debug, Clone, PartialEq)]
pub struct BaseRegen {
    pub client_num: i32,
    pub health: i32,
    pub armour: i32,
    pub ammotype: ItemType,
    pub ammo: i32,
}
impl_from!(BaseRegen, Chan1Server);

/// `N_INITFLAGS` Sends initial flag states to joining clients (CTF modes).
#[derive(Debug, Clone, PartialEq)]
pub struct InitFlagsServer {
    pub scores: [i32; 2],
    pub flags: Vec<FlagState>,
}
impl_from_variant!(InitFlagsServer, Chan1Server, InitFlags);

/// `N_DROPFLAG` A player dropped a flag.
#[derive(Debug, Clone, PartialEq)]
pub struct DropFlag {
    pub client_num: i32,
    pub flag: i32,
    pub version: i32,
    pub drop_loc: Vector3<f32>,
}
impl_from!(DropFlag, Chan1Server);

/// `N_SCOREFLAG` A player scored with a flag.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoreFlag {
    pub client_num: i32,
    pub relay_flag: i32,
    pub relay_version: i32,
    pub goal_flag: i32,
    pub goal_version: i32,
    pub goal_spawn: i32,
    pub team: i32,
    pub score: i32,
    pub oflags: i32,
}
impl_from!(ScoreFlag, Chan1Server);

/// `N_RETURNFLAG` A player returned a flag to its base.
#[derive(Debug, Clone, PartialEq)]
pub struct ReturnFlag {
    pub client_num: i32,
    pub flag: i32,
    pub version: i32,
}
impl_from!(ReturnFlag, Chan1Server);

/// `N_TAKEFLAG` A player picked up a flag.
#[derive(Debug, Clone, PartialEq)]
pub struct TakeFlagServer {
    pub client_num: i32,
    pub flag: i32,
    pub version: i32,
}
impl_from_variant!(TakeFlagServer, Chan1Server, TakeFlag);

/// `N_RESETFLAG` A flag was reset to its spawn location.
#[derive(Debug, Clone, PartialEq)]
pub struct ResetFlag {
    pub flag: i32,
    pub version: i32,
    pub spawn_index: i32,
    pub team: i32,
    pub score: i32,
}
impl_from!(ResetFlag, Chan1Server);

/// `N_INVISFLAG` Marks a flag as invisible or visible.
#[derive(Debug, Clone, PartialEq)]
pub struct InvisFlag {
    pub flag: i32,
    pub invis: bool,
}
impl_from!(InvisFlag, Chan1Server);

/// `N_INITTOKENS` Sends initial token states to joining clients (Collect mode).
#[derive(Debug, Clone, PartialEq)]
pub struct InitTokensServer {
    pub scores: [i32; 2],
    pub tokens: Vec<TokenState>,
    pub client_tokens: Vec<ClientTokenState>,
}
impl_from_variant!(InitTokensServer, Chan1Server, InitTokens);

/// `N_TAKETOKEN` A player collected a token.
#[derive(Debug, Clone, PartialEq)]
pub struct TakeTokenServer {
    pub client_num: i32,
    pub id: i32,
    pub total: i32,
}
impl_from_variant!(TakeTokenServer, Chan1Server, TakeToken);

/// `N_EXPIRETOKENS` Notifies that specific tokens have expired.
#[derive(Debug, Clone, PartialEq)]
pub struct ExpireTokens {
    pub tokens: Vec<i32>,
}
impl_from!(ExpireTokens, Chan1Server);

/// `N_DROPTOKENS` A player dropped their held tokens.
#[derive(Debug, Clone, PartialEq)]
pub struct DropTokens {
    pub client_num: i32,
    pub drop_loc: Vector3<f32>,
    pub tokens: Vec<DroppedToken>,
}
impl_from!(DropTokens, Chan1Server);

/// `N_STEALTOKENS` A player stole tokens from the enemy team.
#[derive(Debug, Clone, PartialEq)]
pub struct StealTokens {
    pub client_num: i32,
    pub team: i32,
    pub basenum: i32,
    pub enemy_team: i32,
    pub score: i32,
    pub drop_loc: Vector3<f32>,
    pub tokens: Vec<StolenToken>,
}
impl_from!(StealTokens, Chan1Server);

/// `N_DEPOSITTOKENS` A player deposited tokens at a base.
#[derive(Debug, Clone, PartialEq)]
pub struct DepositTokensServer {
    pub client_num: i32,
    pub base: i32,
    pub deposited: i32,
    pub team: i32,
    pub score: i32,
    pub flags: i32,
}
impl_from_variant!(DepositTokensServer, Chan1Server, DepositTokens);

// =====================================================================
// CLIENT-PACKET ONLY STRUCTS (Wrapped in N_CLIENT)
// =====================================================================

/// `N_SPAWN` Sent to instruct a client to spawn in the game.
#[derive(Debug, Clone, PartialEq)]
pub struct SpawnServer {
    pub state: PlayerState,
}
impl_client_packet!(SpawnServer, Spawn);

/// `N_TAUNT` Indicates that a player is taunting.
#[derive(Debug, Clone, PartialEq)]
pub struct Taunt;
impl_client_packet!(Taunt, Taunt);

/// `N_EDITENT` Coop Edit: Modification of map entities.
#[derive(Debug, Clone, PartialEq)]
pub struct EditEntServer {
    pub ent_id: i32,
    pub pos: Vector3<f32>,
    pub type_: EntityType,
    pub attr1: i32,
    pub attr2: i32,
    pub attr3: i32,
    pub attr4: i32,
    pub attr5: i32,
}
impl_client_packet!(EditEntServer, EditEnt);

/// `N_EDITMODE` Notifies that a player entered or left Edit Mode.
#[derive(Debug, Clone, PartialEq)]
pub struct EditModeServer {
    pub is_editing: bool,
}
impl_client_packet!(EditModeServer, EditMode);

/// `N_COPY` Coop Edit: Copy geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct CopyServer {
    pub sel: SelInfo,
}
impl_client_packet!(CopyServer, Copy);

/// `N_PASTE` Coop Edit: Paste geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct PasteServer {
    pub sel: SelInfo,
}
impl_client_packet!(PasteServer, Paste);

// =====================================================================
// CLIENT-SPECIFIC STRUCTS
// =====================================================================

/// `N_CONNECT` Attempts to connect to the server with user details and passwords.
#[derive(Debug, Clone, PartialEq)]
pub struct Connect {
    pub name: CubeString,
    pub player_model: PlayerModel,
    pub password_hash: CubeString,
    pub auth_desc: CubeString,
    pub auth_name: CubeString,
}
impl_from!(Connect, Chan1Client);

/// `N_AUTHANS` Provides the mathematical answer to a server's AuthChal.
#[derive(Debug, Clone, PartialEq)]
pub struct AuthAnswer {
    pub desc: CubeString,
    pub id: u32,
    pub answer: CubeString,
}
impl_from!(AuthAnswer, Chan1Client);

/// `N_PING` Measures the roundtrip time.
#[derive(Debug, Clone, PartialEq)]
pub struct Ping {
    pub time: i32,
}
impl_from!(Ping, Chan1Client);

/// `N_FROMAI` Alters the context of subsequent messages in the packet to act as if they came from the specified AI bot.
#[derive(Debug, Clone, PartialEq)]
pub struct FromAi {
    pub qcn: i32,
}
impl_from!(FromAi, Chan1Client);

/// `N_EDITMODE` The client toggled cooperative edit mode locally.
#[derive(Debug, Clone, PartialEq)]
pub struct EditModeClient {
    pub val: bool,
}
impl_from_variant!(EditModeClient, Chan1Client, EditMode);

/// `N_MAPCRC` The client verifies the CRC of the map they currently have loaded.
#[derive(Debug, Clone, PartialEq)]
pub struct MapCrc {
    pub map_name: CubeString,
    pub crc: i32,
}
impl_from!(MapCrc, Chan1Client);

/// `N_CHECKMAPS` Forces a manual check of map CRCs on the server.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckMaps;
impl_from!(CheckMaps, Chan1Client);

/// `N_TRYSPAWN` Requests to spawn from the server.
#[derive(Debug, Clone, PartialEq)]
pub struct TrySpawn;
impl_from!(TrySpawn, Chan1Client);

/// `N_SPAWN` The client actually spawns.
#[derive(Debug, Clone, PartialEq)]
pub struct SpawnClient {
    pub life_sequence: i32,
    pub gun_select: Gun,
}
impl_from_variant!(SpawnClient, Chan1Client, Spawn);

/// `N_SUICIDE` The client forcefully kills themselves.
#[derive(Debug, Clone, PartialEq)]
pub struct Suicide;
impl_from!(Suicide, Chan1Client);

/// `N_SHOOT` The client fired a weapon.
#[derive(Debug, Clone, PartialEq)]
pub struct Shoot {
    pub id: i32,
    pub gun: Gun,
    pub from: Vector3<f32>,
    pub to: Vector3<f32>,
    pub hits: Vec<HitInfo>,
}
impl_from!(Shoot, Chan1Client);

/// `N_EXPLODE` Projectile explosion triggered by the client.
#[derive(Debug, Clone, PartialEq)]
pub struct Explode {
    pub millis: i32,
    pub gun: Gun,
    pub id: i32,
    pub hits: Vec<HitInfo>,
}
impl_from!(Explode, Chan1Client);

/// `N_ITEMPICKUP` The client requested to pick up an item.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemPickup {
    pub item_index: i32,
}
impl_from!(ItemPickup, Chan1Client);

/// `N_SAYTEAM` Submits a team chat message.
#[derive(Debug, Clone, PartialEq)]
pub struct SayTeamClient {
    pub text: CubeString,
}
impl_from_variant!(SayTeamClient, Chan1Client, SayTeam);

/// `N_SWITCHTEAM` The client requests to switch teams.
#[derive(Debug, Clone, PartialEq)]
pub struct SwitchTeam {
    pub team: CubeString,
}
impl_from!(SwitchTeam, Chan1Client);

/// `N_MAPVOTE` The client votes to change the map and mode.
#[derive(Debug, Clone, PartialEq)]
pub struct MapVote {
    pub map_name: CubeString,
    pub mode: GameMode,
}
impl_from!(MapVote, Chan1Client);

/// `N_EDITENT` Coop Edit: A map entity was modified.
#[derive(Debug, Clone, PartialEq)]
pub struct EditEntClient {
    pub ent_id: i32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub type_: EntityType,
    pub attr1: i32,
    pub attr2: i32,
    pub attr3: i32,
    pub attr4: i32,
    pub attr5: i32,
}
impl_from_variant!(EditEntClient, Chan1Client, EditEnt);

/// `N_MASTERMODE` Requests to change the global Master Mode.
#[derive(Debug, Clone, PartialEq)]
pub struct MasterModeClient {
    pub mode: MasterMode,
}
impl_from_variant!(MasterModeClient, Chan1Client, MasterMode);

/// `N_CLEARBANS` Requests to clear all bans.
#[derive(Debug, Clone, PartialEq)]
pub struct ClearBans;
impl_from!(ClearBans, Chan1Client);

/// `N_KICK` Requests to kick a player.
#[derive(Debug, Clone, PartialEq)]
pub struct Kick {
    pub victim_cn: i32,
    pub reason: CubeString,
}
impl_from!(Kick, Chan1Client);

/// `N_SPECTATOR` Requests to move someone (or self) to/from the spectator team.
#[derive(Debug, Clone, PartialEq)]
pub struct SpectatorClient {
    pub spectator_cn: i32,
    pub val: bool,
}
impl_from_variant!(SpectatorClient, Chan1Client, Spectator);

/// `N_SETTEAM` Requests to force someone to a specific team.
#[derive(Debug, Clone, PartialEq)]
pub struct SetTeamClient {
    pub client_num: i32,
    pub team: CubeString,
}
impl_from_variant!(SetTeamClient, Chan1Client, SetTeam);

/// `N_FORCEINTERMISSION` Requests to force the current match to end.
#[derive(Debug, Clone, PartialEq)]
pub struct ForceIntermission;
impl_from!(ForceIntermission, Chan1Client);

/// `N_RECORDDEMO` Toggles automatic demo recording for the next match.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordDemo {
    pub val: bool,
}
impl_from!(RecordDemo, Chan1Client);

/// `N_STOPDEMO` Stops the current demo recording.
#[derive(Debug, Clone, PartialEq)]
pub struct StopDemo;
impl_from!(StopDemo, Chan1Client);

/// `N_CLEARDEMOS` Clears a specific demo (or all) from server memory.
#[derive(Debug, Clone, PartialEq)]
pub struct ClearDemos {
    pub demo_id: i32,
}
impl_from!(ClearDemos, Chan1Client);

/// `N_LISTDEMOS` Requests a listing of all available demos on the server.
#[derive(Debug, Clone, PartialEq)]
pub struct ListDemos;
impl_from!(ListDemos, Chan1Client);

/// `N_GETDEMO` Requests to download a specific demo from the server.
#[derive(Debug, Clone, PartialEq)]
pub struct GetDemo {
    pub demo_id: i32,
    pub tag: i32,
}
impl_from!(GetDemo, Chan1Client);

/// `N_GETMAP` Requests to download the current map from the server (Coop Edit).
#[derive(Debug, Clone, PartialEq)]
pub struct GetMap;
impl_from!(GetMap, Chan1Client);

/// `N_SETMASTER` Requests to set master privileges using a password hash.
#[derive(Debug, Clone, PartialEq)]
pub struct SetMaster {
    pub client_num: i32,
    pub val: bool,
    pub hash: CubeString,
}
impl_from!(SetMaster, Chan1Client);

/// `N_ADDBOT` Requests to add a bot with specific skill.
#[derive(Debug, Clone, PartialEq)]
pub struct AddBot {
    pub skill: i32,
}
impl_from!(AddBot, Chan1Client);

/// `N_DELBOT` Requests to delete a bot.
#[derive(Debug, Clone, PartialEq)]
pub struct DelBot;
impl_from!(DelBot, Chan1Client);

/// `N_BOTLIMIT` Modifies the global bot limit.
#[derive(Debug, Clone, PartialEq)]
pub struct BotLimit {
    pub limit: i32,
}
impl_from!(BotLimit, Chan1Client);

/// `N_BOTBALANCE` Modifies the global bot auto-balance flag.
#[derive(Debug, Clone, PartialEq)]
pub struct BotBalance {
    pub balance: bool,
}
impl_from!(BotBalance, Chan1Client);

/// `N_AUTHTRY` Submits a request to authenticate and claim privileges via cryptographic keys.
#[derive(Debug, Clone, PartialEq)]
pub struct AuthTry {
    pub desc: CubeString,
    pub name: CubeString,
}
impl_from!(AuthTry, Chan1Client);

/// `N_AUTHKICK` Submits a request to kick an individual via cryptographic auth verification.
#[derive(Debug, Clone, PartialEq)]
pub struct AuthKick {
    pub desc: CubeString,
    pub name: CubeString,
    pub victim_cn: i32,
    pub reason: CubeString,
}
impl_from!(AuthKick, Chan1Client);

/// `N_PAUSEGAME` Toggles game paused state.
#[derive(Debug, Clone, PartialEq)]
pub struct PauseGameClient {
    pub val: bool,
}
impl_from_variant!(PauseGameClient, Chan1Client, PauseGame);

/// `N_GAMESPEED` Changes global game speed.
#[derive(Debug, Clone, PartialEq)]
pub struct GameSpeedClient {
    pub speed: i32,
}
impl_from_variant!(GameSpeedClient, Chan1Client, GameSpeed);

/// `N_COPY` Coop Edit: Client copies geometry to their clipboard.
#[derive(Debug, Clone, PartialEq)]
pub struct CopyClient;
impl_from_variant!(CopyClient, Chan1Client, Copy);

/// `N_PASTE` Coop Edit: Client requests to paste geometry from their clipboard.
#[derive(Debug, Clone, PartialEq)]
pub struct PasteClient;
impl_from_variant!(PasteClient, Chan1Client, Paste);

/// `N_CLIPBOARD` Coop Edit: Uploads raw clipboard geometry data.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipboardClient {
    pub unpack_len: i32,
    pub pack_len: i32,
    pub data: Vec<u8>,
}
impl_from_variant!(ClipboardClient, Chan1Client, Clipboard);

/// `N_UNDO` Coop Edit: Undo event.
#[derive(Debug, Clone, PartialEq)]
pub struct UndoClient {
    pub unpack_len: i32,
    pub pack_len: i32,
    pub data: Vec<u8>,
}
impl_from_variant!(UndoClient, Chan1Client, Undo);

/// `N_REDO` Coop Edit: Redo event.
#[derive(Debug, Clone, PartialEq)]
pub struct RedoClient {
    pub unpack_len: i32,
    pub pack_len: i32,
    pub data: Vec<u8>,
}
impl_from_variant!(RedoClient, Chan1Client, Redo);

/// `N_BASES` Sends the list of capture base spawns to the server (Capture / Regen Capture).
#[derive(Debug, Clone, PartialEq)]
pub struct BasesClient {
    pub bases: Vec<BaseSpawn>,
}
impl_from_variant!(BasesClient, Chan1Client, Bases);

/// `N_REPAMMO` Requests ammo replenishment (Regen Capture).
#[derive(Debug, Clone, PartialEq)]
pub struct RepAmmoClient;
impl_from_variant!(RepAmmoClient, Chan1Client, RepAmmo);

/// `N_TRYDROPFLAG` Client attempts to drop the flag they are carrying.
#[derive(Debug, Clone, PartialEq)]
pub struct TryDropFlag;
impl_from!(TryDropFlag, Chan1Client);

/// `N_TAKEFLAG` Client acknowledges picking up a flag.
#[derive(Debug, Clone, PartialEq)]
pub struct TakeFlagClient {
    pub flag: i32,
    pub version: i32,
}
impl_from_variant!(TakeFlagClient, Chan1Client, TakeFlag);

/// `N_INITFLAGS` Sends initial flag spawn positions to the server (CTF modes).
#[derive(Debug, Clone, PartialEq)]
pub struct InitFlagsClient {
    pub flags: Vec<FlagSpawn>,
}
impl_from_variant!(InitFlagsClient, Chan1Client, InitFlags);

/// `N_INITTOKENS` Sends initial token base spawn data to the server (Collect mode).
#[derive(Debug, Clone, PartialEq)]
pub struct InitTokensClient {
    pub bases: Vec<TokenBaseSpawn>,
}
impl_from_variant!(InitTokensClient, Chan1Client, InitTokens);

/// `N_TAKETOKEN` Client collected a token.
#[derive(Debug, Clone, PartialEq)]
pub struct TakeTokenClient {
    pub id: i32,
}
impl_from_variant!(TakeTokenClient, Chan1Client, TakeToken);

/// `N_DEPOSITTOKENS` Client deposited tokens at a base.
#[derive(Debug, Clone, PartialEq)]
pub struct DepositTokensClient {
    pub base: i32,
}
impl_from_variant!(DepositTokensClient, Chan1Client, DepositTokens);

// =====================================================================
// ENUMS
// =====================================================================

/// `N_CLIENT` wrapped messages
#[derive(Debug, Clone, PartialEq)]
pub enum ClientPacket {
    Sound(Sound),
    Text(Text),
    SwitchName(SwitchName),
    SwitchModel(SwitchModel),
    Spawn(SpawnServer),
    GunSelect(GunSelect),
    Taunt(Taunt),
    Remip(Remip),
    EditEnt(EditEntServer),
    EditVar(EditVar),
    ClientPing(ClientPing),
    EditMode(EditModeServer),
    EditFace(EditFace),
    EditTex(EditTex),
    EditMat(EditMat),
    Flip(Flip),
    Copy(CopyServer),
    Paste(PasteServer),
    Rotate(Rotate),
    Replace(Replace),
    DelCube(DelCube),
    EditVSlot(EditVSlot),
    NewMap(NewMap),
}

/// Server -> Client (Channel 1)
#[derive(Debug, Clone, PartialEq)]
pub enum Chan1Server {
    DemoPacket(DemoPacket),
    ServInfo(ServInfo),
    Welcome(Welcome),
    PauseGame(PauseGameServer),
    GameSpeed(GameSpeedServer),
    SayTeam(SayTeamServer),
    MapChange(MapChange),
    ForceDeath(ForceDeath),
    ItemList(ItemList),
    InitClient(InitClient),
    ClientDisconnected(ClientDisconnected),
    SpawnState(SpawnState),
    ShotFx(ShotFx),
    ExplodeFx(ExplodeFx),
    Damage(Damage),
    HitPush(HitPush),
    Died(Died),
    TeamInfo(TeamInfo),
    Resume(Resume),
    ItemSpawn(ItemSpawn),
    ItemAck(ItemAck),
    Clipboard(ClipboardServer),
    Undo(UndoServer),
    Redo(RedoServer),
    Pong(Pong),
    TimeUp(TimeUp),
    ServMsg(ServMsg),
    SendDemoList(SendDemoList),
    DemoPlayback(DemoPlayback),
    CurrentMaster(CurrentMaster),
    MasterMode(MasterModeServer),
    Spectator(SpectatorServer),
    SetTeam(SetTeamServer),
    Announce(Announce),
    NewMap(NewMap),
    ReqAuth(ReqAuth),
    AuthChal(AuthChal),
    InitAi(InitAi),
    ServCmd(ServCmd),
    Bases(BasesServer),
    BaseInfo(BaseInfo),
    BaseScore(BaseScore),
    RepAmmo(RepAmmoServer),
    BaseRegen(BaseRegen),
    InitFlags(InitFlagsServer),
    DropFlag(DropFlag),
    ScoreFlag(ScoreFlag),
    ReturnFlag(ReturnFlag),
    TakeFlag(TakeFlagServer),
    ResetFlag(ResetFlag),
    InvisFlag(InvisFlag),
    InitTokens(InitTokensServer),
    TakeToken(TakeTokenServer),
    ExpireTokens(ExpireTokens),
    DropTokens(DropTokens),
    StealTokens(StealTokens),
    DepositTokens(DepositTokensServer),
    /// `N_CLIENT` Wrapper
    Client(Client),
}

/// Client -> Server (Channel 1)
#[derive(Debug, Clone, PartialEq)]
pub enum Chan1Client {
    Connect(Connect),
    AuthAnswer(AuthAnswer),
    Ping(Ping),
    FromAi(FromAi),
    Sound(Sound),
    EditMode(EditModeClient),
    MapCrc(MapCrc),
    CheckMaps(CheckMaps),
    TrySpawn(TrySpawn),
    GunSelect(GunSelect),
    Spawn(SpawnClient),
    Suicide(Suicide),
    Shoot(Shoot),
    Explode(Explode),
    ItemPickup(ItemPickup),
    Text(Text),
    SayTeam(SayTeamClient),
    SwitchName(SwitchName),
    SwitchModel(SwitchModel),
    SwitchTeam(SwitchTeam),
    MapVote(MapVote),
    ItemList(ItemList),
    EditEnt(EditEntClient),
    EditVar(EditVar),
    ClientPing(ClientPing),
    MasterMode(MasterModeClient),
    ClearBans(ClearBans),
    Kick(Kick),
    Spectator(SpectatorClient),
    SetTeam(SetTeamClient),
    ForceIntermission(ForceIntermission),
    RecordDemo(RecordDemo),
    StopDemo(StopDemo),
    ClearDemos(ClearDemos),
    ListDemos(ListDemos),
    GetDemo(GetDemo),
    GetMap(GetMap),
    NewMap(NewMap),
    SetMaster(SetMaster),
    AddBot(AddBot),
    DelBot(DelBot),
    BotLimit(BotLimit),
    BotBalance(BotBalance),
    AuthTry(AuthTry),
    AuthKick(AuthKick),
    PauseGame(PauseGameClient),
    GameSpeed(GameSpeedClient),
    Copy(CopyClient),
    Paste(PasteClient),
    Clipboard(ClipboardClient),
    Undo(UndoClient),
    Redo(RedoClient),
    EditFace(EditFace),
    EditTex(EditTex),
    EditMat(EditMat),
    Flip(Flip),
    Rotate(Rotate),
    Replace(Replace),
    DelCube(DelCube),
    EditVSlot(EditVSlot),
    Remip(Remip),
    ServCmd(ServCmd),
    Bases(BasesClient),
    RepAmmo(RepAmmoClient),
    TryDropFlag(TryDropFlag),
    TakeFlag(TakeFlagClient),
    InitFlags(InitFlagsClient),
    InitTokens(InitTokensClient),
    TakeToken(TakeTokenClient),
    DepositTokens(DepositTokensClient),
}
