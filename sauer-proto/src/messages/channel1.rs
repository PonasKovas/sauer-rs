use crate::data::*;
use cube_string::CubeString;

// =====================================================================
// CHANNEL 1: GAME STATE AND EVENTS (Reliable)
// =====================================================================

mod decode_client;
mod decode_server;
mod encode_client;
mod encode_server;

/// Server -> Client (Channel 1)
#[derive(Debug, Clone, PartialEq)]
pub enum Chan1Server {
    /// `N_DEMOPACKET` Demo playback marker.
    DemoPacket,

    /// `N_SERVINFO` Initial server information sent when a client starts connecting.
    ServInfo {
        client_num: i32,
        protocol: i32,
        session_id: i32,
        has_password: bool,
        server_desc: CubeString,
        server_auth: CubeString,
    },

    /// `N_WELCOME` The client is fully welcomed and joined into the game.
    Welcome,

    /// `N_PAUSEGAME` Sets or removes the paused state of the game.
    PauseGame { is_paused: bool, client_num: i32 },

    /// `N_GAMESPEED` Modifies the speed of the game.
    GameSpeed { speed: i32, client_num: i32 },

    /// `N_SOUND` Plays a sound at the target client's location.
    Sound { client_num: i32, sound_id: i32 },

    /// `N_TEXT` Emits a global chat message from a client.
    Text { client_num: i32, text: CubeString },

    /// `N_SAYTEAM` Emits a team chat message.
    SayTeam { client_num: i32, text: CubeString },

    /// `N_MAPCHANGE` Notifies clients to change the current map and gamemode.
    MapChange {
        map_name: CubeString,
        mode: GameMode,
        has_items: bool,
    },

    /// `N_FORCEDEATH` Forces a client to die instantly.
    ForceDeath { client_num: i32 },

    /// `N_ITEMLIST` Bulk sends the locations/types of all items on a map.
    ItemList { items: Vec<ItemSpawn> },

    /// `N_INITCLIENT` A new client has connected.
    InitClient {
        client_num: i32,
        name: CubeString,
        team: CubeString,
        player_model: PlayerModel,
    },

    /// `N_SWITCHNAME` Informs the client that someone changed their name.
    SwitchName { client_num: i32, name: CubeString },

    /// `N_SWITCHMODEL` Informs the client that someone changed their playermodel.
    SwitchModel { client_num: i32, model: PlayerModel },

    /// `N_CDIS` A client has fully disconnected.
    ClientDisconnected { client_num: i32 },

    /// `N_SPAWN` Sent to instruct a client to spawn in the game.
    Spawn { client_num: i32, state: PlayerState },

    /// `N_SPAWNSTATE` Confirms the specific state (health, ammo, etc.) of a client who just spawned.
    SpawnState { client_num: i32, state: PlayerState },

    /// `N_SHOTFX` Replicates weapon firing effects across clients.
    ShotFx {
        client_num: i32,
        gun: Gun,
        id: i32,
        from: Vector3<f32>,
        to: Vector3<f32>,
    },

    /// `N_EXPLODEFX` Replicates projectile explosion effects across clients.
    ExplodeFx { client_num: i32, gun: Gun, id: i32 },

    /// `N_DAMAGE` Applies damage to a target client.
    Damage {
        target_cn: i32,
        actor_cn: i32,
        damage: i32,
        armour: i32,
        health: i32,
    },

    /// `N_HITPUSH` Applies physical knockback from weapon hits to a client.
    HitPush {
        target_cn: i32,
        gun: Gun,
        damage: i32,
        dir: Vector3<f32>,
    },

    /// `N_DIED` Notifies that a player was killed by an actor.
    Died {
        victim_cn: i32,
        actor_cn: i32,
        actor_frags: i32,
        team_frags: i32,
    },

    /// `N_TEAMINFO` Transmits team scores (frags).
    TeamInfo { teams: Vec<TeamInfo> },

    /// `N_GUNSELECT` Indicates that a player changed their current weapon selection.
    GunSelect { client_num: i32, gun: Gun },

    /// `N_TAUNT` Indicates that a player is taunting.
    Taunt { client_num: i32 },

    /// `N_RESUME` Sent during a late join to tell the connecting client about the state of all players.
    Resume { clients: Vec<ClientResumeState> },

    /// `N_ITEMSPAWN` Notifies that an item has spawned on the map.
    ItemSpawn { item_index: i32 },

    /// `N_ITEMACC` Acknowledges an item pickup.
    ItemAck { item_index: i32, client_num: i32 },

