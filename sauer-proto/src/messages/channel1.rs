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
    /// Demo playback marker.
    DemoPacket,

    /// Initial server information sent when a client starts connecting.
    ServInfo {
        client_num: i32,
        protocol: i32,
        session_id: i32,
        has_password: bool,
        server_desc: CubeString,
        server_auth: CubeString,
    },

    /// The client is fully welcomed and joined into the game.
    Welcome,

    /// Sets or removes the paused state of the game.
    PauseGame {
        is_paused: bool,
        client_num: i32,
    },

    /// Modifies the speed of the game.
    GameSpeed {
        speed: i32,
        client_num: i32,
    },

    /// Plays a sound at the target client's location. (Implicitly wrapped in N_CLIENT).
    Sound {
        client_num: i32,
        sound_id: i32,
    },

    /// Emits a global chat message from a client. (Implicitly wrapped in N_CLIENT).
    Text {
        client_num: i32,
        text: CubeString,
    },

    /// Emits a team chat message.
    SayTeam {
        client_num: i32,
        text: CubeString,
    },

    /// Notifies clients to change the current map and gamemode.
    MapChange {
        map_name: CubeString,
        mode: GameMode,
        has_items: bool,
    },

    /// Forces a client to die instantly.
    ForceDeath {
        client_num: i32,
    },

    /// Bulk sends the locations/types of all items on a map.
    ItemList {
        items: Vec<ItemSpawn>,
    },

    /// A new client has connected (Unwrapped).
    InitClient {
        client_num: i32,
        name: CubeString,
        team: CubeString,
        player_model: PlayerModel,
    },

    /// Informs the client that someone changed their name. (Implicitly wrapped).
    SwitchName {
        client_num: i32,
        name: CubeString,
    },

    /// Informs the client that someone changed their playermodel. (Implicitly wrapped).
    SwitchModel {
        client_num: i32,
        model: PlayerModel,
    },

    /// A client has fully disconnected.
    ClientDisconnected {
        client_num: i32,
    },

    /// Sent to instruct a client to spawn in the game. (Implicitly wrapped).
    Spawn {
        client_num: i32,
        state: PlayerState,
    },

    /// Confirms the specific state (health, ammo, etc.) of a client who just spawned.
    SpawnState {
        client_num: i32,
        state: PlayerState,
    },

    /// Replicates weapon firing effects across clients.
    ShotFx {
        client_num: i32,
        gun: Gun,
        id: i32,
        from: Vector3<f32>,
        to: Vector3<f32>,
    },

    /// Replicates projectile explosion effects across clients.
    ExplodeFx {
        client_num: i32,
        gun: Gun,
        id: i32,
    },

    /// Applies damage to a target client.
    Damage {
        target_cn: i32,
        actor_cn: i32,
        damage: i32,
        armour: i32,
        health: i32,
    },

    /// Applies physical knockback from weapon hits to a client.
    HitPush {
        target_cn: i32,
        gun: Gun,
        damage: i32,
        dir: Vector3<f32>,
    },

    /// Notifies that a player was killed by an actor.
    Died {
        victim_cn: i32,
        actor_cn: i32,
        actor_frags: i32,
        team_frags: i32,
    },

    /// Transmits team scores (frags).
    TeamInfo {
        teams: Vec<TeamInfo>,
    },

    /// Indicates that a player changed their current weapon selection. (Implicitly wrapped).
    GunSelect {
        client_num: i32,
        gun: Gun,
    },

    /// Indicates that a player is taunting. (Implicitly wrapped).
    Taunt {
        client_num: i32,
    },

    /// Sent during a late join to tell the connecting client about the state of all players.
    Resume {
        clients: Vec<ClientResumeState>,
    },

    /// Notifies that an item has spawned on the map.
    ItemSpawn {
        item_index: i32,
    },

    /// Acknowledges an item pickup.
    ItemAck {
        item_index: i32,
        client_num: i32,
    },

    /// Pushes map clipboard data (for cooperative editing) to the client.
    Clipboard {
        client_num: i32,
        unpack_len: i32,
        pack_len: i32,
        data: Vec<u8>,
    },

    /// Sends geometry undo data to the client.
    Undo {
        client_num: i32,
        unpack_len: i32,
        pack_len: i32,
        data: Vec<u8>,
    },

    /// Sends geometry redo data to the client.
    Redo {
        client_num: i32,
        unpack_len: i32,
        pack_len: i32,
        data: Vec<u8>,
    },

    /// Coop Edit: Face Modification.
    EditFace {
        client_num: i32,
        sel: SelInfo,
        dir: i32,
        mode: EditFaceMode,
    },

    /// Coop Edit: Texture Modification.
    EditTex {
        client_num: i32,
        sel: SelInfo,
        tex: i32,
        all_faces: bool,
        extra_data: Vec<u8>,
    },

    /// Coop Edit: Material Modification.
    EditMat {
        client_num: i32,
        sel: SelInfo,
        mat: i32,
        filter: i32,
    },

    /// Coop Edit: Flip geometry.
    Flip {
        client_num: i32,
        sel: SelInfo,
    },

    /// Coop Edit: Copy geometry.
    Copy {
        client_num: i32,
        sel: SelInfo,
    },

    /// Coop Edit: Paste geometry.
    Paste {
        client_num: i32,
        sel: SelInfo,
    },

    /// Coop Edit: Rotate geometry.
    Rotate {
        client_num: i32,
        sel: SelInfo,
        dir: i32,
    },

    /// Coop Edit: Replace texture.
    Replace {
        client_num: i32,
        sel: SelInfo,
        old_tex: i32,
        new_tex: i32,
        in_sel: i32,
        extra_data: Vec<u8>,
    },

    /// Coop Edit: Delete a cube.
    DelCube {
        client_num: i32,
        sel: SelInfo,
    },

    /// Coop Edit: Modify texture V-Slots.
    EditVSlot {
        client_num: i32,
        sel: SelInfo,
        delta: i32,
        all_faces: bool,
        extra_data: Vec<u8>,
    },

    /// Coop Edit: Informs clients that someone recalculates lighting/mips. (Implicitly wrapped).
    Remip {
        client_num: i32,
    },

    /// Coop Edit: Modification of map entities. (Implicitly wrapped).
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

    /// Coop Edit: A map variable was overridden. (Implicitly wrapped).
    EditVar {
        client_num: i32,
        name: CubeString,
        val: EditVarValue,
    },

    /// Basic ping packet sent to measure round-trip time.
    Pong {
        ping: i32,
    },

    /// Updates the scoreboard ping value for a specific client. (Implicitly wrapped).
    ClientPing {
        client_num: i32,
        ping: i32,
    },

    /// Indicates that the current match time limit has been updated.
    TimeUp {
        seconds_remaining: i32,
    },

    /// Displays a direct system/server message in the client's console.
    ServMsg {
        text: CubeString,
    },

    /// Replicates the list of available demos back to the client.
    SendDemoList {
        demos: Vec<CubeString>,
    },

    /// Informs the client that the server has started or stopped demo playback.
    DemoPlayback {
        is_on: bool,
        client_num: i32,
    },

    /// Notifies clients of the current master mode and list of clients with privileges.
    CurrentMaster {
        master_mode: MasterMode,
        clients: Vec<MasterClient>,
    },

    /// Updates the global Master Mode.
    MasterMode {
        master_mode: MasterMode,
    },

    /// Notifies that a player entered or left Edit Mode. (Implicitly wrapped).
    EditMode {
        client_num: i32,
        is_editing: bool,
    },

    /// Forces a player into or out of Spectator Mode.
    Spectator {
        client_num: i32,
        is_spectator: bool,
    },

    /// Moves a player to a specific team.
    SetTeam {
        client_num: i32,
        team: CubeString,
        reason: i32,
    },

    /// Server announces important item spawns natively (e.g., Quad Damage).
    Announce {
        item_type: ItemType,
    },

    /// Sent to force all clients to generate a new empty map of the specified size.
    NewMap {
        client_num: Option<i32>,
        size: i32,
    },

    /// Initiates an authentication request from the server.
    ReqAuth {
        auth_domain: CubeString,
    },

    /// Sends the cryptographic challenge down to the client.
    AuthChal {
        desc: CubeString,
        id: u32,
        challenge: CubeString,
    },

    /// Spawns a server-side AI Bot into the client's game world.
    InitAi {
        bot_cn: i32,
        owner_cn: i32,
        ai_type: AiType,
        skill: i32,
        player_model: PlayerModel,
        name: CubeString,
        team: CubeString,
    },

    /// Remote server command feedback.
    ServCmd {
        cmd: CubeString,
    },

    Bases {
        bases: Vec<BaseInfo>,
    },

    BaseInfo {
        base: i32,
        owner: CubeString,
        enemy: CubeString,
        converted: bool,
        ammo: i32,
    },

    BaseScore {
        base: i32,
        team: CubeString,
        total: i32,
    },

    RepAmmo {
        client_num: i32,
        ammotype: ItemType,
    },

    BaseRegen {
        client_num: i32,
        health: i32,
        armour: i32,
        ammotype: ItemType,
        ammo: i32,
    },

    InitFlags {
        scores: [i32; 2],
        flags: Vec<FlagState>,
    },

    DropFlag {
        client_num: i32,
        flag: i32,
        version: i32,
        drop_loc: Vector3<f32>,
    },

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

    ReturnFlag {
        client_num: i32,
        flag: i32,
        version: i32,
    },

    TakeFlag {
        client_num: i32,
        flag: i32,
        version: i32,
    },

    ResetFlag {
        flag: i32,
        version: i32,
        spawn_index: i32,
        team: i32,
        score: i32,
    },

    InvisFlag {
        flag: i32,
        invis: bool,
    },

    InitTokens {
        scores: [i32; 2],
        tokens: Vec<TokenState>,
        client_tokens: Vec<ClientTokenState>,
    },

    TakeToken {
        client_num: i32,
        id: i32,
        total: i32,
    },

    ExpireTokens {
        tokens: Vec<i32>,
    },

    DropTokens {
        client_num: i32,
        drop_loc: Vector3<f32>,
        tokens: Vec<DroppedToken>,
    },

    StealTokens {
        client_num: i32,
        team: i32,
        basenum: i32,
        enemy_team: i32,
        score: i32,
        drop_loc: Vector3<f32>,
        tokens: Vec<StolenToken>,
    },

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
    /// Attempts to connect to the server with user details and passwords.
    Connect {
        name: CubeString,
        player_model: PlayerModel,
        password_hash: CubeString,
        auth_desc: CubeString,
        auth_name: CubeString,
    },

    /// Provides the mathematical answer to a server's AuthChal.
    AuthAnswer {
        desc: CubeString,
        id: u32,
        answer: CubeString,
    },

    /// Measures the roundtrip time.
    Ping {
        time: i32,
    },

    /// Alters the context of subsequent messages in the packet to act as if they came from the specified AI bot.
    FromAi {
        qcn: i32,
    },

    /// Plays a sound
    Sound {
        sound_id: i32,
    },

    /// The client toggled cooperative edit mode locally.
    EditMode {
        val: bool,
    },

    /// The client verifies the CRC of the map they currently have loaded.
    MapCrc {
        map_name: CubeString,
        crc: i32,
    },

    /// Forces a manual check of map CRCs on the server.
    CheckMaps,

    /// Requests to spawn from the server.
    TrySpawn,

    /// Acknowledges the client switching weapons.
    GunSelect {
        gun: Gun,
    },

    /// The client actually spawns.
    Spawn {
        life_sequence: i32,
        gun_select: Gun,
    },

    /// The client forcefully kills themselves.
    Suicide,

    /// The client fired a weapon.
    Shoot {
        id: i32,
        gun: Gun,
        from: Vector3<f32>,
        to: Vector3<f32>,
        hits: Vec<HitInfo>,
    },

    /// Projectile explosion triggered by the client.
    Explode {
        millis: i32,
        gun: Gun,
        id: i32,
        hits: Vec<HitInfo>,
    },

    /// The client requested to pick up an item.
    ItemPickup {
        item_index: i32,
    },

    /// Submits a global chat message.
    Text {
        text: CubeString,
    },

    /// Submits a team chat message.
    SayTeam {
        text: CubeString,
    },

    /// The client requests to change their display name.
    SwitchName {
        name: CubeString,
    },

    /// The client changes their display model.
    SwitchModel {
        model: PlayerModel,
    },

    /// The client requests to switch teams.
    SwitchTeam {
        team: CubeString,
    },

    /// The client votes to change the map and mode.
    MapVote {
        map_name: CubeString,
        mode: GameMode,
    },

    /// Bulk sends the items available on the client's map to the server (synchronization).
    ItemList {
        items: Vec<ItemSpawn>,
    },

    /// Coop Edit: A map entity was modified.
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

    /// Coop Edit: A map variable was modified locally.
    EditVar {
        name: CubeString,
        val: EditVarValue,
    },

    /// Reports the client's smoothed ping to the server.
    ClientPing {
        ping: i32,
    },

    /// Requests to change the global Master Mode.
    MasterMode {
        mode: MasterMode,
    },

    /// Requests to clear all bans.
    ClearBans,

    /// Requests to kick a player.
    Kick {
        victim_cn: i32,
        reason: CubeString,
    },

    /// Requests to move someone (or self) to/from the spectator team.
    Spectator {
        spectator_cn: i32,
        val: bool,
    },

    /// Requests to force someone to a specific team.
    SetTeam {
        client_num: i32,
        team: CubeString,
    },

    /// Requests to force the current match to end.
    ForceIntermission,

    /// Toggles automatic demo recording for the next match.
    RecordDemo {
        val: bool,
    },

    /// Stops the current demo recording.
    StopDemo,

    /// Clears a specific demo (or all) from server memory.
    ClearDemos {
        demo_id: i32,
    },

    /// Requests a listing of all available demos on the server.
    ListDemos,

    /// Requests to download a specific demo from the server.
    GetDemo {
        demo_id: i32,
        tag: i32,
    },

    /// Requests to download the current map from the server (Coop Edit).
    GetMap,

    /// Requests to clear the map into a new empty state of the specified size.
    NewMap {
        size: i32,
    },

    /// Requests to set master privileges using a password hash.
    SetMaster {
        client_num: i32,
        val: bool,
        hash: CubeString,
    },

    /// Requests to add a bot with specific skill.
    AddBot {
        skill: i32,
    },

    /// Requests to delete a bot.
    DelBot,

    /// Modifies the global bot limit.
    BotLimit {
        limit: i32,
    },

    /// Modifies the global bot auto-balance flag.
    BotBalance {
        balance: bool,
    },

    /// Submits a request to authenticate and claim privileges via cryptographic keys.
    AuthTry {
        desc: CubeString,
        name: CubeString,
    },

    /// Submits a request to kick an individual via cryptographic auth verification.
    AuthKick {
        desc: CubeString,
        name: CubeString,
        victim_cn: i32,
        reason: CubeString,
    },

    /// Toggles game paused state.
    PauseGame {
        val: bool,
    },

    /// Changes global game speed.
    GameSpeed {
        speed: i32,
    },

    /// Coop Edit: Client copies geometry to their clipboard.
    Copy,

    /// Coop Edit: Client requests to paste geometry from their clipboard.
    Paste,

    /// Coop Edit: Uploads raw clipboard geometry data.
    Clipboard {
        unpack_len: i32,
        pack_len: i32,
        data: Vec<u8>,
    },

    /// Coop Edit: Undo event.
    Undo {
        unpack_len: i32,
        pack_len: i32,
        data: Vec<u8>,
    },

    /// Coop Edit: Redo event.
    Redo {
        unpack_len: i32,
        pack_len: i32,
        data: Vec<u8>,
    },

    /// Coop Edit: Face Modification.
    EditFace {
        sel: SelInfo,
        dir: i32,
        mode: EditFaceMode,
    },

    /// Coop Edit: Texture Modification.
    EditTex {
        sel: SelInfo,
        tex: i32,
        all_faces: bool,
        extra_data: Vec<u8>,
    },

    /// Coop Edit: Material Modification.
    EditMat {
        sel: SelInfo,
        mat: i32,
        filter: i32,
    },

    /// Coop Edit: Flip geometry.
    Flip {
        sel: SelInfo,
    },

    /// Coop Edit: Rotate geometry.
    Rotate {
        sel: SelInfo,
        dir: i32,
    },

    /// Coop Edit: Replace texture.
    Replace {
        sel: SelInfo,
        old_tex: i32,
        new_tex: i32,
        in_sel: i32,
        extra_data: Vec<u8>,
    },

    /// Coop Edit: Delete a cube.
    DelCube {
        sel: SelInfo,
    },

    /// Coop Edit: Modify texture V-Slots.
    EditVSlot {
        sel: SelInfo,
        delta: i32,
        all_faces: bool,
        extra_data: Vec<u8>,
    },

    /// Coop Edit: Instructs the server to broadcast a lighting/mip recalculation.
    Remip,

    /// Dispatches a raw server command payload.
    ServCmd {
        cmd: CubeString,
    },

    Bases {
        bases: Vec<BaseSpawn>,
    },

    RepAmmo,

    TryDropFlag,

    TakeFlag {
        flag: i32,
        version: i32,
    },

    InitFlags {
        flags: Vec<FlagSpawn>,
    },

    InitTokens {
        bases: Vec<TokenBaseSpawn>,
    },

    TakeToken {
        id: i32,
    },

    DepositTokens {
        base: i32,
    },
}