    /// `N_CLIPBOARD` Pushes map clipboard data (for cooperative editing) to the client.
    Clipboard {
        client_num: i32,
        unpack_len: i32,
        pack_len: i32,
        data: Vec<u8>,
    },

    /// `N_UNDO` Sends geometry undo data to the client.
    Undo {
        client_num: i32,
        unpack_len: i32,
        pack_len: i32,
        data: Vec<u8>,
    },

    /// `N_REDO` Sends geometry redo data to the client.
    Redo {
        client_num: i32,
        unpack_len: i32,
        pack_len: i32,
        data: Vec<u8>,
    },

    /// `N_EDITF` Coop Edit: Face Modification.
    EditFace {
        client_num: i32,
        sel: SelInfo,
        dir: i32,
        mode: EditFaceMode,
    },

    /// `N_EDITT` Coop Edit: Texture Modification.
    EditTex {
        client_num: i32,
        sel: SelInfo,
        tex: i32,
        all_faces: bool,
        extra_data: Vec<u8>,
    },

    /// `N_EDITM` Coop Edit: Material Modification.
    EditMat {
        client_num: i32,
        sel: SelInfo,
        mat: i32,
        filter: i32,
    },

    /// `N_FLIP` Coop Edit: Flip geometry.
    Flip { client_num: i32, sel: SelInfo },

    /// `N_COPY` Coop Edit: Copy geometry.
    Copy { client_num: i32, sel: SelInfo },

    /// `N_PASTE` Coop Edit: Paste geometry.
    Paste { client_num: i32, sel: SelInfo },

    /// `N_ROTATE` Coop Edit: Rotate geometry.
    Rotate {
        client_num: i32,
        sel: SelInfo,
        dir: i32,
    },

    /// `N_REPLACE` Coop Edit: Replace texture.
    Replace {
        client_num: i32,
        sel: SelInfo,
        old_tex: i32,
        new_tex: i32,
        in_sel: i32,
        extra_data: Vec<u8>,
    },

    /// `N_DELCUBE` Coop Edit: Delete a cube.
    DelCube { client_num: i32, sel: SelInfo },

    /// `N_EDITVSLOT` Coop Edit: Modify texture V-Slots.
    EditVSlot {
        client_num: i32,
        sel: SelInfo,
        delta: i32,
        all_faces: bool,
        extra_data: Vec<u8>,
    },

    /// `N_REMIP` Coop Edit: Informs clients that someone recalculates lighting/mips.
    Remip { client_num: i32 },

    /// `N_EDITENT` Coop Edit: Modification of map entities.
    EditEnt {
        client_num: i32,
        ent_id: i32,
        pos: Vector3<f32>,
        type_: EntityType,
        attr1: i32,
        attr2: i32,
        attr3: i32,
        attr4: i32,
        attr5: i32,
    },

    /// `N_EDITVAR` Coop Edit: A map variable was overridden.
    EditVar {
        client_num: i32,
        name: CubeString,
        val: EditVarValue,
    },

    /// `N_PONG` Basic ping packet sent to measure round-trip time.
    Pong { ping: i32 },

    /// `N_CLIENTPING` Updates the scoreboard ping value for a specific client.
    ClientPing { client_num: i32, ping: i32 },

    /// `N_TIMEUP` Indicates that the current match time limit has been updated.
    TimeUp { seconds_remaining: i32 },

    /// `N_SERVMSG` Displays a direct system/server message in the client's console.
    ServMsg { text: CubeString },

    /// `N_SENDDEMOLIST` Replicates the list of available demos back to the client.
    SendDemoList { demos: Vec<CubeString> },

    /// `N_DEMOPLAYBACK` Informs the client that the server has started or stopped demo playback.
    DemoPlayback { is_on: bool, client_num: i32 },

    /// `N_CURRENTMASTER` Notifies clients of the current master mode and list of clients with privileges.
    CurrentMaster {
        master_mode: MasterMode,
        clients: Vec<MasterClient>,
    },

    /// `N_MASTERMODE` Updates the global Master Mode.
    MasterMode { master_mode: MasterMode },

    /// `N_EDITMODE` Notifies that a player entered or left Edit Mode.
    EditMode { client_num: i32, is_editing: bool },

    /// `N_SPECTATOR` Forces a player into or out of Spectator Mode.
    Spectator { client_num: i32, is_spectator: bool },

    /// `N_SETTEAM` Moves a player to a specific team.
    SetTeam {
        client_num: i32,
        team: CubeString,
        reason: i32,
    },

    /// `N_ANNOUNCE` Server announces important item spawns natively (e.g., Quad Damage).
    Announce { item_type: ItemType },

    /// `N_NEWMAP` Sent to force all clients to generate a new empty map of the specified size.
    NewMap { client_num: Option<i32>, size: i32 },

    /// `N_REQAUTH` Initiates an authentication request from the server.
    ReqAuth { auth_domain: CubeString },

    /// `N_AUTHCHAL` Sends the cryptographic challenge down to the client.
    AuthChal {
        desc: CubeString,
        id: u32,
        challenge: CubeString,
    },

    /// `N_INITAI` Spawns a server-side AI Bot into the client's game world.
    InitAi {
        bot_cn: i32,
        owner_cn: i32,
        ai_type: AiType,
        skill: i32,
        player_model: PlayerModel,
        name: CubeString,
        team: CubeString,
    },

    /// `N_SERVCMD` Remote server command feedback.
    ServCmd { cmd: CubeString },

    /// `N_BASES` Sends the list of capture bases for the current map (Capture / Regen Capture).
    Bases { bases: Vec<BaseInfo> },

    /// `N_BASEINFO` Updates the state of a single capture base.
    BaseInfo {
        base: i32,
        owner: CubeString,
        enemy: CubeString,
        converted: bool,
        ammo: i32,
    },

    /// `N_BASESCORE` Updates the score for a team on a capture base.
    BaseScore {
        base: i32,
        team: CubeString,
        total: i32,
    },

    /// `N_REPAMMO` Replenishes a player's ammo (Regen Capture).
    RepAmmo { client_num: i32, ammotype: ItemType },

    /// `N_BASEREGEN` Regenerates a player's health and armour at a capture base.
    BaseRegen {
        client_num: i32,
        health: i32,
        armour: i32,
        ammotype: ItemType,
        ammo: i32,
    },

    /// `N_INITFLAGS` Sends initial flag states to joining clients (CTF modes).
    InitFlags {
        scores: [i32; 2],
        flags: Vec<FlagState>,
    },

    /// `N_DROPFLAG` A player dropped a flag.
    DropFlag {
        client_num: i32,
        flag: i32,
        version: i32,
        drop_loc: Vector3<f32>,
    },

    /// `N_SCOREFLAG` A player scored with a flag.
    ScoreFlag {
        client_num: i32,
        relay_flag: i32,
        relay_version: i32,
        goal_flag: i32,
        goal_version: i32,
        goal_spawn: i32,
        team: i32,
        score: i32,
        oflags: i32,
    },

    /// `N_RETURNFLAG` A player returned a flag to its base.
    ReturnFlag {
        client_num: i32,
        flag: i32,
        version: i32,
    },

    /// `N_TAKEFLAG` A player picked up a flag.
    TakeFlag {
        client_num: i32,
        flag: i32,
        version: i32,
    },

    /// `N_RESETFLAG` A flag was reset to its spawn location.
    ResetFlag {
        flag: i32,
        version: i32,
        spawn_index: i32,
        team: i32,
        score: i32,
    },

    /// `N_INVISFLAG` Marks a flag as invisible or visible.
    InvisFlag { flag: i32, invis: bool },

    /// `N_INITTOKENS` Sends initial token states to joining clients (Collect mode).
    InitTokens {
        scores: [i32; 2],
        tokens: Vec<TokenState>,
        client_tokens: Vec<ClientTokenState>,
    },

    /// `N_TAKETOKEN` A player collected a token.
    TakeToken {
        client_num: i32,
        id: i32,
        total: i32,
    },

    /// `N_EXPIRETOKENS` Notifies that specific tokens have expired.
    ExpireTokens { tokens: Vec<i32> },

    /// `N_DROPTOKENS` A player dropped their held tokens.
    DropTokens {
        client_num: i32,
        drop_loc: Vector3<f32>,
        tokens: Vec<DroppedToken>,
    },

    /// `N_STEALTOKENS` A player stole tokens from the enemy team.
    StealTokens {
        client_num: i32,
        team: i32,
        basenum: i32,
        enemy_team: i32,
        score: i32,
        drop_loc: Vector3<f32>,
        tokens: Vec<StolenToken>,
    },

    /// `N_DEPOSITTOKENS` A player deposited tokens at a base.
    DepositTokens {
        client_num: i32,
        base: i32,
        deposited: i32,
        team: i32,
        score: i32,
        flags: i32,
    },
}

/// Client -> Server (Channel 1)
#[derive(Debug, Clone, PartialEq)]
pub enum Chan1Client {
    /// `N_CONNECT` Attempts to connect to the server with user details and passwords.
    Connect {
        name: CubeString,
        player_model: PlayerModel,
        password_hash: CubeString,
        auth_desc: CubeString,
        auth_name: CubeString,
    },

    /// `N_AUTHANS` Provides the mathematical answer to a server's AuthChal.
    AuthAnswer {
        desc: CubeString,
        id: u32,
        answer: CubeString,
    },

    /// `N_PING` Measures the roundtrip time.
    Ping { time: i32 },

    /// `N_FROMAI` Alters the context of subsequent messages in the packet to act as if they came from the specified AI bot.
    FromAi { qcn: i32 },

    /// `N_SOUND` Plays a sound.
    Sound { sound_id: i32 },

    /// `N_EDITMODE` The client toggled cooperative edit mode locally.
    EditMode { val: bool },

    /// `N_MAPCRC` The client verifies the CRC of the map they currently have loaded.
    MapCrc { map_name: CubeString, crc: i32 },

    /// `N_CHECKMAPS` Forces a manual check of map CRCs on the server.
    CheckMaps,

    /// `N_TRYSPAWN` Requests to spawn from the server.
    TrySpawn,

    /// `N_GUNSELECT` Acknowledges the client switching weapons.
    GunSelect { gun: Gun },

    /// `N_SPAWN` The client actually spawns.
    Spawn { life_sequence: i32, gun_select: Gun },

    /// `N_SUICIDE` The client forcefully kills themselves.
    Suicide,

    /// `N_SHOOT` The client fired a weapon.
    Shoot {
        id: i32,
        gun: Gun,
        from: Vector3<f32>,
        to: Vector3<f32>,
        hits: Vec<HitInfo>,
    },

    /// `N_EXPLODE` Projectile explosion triggered by the client.
    Explode {
        millis: i32,
        gun: Gun,
        id: i32,
        hits: Vec<HitInfo>,
    },

    /// `N_ITEMPICKUP` The client requested to pick up an item.
    ItemPickup { item_index: i32 },

    /// `N_TEXT` Submits a global chat message.
    Text { text: CubeString },

    /// `N_SAYTEAM` Submits a team chat message.
    SayTeam { text: CubeString },

    /// `N_SWITCHNAME` The client requests to change their display name.
    SwitchName { name: CubeString },

    /// `N_SWITCHMODEL` The client changes their display model.
    SwitchModel { model: PlayerModel },

    /// `N_SWITCHTEAM` The client requests to switch teams.
    SwitchTeam { team: CubeString },

    /// `N_MAPVOTE` The client votes to change the map and mode.
    MapVote {
        map_name: CubeString,
        mode: GameMode,
    },

    /// `N_ITEMLIST` Bulk sends the items available on the client's map to the server (synchronization).
    ItemList { items: Vec<ItemSpawn> },

    /// `N_EDITENT` Coop Edit: A map entity was modified.
    EditEnt {
        ent_id: i32,
        x: i32,
        y: i32,
        z: i32,
        type_: EntityType,
        attr1: i32,
        attr2: i32,
        attr3: i32,
        attr4: i32,
        attr5: i32,
    },

    /// `N_EDITVAR` Coop Edit: A map variable was modified locally.
    EditVar { name: CubeString, val: EditVarValue },

    /// `N_CLIENTPING` Reports the client's smoothed ping to the server.
    ClientPing { ping: i32 },

    /// `N_MASTERMODE` Requests to change the global Master Mode.
    MasterMode { mode: MasterMode },

    /// `N_CLEARBANS` Requests to clear all bans.
    ClearBans,

    /// `N_KICK` Requests to kick a player.
    Kick { victim_cn: i32, reason: CubeString },

    /// `N_SPECTATOR` Requests to move someone (or self) to/from the spectator team.
    Spectator { spectator_cn: i32, val: bool },

    /// `N_SETTEAM` Requests to force someone to a specific team.
    SetTeam { client_num: i32, team: CubeString },

    /// `N_FORCEINTERMISSION` Requests to force the current match to end.
    ForceIntermission,

    /// `N_RECORDDEMO` Toggles automatic demo recording for the next match.
    RecordDemo { val: bool },

    /// `N_STOPDEMO` Stops the current demo recording.
    StopDemo,

    /// `N_CLEARDEMOS` Clears a specific demo (or all) from server memory.
    ClearDemos { demo_id: i32 },

    /// `N_LISTDEMOS` Requests a listing of all available demos on the server.
    ListDemos,

    /// `N_GETDEMO` Requests to download a specific demo from the server.
    GetDemo { demo_id: i32, tag: i32 },

    /// `N_GETMAP` Requests to download the current map from the server (Coop Edit).
    GetMap,

    /// `N_NEWMAP` Requests to clear the map into a new empty state of the specified size.
    NewMap { size: i32 },

    /// `N_SETMASTER` Requests to set master privileges using a password hash.
    SetMaster {
        client_num: i32,
        val: bool,
        hash: CubeString,
    },

    /// `N_ADDBOT` Requests to add a bot with specific skill.
    AddBot { skill: i32 },

    /// `N_DELBOT` Requests to delete a bot.
    DelBot,

    /// `N_BOTLIMIT` Modifies the global bot limit.
    BotLimit { limit: i32 },

    /// `N_BOTBALANCE` Modifies the global bot auto-balance flag.
    BotBalance { balance: bool },

    /// `N_AUTHTRY` Submits a request to authenticate and claim privileges via cryptographic keys.
    AuthTry { desc: CubeString, name: CubeString },

    /// `N_AUTHKICK` Submits a request to kick an individual via cryptographic auth verification.
    AuthKick {
        desc: CubeString,
        name: CubeString,
        victim_cn: i32,
        reason: CubeString,
    },

    /// `N_PAUSEGAME` Toggles game paused state.
    PauseGame { val: bool },

    /// `N_GAMESPEED` Changes global game speed.
    GameSpeed { speed: i32 },

    /// `N_COPY` Coop Edit: Client copies geometry to their clipboard.
    Copy,

    /// `N_PASTE` Coop Edit: Client requests to paste geometry from their clipboard.
    Paste,

    /// `N_CLIPBOARD` Coop Edit: Uploads raw clipboard geometry data.
    Clipboard {
        unpack_len: i32,
        pack_len: i32,
        data: Vec<u8>,
    },

    /// `N_UNDO` Coop Edit: Undo event.
    Undo {
        unpack_len: i32,
        pack_len: i32,
        data: Vec<u8>,
    },

    /// `N_REDO` Coop Edit: Redo event.
    Redo {
        unpack_len: i32,
        pack_len: i32,
        data: Vec<u8>,
    },

    /// `N_EDITF` Coop Edit: Face Modification.
    EditFace {
        sel: SelInfo,
        dir: i32,
        mode: EditFaceMode,
    },

    /// `N_EDITT` Coop Edit: Texture Modification.
    EditTex {
        sel: SelInfo,
        tex: i32,
        all_faces: bool,
        extra_data: Vec<u8>,
    },

    /// `N_EDITM` Coop Edit: Material Modification.
    EditMat { sel: SelInfo, mat: i32, filter: i32 },

    /// `N_FLIP` Coop Edit: Flip geometry.
    Flip { sel: SelInfo },

    /// `N_ROTATE` Coop Edit: Rotate geometry.
    Rotate { sel: SelInfo, dir: i32 },

    /// `N_REPLACE` Coop Edit: Replace texture.
    Replace {
        sel: SelInfo,
        old_tex: i32,
        new_tex: i32,
        in_sel: i32,
        extra_data: Vec<u8>,
    },

    /// `N_DELCUBE` Coop Edit: Delete a cube.
    DelCube { sel: SelInfo },

    /// `N_EDITVSLOT` Coop Edit: Modify texture V-Slots.
    EditVSlot {
        sel: SelInfo,
        delta: i32,
        all_faces: bool,
        extra_data: Vec<u8>,
    },

    /// `N_REMIP` Coop Edit: Instructs the server to broadcast a lighting/mip recalculation.
    Remip,

    /// `N_SERVCMD` Dispatches a raw server command payload.
    ServCmd { cmd: CubeString },

    /// `N_BASES` Sends the list of capture base spawns to the server (Capture / Regen Capture).
    Bases { bases: Vec<BaseSpawn> },

    /// `N_REPAMMO` Requests ammo replenishment (Regen Capture).
    RepAmmo,

    /// `N_TRYDROPFLAG` Client attempts to drop the flag they are carrying.
    TryDropFlag,

    /// `N_TAKEFLAG` Client acknowledges picking up a flag.
    TakeFlag { flag: i32, version: i32 },

    /// `N_INITFLAGS` Sends initial flag spawn positions to the server (CTF modes).
    InitFlags { flags: Vec<FlagSpawn> },

    /// `N_INITTOKENS` Sends initial token base spawn data to the server (Collect mode).
    InitTokens { bases: Vec<TokenBaseSpawn> },

    /// `N_TAKETOKEN` Client collected a token.
    TakeToken { id: i32 },

    /// `N_DEPOSITTOKENS` Client deposited tokens at a base.
    DepositTokens { base: i32 },
}
