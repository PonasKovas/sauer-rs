use crate::ParseError;
use crate::data;
use crate::data::*;
use crate::error::ParseResultExt;
use crate::internal::macros::{make_packet_enum, make_struct};
use crate::internal::read_slice;
use crate::internal::{SauerRead, SauerWrite, check_tag, constants::*};
use crate::route::{C2S, Chan1, S2C};
use cube_string::CubeString;

make_packet_enum!(
/// Server -> Client (Channel 1)
[S2C<Chan1>] Chan1Server {
    [N_DEMOPACKET] DemoPacket,
    [N_SERVINFO] ServInfo,
    [N_WELCOME] Welcome,
    [N_PAUSEGAME] PauseGameServer,
    [N_GAMESPEED] GameSpeedServer,
    [N_SAYTEAM] SayTeamServer,
    [N_MAPCHANGE] MapChange,
    [N_FORCEDEATH] ForceDeath,
    [N_ITEMLIST] ItemList,
    [N_INITCLIENT] InitClient,
    [N_CDIS] ClientDisconnected,
    [N_SPAWNSTATE] SpawnState,
    [N_SHOTFX] ShotFx,
    [N_EXPLODEFX] ExplodeFx,
    [N_DAMAGE] Damage,
    [N_HITPUSH] HitPush,
    [N_DIED] Died,
    [N_TEAMINFO] TeamInfo,
    [N_RESUME] Resume,
    [N_ITEMSPAWN] ItemSpawn,
    [N_ITEMACC] ItemAck,
    [N_CLIPBOARD] ClipboardServer,
    [N_UNDO] UndoServer,
    [N_REDO] RedoServer,
    [N_PONG] Pong,
    [N_TIMEUP] TimeUp,
    [N_SERVMSG] ServMsg,
    [N_SENDDEMOLIST] SendDemoList,
    [N_DEMOPLAYBACK] DemoPlayback,
    [N_CURRENTMASTER] CurrentMaster,
    [N_MASTERMODE] MasterModeServer,
    [N_SPECTATOR] SpectatorServer,
    [N_SETTEAM] SetTeamServer,
    [N_ANNOUNCE] Announce,
    [N_NEWMAP] NewMap,
    [N_REQAUTH] ReqAuth,
    [N_AUTHCHAL] AuthChal,
    [N_INITAI] InitAi,
    [N_SERVCMD] ServCmd,
    [N_BASES] BasesServer,
    [N_BASEINFO] BaseInfo,
    [N_BASESCORE] BaseScore,
    [N_REPAMMO] RepAmmoServer,
    [N_BASEREGEN] BaseRegen,
    [N_INITFLAGS] InitFlagsServer,
    [N_DROPFLAG] DropFlag,
    [N_SCOREFLAG] ScoreFlag,
    [N_RETURNFLAG] ReturnFlag,
    [N_TAKEFLAG] TakeFlagServer,
    [N_RESETFLAG] ResetFlag,
    [N_INVISFLAG] InvisFlag,
    [N_INITTOKENS] InitTokensServer,
    [N_TAKETOKEN] TakeTokenServer,
    [N_EXPIRETOKENS] ExpireTokens,
    [N_DROPTOKENS] DropTokens,
    [N_STEALTOKENS] StealTokens,
    [N_DEPOSITTOKENS] DepositTokensServer,

    [N_SOUND] Sound [N_CLIENT Client],
    [N_TEXT] Text [N_CLIENT Client],
    [N_SWITCHNAME] SwitchName [N_CLIENT Client],
    [N_SWITCHMODEL] SwitchModel [N_CLIENT Client],
    [N_SPAWN] SpawnServer [N_CLIENT Client],
    [N_GUNSELECT] GunSelect [N_CLIENT Client],
    [N_TAUNT] Taunt [N_CLIENT Client],
    [N_REMIP] Remip [N_CLIENT Client],
    [N_EDITENT] EditEntServer [N_CLIENT Client],
    [N_EDITVAR] EditVar [N_CLIENT Client],
    [N_CLIENTPING] ClientPing [N_CLIENT Client],
    [N_EDITMODE] EditModeServer [N_CLIENT Client],
    [N_EDITF] EditFace [N_CLIENT Client],
    [N_EDITT] EditTex [N_CLIENT Client],
    [N_EDITM] EditMat [N_CLIENT Client],
    [N_FLIP] Flip [N_CLIENT Client],
    [N_COPY] CopyServer [N_CLIENT Client],
    [N_PASTE] PasteServer [N_CLIENT Client],
    [N_ROTATE] Rotate [N_CLIENT Client],
    [N_REPLACE] Replace [N_CLIENT Client],
    [N_DELCUBE] DelCube [N_CLIENT Client],
    [N_EDITVSLOT] EditVSlot [N_CLIENT Client],
    [N_NEWMAP] NewMapClient [N_CLIENT Client] [DUPLICATE NewMap],
});

make_packet_enum!(
/// Client -> Server (Channel 1)
[C2S<Chan1>] Chan1Client {
    [N_CONNECT] Connect,
    [N_AUTHANS] AuthAnswer,
    [N_PING] Ping,
    [N_FROMAI] FromAi,
    [N_SOUND] Sound,
    [N_EDITMODE] EditModeClient,
    [N_MAPCRC] MapCrc,
    [N_CHECKMAPS] CheckMaps,
    [N_TRYSPAWN] TrySpawn,
    [N_GUNSELECT] GunSelect,
    [N_SPAWN] SpawnClient,
    [N_SUICIDE] Suicide,
    [N_SHOOT] Shoot,
    [N_EXPLODE] Explode,
    [N_ITEMPICKUP] ItemPickup,
    [N_TEXT] Text,
    [N_SAYTEAM] SayTeamClient,
    [N_SWITCHNAME] SwitchName,
    [N_SWITCHMODEL] SwitchModel,
    [N_SWITCHTEAM] SwitchTeam,
    [N_MAPVOTE] MapVote,
    [N_ITEMLIST] ItemList,
    [N_EDITENT] EditEntClient,
    [N_EDITVAR] EditVar,
    [N_CLIENTPING] ClientPing,
    [N_MASTERMODE] MasterModeClient,
    [N_CLEARBANS] ClearBans,
    [N_KICK] Kick,
    [N_SPECTATOR] SpectatorClient,
    [N_SETTEAM] SetTeamClient,
    [N_FORCEINTERMISSION] ForceIntermission,
    [N_RECORDDEMO] RecordDemo,
    [N_STOPDEMO] StopDemo,
    [N_CLEARDEMOS] ClearDemos,
    [N_LISTDEMOS] ListDemos,
    [N_GETDEMO] GetDemo,
    [N_GETMAP] GetMap,
    [N_NEWMAP] NewMap,
    [N_SETMASTER] SetMaster,
    [N_ADDBOT] AddBot,
    [N_DELBOT] DelBot,
    [N_BOTLIMIT] BotLimit,
    [N_BOTBALANCE] BotBalance,
    [N_AUTHTRY] AuthTry,
    [N_AUTHKICK] AuthKick,
    [N_PAUSEGAME] PauseGameClient,
    [N_GAMESPEED] GameSpeedClient,
    [N_COPY] CopyClient,
    [N_PASTE] PasteClient,
    [N_CLIPBOARD] ClipboardClient,
    [N_UNDO] UndoClient,
    [N_REDO] RedoClient,
    [N_EDITF] EditFace,
    [N_EDITT] EditTex,
    [N_EDITM] EditMat,
    [N_FLIP] Flip,
    [N_ROTATE] Rotate,
    [N_REPLACE] Replace,
    [N_DELCUBE] DelCube,
    [N_EDITVSLOT] EditVSlot,
    [N_REMIP] Remip,
    [N_SERVCMD] ServCmd,
    [N_BASES] BasesClient,
    [N_REPAMMO] RepAmmoClient,
    [N_TRYDROPFLAG] TryDropFlag,
    [N_TAKEFLAG] TakeFlagClient,
    [N_INITFLAGS] InitFlagsClient,
    [N_INITTOKENS] InitTokensClient,
    [N_TAKETOKEN] TakeTokenClient,
    [N_DEPOSITTOKENS] DepositTokensClient,
});

/// `N_CLIENT` Wrapper
#[derive(Debug, Clone, PartialEq)]
pub struct Client<T> {
    pub client_num: i32,
    pub packet: T,
}

// =====================================================================
// SHARED STRUCTS (Server & Client identical)
// =====================================================================

make_struct!(
/// `N_SOUND` Plays a sound.
[N_SOUND] Sound {
    sound_id: i32,
});

make_struct!(
/// `N_TEXT` Emits a chat message.
[N_TEXT] Text {
    text: CubeString,
});

make_struct!(
/// `N_SWITCHNAME` Requests/Informs name change.
[N_SWITCHNAME] SwitchName {
    name: CubeString,
});

make_struct!(
/// `N_SWITCHMODEL` Requests/Informs model change.
[N_SWITCHMODEL] SwitchModel {
    model: PlayerModel,
});

make_struct!(
/// `N_GUNSELECT` Acknowledges or indicates a player changed their current weapon selection.
[N_GUNSELECT] GunSelect {
    gun: Gun,
});

/// `N_EDITVAR` Coop Edit: A map variable was modified.
#[derive(Debug, Clone, PartialEq)]
pub struct EditVar {
    pub name: CubeString,
    pub val: EditVarValue,
}
impl SauerWrite for EditVar {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_EDITVAR.sauer_write(out);
        match &self.val {
            EditVarValue::Int(i) => {
                0i32.sauer_write(out);
                self.name.sauer_write(out);
                i.sauer_write(out);
            }
            EditVarValue::Float(f) => {
                1i32.sauer_write(out);
                self.name.sauer_write(out);
                f.sauer_write(out);
            }
            EditVarValue::String(s) => {
                2i32.sauer_write(out);
                self.name.sauer_write(out);
                s.sauer_write(out);
            }
        }
    }
}
impl SauerRead for EditVar {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_EDITVAR)?;
        let var_type = i32::sauer_read(reader)
            .in_field("type")
            .in_struct("N_EDITVAR")?;
        let name = CubeString::sauer_read(reader)
            .in_field("name")
            .in_struct("N_EDITVAR")?;
        let val = match var_type {
            0 => EditVarValue::Int(
                i32::sauer_read(reader)
                    .in_field("val")
                    .in_struct("N_EDITVAR")?,
            ),
            1 => EditVarValue::Float(
                f32::sauer_read(reader)
                    .in_field("val")
                    .in_struct("N_EDITVAR")?,
            ),
            2 => EditVarValue::String(
                CubeString::sauer_read(reader)
                    .in_field("val")
                    .in_struct("N_EDITVAR")?,
            ),
            _ => {
                return Err(ParseError::InvalidEnumValue {
                    value: var_type,
                    enum_name: "EditVarValue",
                });
            }
        };
        Ok(Self { name, val })
    }
}

make_struct!(
/// `N_CLIENTPING` Reports/Updates the scoreboard ping value.
[N_CLIENTPING] ClientPing {
    ping: i32,
});

make_struct!(
/// `N_EDITF` Coop Edit: Face Modification.
[N_EDITF] EditFace {
    sel: SelInfo,
    dir: i32,
    mode: EditFaceMode,
});

/// `N_EDITT` Coop Edit: Texture Modification.
#[derive(Debug, Clone, PartialEq)]
pub struct EditTex {
    pub sel: SelInfo,
    pub tex: i32,
    pub all_faces: bool,
    pub extra_data: Vec<u8>,
}
impl SauerWrite for EditTex {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_EDITT.sauer_write(out);
        self.sel.sauer_write(out);
        self.tex.sauer_write(out);
        self.all_faces.sauer_write(out);
        let extra_len = self.extra_data.len() as u16;
        (extra_len as u8).sauer_write(out);
        ((extra_len >> 8) as u8).sauer_write(out);
        out.extend_from_slice(&self.extra_data);
    }
}
impl SauerRead for EditTex {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_EDITT)?;
        let sel = SelInfo::sauer_read(reader)
            .in_field("sel")
            .in_struct("N_EDITT")?;
        let tex = i32::sauer_read(reader)
            .in_field("tex")
            .in_struct("N_EDITT")?;
        let all_faces = bool::sauer_read(reader)
            .in_field("all_faces")
            .in_struct("N_EDITT")?;
        let lo = u8::sauer_read(reader)
            .in_field("extra_lo")
            .in_struct("N_EDITT")? as u16;
        let hi = u8::sauer_read(reader)
            .in_field("extra_hi")
            .in_struct("N_EDITT")? as u16;
        let extra_len = (lo | (hi << 8)) as usize;

        let extra_data = read_slice(reader, extra_len)?.to_vec();

        Ok(Self {
            sel,
            tex,
            all_faces,
            extra_data,
        })
    }
}

make_struct!(
/// `N_EDITM` Coop Edit: Material Modification.
[N_EDITM] EditMat {
    sel: SelInfo,
    mat: i32,
    filter: i32,
});

make_struct!(
/// `N_FLIP` Coop Edit: Flip geometry.
[N_FLIP] Flip {
    sel: SelInfo,
});

make_struct!(
/// `N_ROTATE` Coop Edit: Rotate geometry.
[N_ROTATE] Rotate {
    sel: SelInfo,
    dir: i32,
});

/// `N_REPLACE` Coop Edit: Replace texture.
#[derive(Debug, Clone, PartialEq)]
pub struct Replace {
    pub sel: SelInfo,
    pub old_tex: i32,
    pub new_tex: i32,
    pub in_sel: i32,
    pub extra_data: Vec<u8>,
}
impl SauerWrite for Replace {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_REPLACE.sauer_write(out);
        self.sel.sauer_write(out);
        self.old_tex.sauer_write(out);
        self.new_tex.sauer_write(out);
        self.in_sel.sauer_write(out);
        let extra_len = self.extra_data.len() as u16;
        (extra_len as u8).sauer_write(out);
        ((extra_len >> 8) as u8).sauer_write(out);
        out.extend_from_slice(&self.extra_data);
    }
}
impl SauerRead for Replace {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_REPLACE)?;
        let sel = SelInfo::sauer_read(reader)
            .in_field("sel")
            .in_struct("N_REPLACE")?;
        let old_tex = i32::sauer_read(reader)
            .in_field("old_tex")
            .in_struct("N_REPLACE")?;
        let new_tex = i32::sauer_read(reader)
            .in_field("new_tex")
            .in_struct("N_REPLACE")?;
        let in_sel = i32::sauer_read(reader)
            .in_field("in_sel")
            .in_struct("N_REPLACE")?;
        let lo = u8::sauer_read(reader)
            .in_field("extra_lo")
            .in_struct("N_REPLACE")? as u16;
        let hi = u8::sauer_read(reader)
            .in_field("extra_hi")
            .in_struct("N_REPLACE")? as u16;
        let extra_len = (lo | (hi << 8)) as usize;

        let extra_data = read_slice(reader, extra_len)?.to_vec();

        Ok(Self {
            sel,
            old_tex,
            new_tex,
            in_sel,
            extra_data,
        })
    }
}

make_struct!(
/// `N_DELCUBE` Coop Edit: Delete a cube.
[N_DELCUBE] DelCube {
    sel: SelInfo,
});

/// `N_EDITVSLOT` Coop Edit: Modify texture V-Slots.
#[derive(Debug, Clone, PartialEq)]
pub struct EditVSlot {
    pub sel: SelInfo,
    pub delta: i32,
    pub all_faces: bool,
    pub extra_data: Vec<u8>,
}
impl SauerWrite for EditVSlot {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_EDITVSLOT.sauer_write(out);
        self.sel.sauer_write(out);
        self.delta.sauer_write(out);
        self.all_faces.sauer_write(out);
        let extra_len = self.extra_data.len() as u16;
        (extra_len as u8).sauer_write(out);
        ((extra_len >> 8) as u8).sauer_write(out);
        out.extend_from_slice(&self.extra_data);
    }
}
impl SauerRead for EditVSlot {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_EDITVSLOT)?;
        let sel = SelInfo::sauer_read(reader)
            .in_field("sel")
            .in_struct("N_EDITVSLOT")?;
        let delta = i32::sauer_read(reader)
            .in_field("delta")
            .in_struct("N_EDITVSLOT")?;
        let all_faces = bool::sauer_read(reader)
            .in_field("all_faces")
            .in_struct("N_EDITVSLOT")?;
        let lo = u8::sauer_read(reader)
            .in_field("extra_lo")
            .in_struct("N_EDITVSLOT")? as u16;
        let hi = u8::sauer_read(reader)
            .in_field("extra_hi")
            .in_struct("N_EDITVSLOT")? as u16;
        let extra_len = (lo | (hi << 8)) as usize;

        let extra_data = read_slice(reader, extra_len)?.to_vec();

        Ok(Self {
            sel,
            delta,
            all_faces,
            extra_data,
        })
    }
}

make_struct!(
/// `N_REMIP` Coop Edit: Lighting/mips recalculation.
[N_REMIP] Remip {}
);

make_struct!(
/// `N_NEWMAP` Generates a new empty map of the specified size.
[N_NEWMAP] NewMap {
    size: i32,
});

/// `N_ITEMLIST` Bulk sends the items available on a map (synchronization).
#[derive(Debug, Clone, PartialEq)]
pub struct ItemList {
    pub items: Vec<data::ItemSpawn>,
}
impl SauerWrite for ItemList {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_ITEMLIST.sauer_write(out);
        for item in &self.items {
            item.sauer_write(out);
        }
        (-1i32).sauer_write(out);
    }
}
impl SauerRead for ItemList {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_ITEMLIST)?;
        let mut items = Vec::new();
        loop {
            // peek if -1
            let mut temp_reader = *reader;
            if i32::sauer_read(&mut temp_reader)? < 0 {
                break;
            }

            items.push(data::ItemSpawn::sauer_read(reader)?);
        }
        Ok(Self { items })
    }
}

make_struct!(
/// `N_SERVCMD` Raw server command.
[N_SERVCMD] ServCmd {
    cmd: CubeString,
});

// =====================================================================
// SERVER-SPECIFIC STRUCTS
// =====================================================================

make_struct!(
/// `N_DEMOPACKET` Demo playback marker.
[N_DEMOPACKET] DemoPacket {}
);

make_struct!(
/// `N_SERVINFO` Initial server information sent when a client starts connecting.
[N_SERVINFO] ServInfo {
    client_num: i32,
    protocol: i32,
    session_id: i32,
    has_password: bool,
    server_desc: CubeString,
    server_auth: CubeString,
});

make_struct!(
/// `N_WELCOME` The client is fully welcomed and joined into the game.
[N_WELCOME] Welcome {}
);

make_struct!(
/// `N_PAUSEGAME` Sets or removes the paused state of the game.
[N_PAUSEGAME] PauseGameServer {
    is_paused: bool,
    client_num: i32,
});

make_struct!(
/// `N_GAMESPEED` Modifies the speed of the game.
[N_GAMESPEED] GameSpeedServer {
    speed: i32,
    client_num: i32,
});

make_struct!(
/// `N_SAYTEAM` Emits a team chat message.
[N_SAYTEAM] SayTeamServer {
    client_num: i32,
    text: CubeString,
});

make_struct!(
/// `N_MAPCHANGE` Notifies clients to change the current map and gamemode.
[N_MAPCHANGE] MapChange {
    map_name: CubeString,
    mode: GameMode,
    has_items: bool,
});

make_struct!(
/// `N_FORCEDEATH` Forces a client to die instantly.
[N_FORCEDEATH] ForceDeath {
    client_num: i32,
});

make_struct!(
/// `N_INITCLIENT` A new client has connected.
[N_INITCLIENT] InitClient {
    client_num: i32,
    name: CubeString,
    team: CubeString,
    player_model: PlayerModel,
});

make_struct!(
/// `N_CDIS` A client has fully disconnected.
[N_CDIS] ClientDisconnected {
    client_num: i32,
});

make_struct!(
/// `N_SPAWNSTATE` Confirms the specific state (health, ammo, etc.) of a client who just spawned.
[N_SPAWNSTATE] SpawnState {
    client_num: i32,
    state: PlayerState,
});

make_struct!(
/// `N_SHOTFX` Replicates weapon firing effects across clients.
[N_SHOTFX] ShotFx {
    client_num: i32,
    gun: Gun,
    id: i32,
    from: Vec3<f32> => data::vec3::write_quantized::<DMF>, data::vec3::read_quantized::<DMF>,
    to: Vec3<f32> => data::vec3::write_quantized::<DMF>, data::vec3::read_quantized::<DMF>,
});

make_struct!(
/// `N_EXPLODEFX` Replicates projectile explosion effects across clients.
[N_EXPLODEFX] ExplodeFx {
    client_num: i32,
    gun: Gun,
    id: i32,
});

make_struct!(
/// `N_DAMAGE` Applies damage to a target client.
[N_DAMAGE] Damage {
    target_cn: i32,
    actor_cn: i32,
    damage: i32,
    armour: i32,
    health: i32,
});

make_struct!(
/// `N_HITPUSH` Applies physical knockback from weapon hits to a client.
[N_HITPUSH] HitPush {
    target_cn: i32,
    gun: Gun,
    damage: i32,
    dir: Vec3<f32> => data::vec3::write_quantized::<DMF>, data::vec3::read_quantized::<DMF>,
});

make_struct!(
/// `N_DIED` Notifies that a player was killed by an actor.
[N_DIED] Died {
    victim_cn: i32,
    actor_cn: i32,
    actor_frags: i32,
    team_frags: i32,
});

/// `N_TEAMINFO` Transmits team scores (frags).
#[derive(Debug, Clone, PartialEq)]
pub struct TeamInfo {
    pub teams: Vec<data::TeamInfo>,
}
impl SauerWrite for TeamInfo {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_TEAMINFO.sauer_write(out);
        for team in &self.teams {
            team.sauer_write(out);
        }
        0u8.sauer_write(out); // Empty string terminator
    }
}
impl SauerRead for TeamInfo {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_TEAMINFO)?;
        let mut teams = Vec::new();
        loop {
            // peek if 0
            let mut temp_reader = *reader;
            if i32::sauer_read(&mut temp_reader)? <= 0 {
                break;
            }

            teams.push(data::TeamInfo::sauer_read(reader)?);
        }
        Ok(Self { teams })
    }
}

/// `N_RESUME` Sent during a late join to tell the connecting client about the state of all players.
#[derive(Debug, Clone, PartialEq)]
pub struct Resume {
    pub clients: Vec<ClientResumeState>,
}
impl SauerWrite for Resume {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_RESUME.sauer_write(out);
        for c in &self.clients {
            c.sauer_write(out);
        }
        (-1i32).sauer_write(out);
    }
}
impl SauerRead for Resume {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_RESUME)?;
        let mut clients = Vec::new();
        loop {
            // peek if -1
            let mut temp_reader = *reader;
            if i32::sauer_read(&mut temp_reader)? < 0 {
                break;
            }
            clients.push(ClientResumeState::sauer_read(reader)?);
        }
        Ok(Self { clients })
    }
}

make_struct!(
/// `N_ITEMSPAWN` Notifies that an item has spawned on the map.
[N_ITEMSPAWN] ItemSpawn {
    item_index: i32,
});

make_struct!(
/// `N_ITEMACC` Acknowledges an item pickup.
[N_ITEMACC] ItemAck {
    item_index: i32,
    client_num: i32,
});

/// `N_CLIPBOARD` Pushes map clipboard data (for cooperative editing) to the client.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipboardServer {
    pub client_num: i32,
    pub unpack_len: i32,
    pub pack_len: i32,
    pub data: Vec<u8>,
}
impl SauerWrite for ClipboardServer {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_CLIPBOARD.sauer_write(out);
        self.client_num.sauer_write(out);
        self.unpack_len.sauer_write(out);
        self.pack_len.sauer_write(out);
        out.extend_from_slice(&self.data);
    }
}
impl SauerRead for ClipboardServer {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_CLIPBOARD)?;
        let client_num = i32::sauer_read(reader)?;
        let unpack_len = i32::sauer_read(reader)?;
        let pack_len = i32::sauer_read(reader)?;
        let len = if pack_len > 0 { pack_len as usize } else { 0 };
        let data = read_slice(reader, len)?.to_vec();

        Ok(Self {
            client_num,
            unpack_len,
            pack_len,
            data,
        })
    }
}

/// `N_UNDO` Sends geometry undo data to the client.
#[derive(Debug, Clone, PartialEq)]
pub struct UndoServer {
    pub client_num: i32,
    pub unpack_len: i32,
    pub pack_len: i32,
    pub data: Vec<u8>,
}
impl SauerWrite for UndoServer {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_UNDO.sauer_write(out);
        self.client_num.sauer_write(out);
        self.unpack_len.sauer_write(out);
        self.pack_len.sauer_write(out);
        out.extend_from_slice(&self.data);
    }
}
impl SauerRead for UndoServer {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_UNDO)?;
        let client_num = i32::sauer_read(reader)?;
        let unpack_len = i32::sauer_read(reader)?;
        let pack_len = i32::sauer_read(reader)?;
        let len = if pack_len > 0 { pack_len as usize } else { 0 };
        let data = read_slice(reader, len)?.to_vec();

        Ok(Self {
            client_num,
            unpack_len,
            pack_len,
            data,
        })
    }
}

/// `N_REDO` Sends geometry redo data to the client.
#[derive(Debug, Clone, PartialEq)]
pub struct RedoServer {
    pub client_num: i32,
    pub unpack_len: i32,
    pub pack_len: i32,
    pub data: Vec<u8>,
}
impl SauerWrite for RedoServer {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_REDO.sauer_write(out);
        self.client_num.sauer_write(out);
        self.unpack_len.sauer_write(out);
        self.pack_len.sauer_write(out);
        out.extend_from_slice(&self.data);
    }
}
impl SauerRead for RedoServer {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_REDO)?;
        let client_num = i32::sauer_read(reader)?;
        let unpack_len = i32::sauer_read(reader)?;
        let pack_len = i32::sauer_read(reader)?;
        let len = if pack_len > 0 { pack_len as usize } else { 0 };
        let data = read_slice(reader, len)?.to_vec();

        Ok(Self {
            client_num,
            unpack_len,
            pack_len,
            data,
        })
    }
}

make_struct!(
/// `N_PONG` Basic ping packet sent to measure round-trip time.
[N_PONG] Pong {
    ping: i32,
});

make_struct!(
/// `N_TIMEUP` Indicates that the current match time limit has been updated.
[N_TIMEUP] TimeUp {
    seconds_remaining: i32,
});

make_struct!(
/// `N_SERVMSG` Displays a direct system/server message in the client's console.
[N_SERVMSG] ServMsg {
    text: CubeString,
});

/// `N_SENDDEMOLIST` Replicates the list of available demos back to the client.
#[derive(Debug, Clone, PartialEq)]
pub struct SendDemoList {
    pub demos: Vec<CubeString>,
}
impl SauerWrite for SendDemoList {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_SENDDEMOLIST.sauer_write(out);
        (self.demos.len() as i32).sauer_write(out);
        for demo in &self.demos {
            demo.sauer_write(out);
        }
    }
}
impl SauerRead for SendDemoList {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_SENDDEMOLIST)?;
        let count = i32::sauer_read(reader)?;
        let mut demos = Vec::with_capacity(count as usize);
        for _ in 0..count {
            demos.push(CubeString::sauer_read(reader)?);
        }
        Ok(Self { demos })
    }
}

make_struct!(
/// `N_DEMOPLAYBACK` Informs the client that the server has started or stopped demo playback.
[N_DEMOPLAYBACK] DemoPlayback {
    is_on: bool,
    client_num: i32,
});

/// `N_CURRENTMASTER` Notifies clients of the current master mode and list of clients with privileges.
#[derive(Debug, Clone, PartialEq)]
pub struct CurrentMaster {
    pub master_mode: MasterMode,
    pub clients: Vec<MasterClient>,
}
impl SauerWrite for CurrentMaster {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_CURRENTMASTER.sauer_write(out);
        (self.master_mode as i32).sauer_write(out);
        for c in &self.clients {
            c.sauer_write(out);
        }
        (-1i32).sauer_write(out);
    }
}
impl SauerRead for CurrentMaster {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_CURRENTMASTER)?;
        let mm_val = i32::sauer_read(reader)?;
        let master_mode =
            MasterMode::try_from(mm_val).map_err(|_| ParseError::InvalidEnumValue {
                value: mm_val,
                enum_name: "MasterMode",
            })?;
        let mut clients = Vec::new();
        loop {
            // peek if -1
            let mut temp_reader = *reader;
            if i32::sauer_read(&mut temp_reader)? < 0 {
                break;
            }

            clients.push(MasterClient::sauer_read(reader)?);
        }
        Ok(Self {
            master_mode,
            clients,
        })
    }
}

make_struct!(
/// `N_MASTERMODE` Updates the global Master Mode.
[N_MASTERMODE] MasterModeServer {
    master_mode: MasterMode,
});

make_struct!(
/// `N_SPECTATOR` Forces a player into or out of Spectator Mode.
[N_SPECTATOR] SpectatorServer {
    client_num: i32,
    is_spectator: bool,
});

make_struct!(
/// `N_SETTEAM` Moves a player to a specific team.
[N_SETTEAM] SetTeamServer {
    client_num: i32,
    team: CubeString,
    reason: i32,
});

make_struct!(
/// `N_ANNOUNCE` Server announces important item spawns natively (e.g., Quad Damage).
[N_ANNOUNCE] Announce {
    item_type: ItemType,
});

make_struct!(
/// `N_REQAUTH` Initiates an authentication request from the server.
[N_REQAUTH] ReqAuth {
    auth_domain: CubeString,
});

make_struct!(
/// `N_AUTHCHAL` Sends the cryptographic challenge down to the client.
[N_AUTHCHAL] AuthChal {
    desc: CubeString,
    id: u32,
    challenge: CubeString,
});

make_struct!(
/// `N_INITAI` Spawns a server-side AI Bot into the client's game world.
[N_INITAI] InitAi {
    bot_cn: i32,
    owner_cn: i32,
    ai_type: AiType,
    skill: i32,
    player_model: PlayerModel,
    name: CubeString,
    team: CubeString,
});

/// `N_BASES` Sends the list of capture bases for the current map (Capture / Regen Capture).
#[derive(Debug, Clone, PartialEq)]
pub struct BasesServer {
    pub bases: Vec<data::BaseInfo>,
}
impl SauerWrite for BasesServer {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_BASES.sauer_write(out);
        (self.bases.len() as i32).sauer_write(out);
        for b in &self.bases {
            b.sauer_write(out);
        }
    }
}
impl SauerRead for BasesServer {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_BASES)?;
        let num_bases = i32::sauer_read(reader)?;
        if num_bases < 0 || num_bases > 256 {
            return Err(ParseError::ValueOutOfRange {
                field: "num_bases",
                value: num_bases,
                max: 256,
            });
        }
        let mut bases = Vec::with_capacity(num_bases as usize);
        for _ in 0..num_bases {
            bases.push(data::BaseInfo::sauer_read(reader)?);
        }
        Ok(Self { bases })
    }
}

make_struct!(
/// `N_BASEINFO` Updates the state of a single capture base.
[N_BASEINFO] BaseInfo {
    base: i32,
    owner: CubeString,
    enemy: CubeString,
    converted: bool,
    ammo: i32,
});

make_struct!(
/// `N_BASESCORE` Updates the score for a team on a capture base.
[N_BASESCORE] BaseScore {
    base: i32,
    team: CubeString,
    total: i32,
});

make_struct!(
/// `N_REPAMMO` Replenishes a player's ammo (Regen Capture).
[N_REPAMMO] RepAmmoServer {
    client_num: i32,
    ammotype: ItemType,
});

make_struct!(
/// `N_BASEREGEN` Regenerates a player's health and armour at a capture base.
[N_BASEREGEN] BaseRegen {
    client_num: i32,
    health: i32,
    armour: i32,
    ammotype: ItemType,
    ammo: i32,
});

/// `N_INITFLAGS` Sends initial flag states to joining clients (CTF modes).
#[derive(Debug, Clone, PartialEq)]
pub struct InitFlagsServer {
    pub scores: [i32; 2],
    pub flags: Vec<FlagState>,
}
impl SauerWrite for InitFlagsServer {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_INITFLAGS.sauer_write(out);
        self.scores[0].sauer_write(out);
        self.scores[1].sauer_write(out);
        (self.flags.len() as i32).sauer_write(out);
        for f in &self.flags {
            f.version.sauer_write(out);
            f.spawn_index.sauer_write(out);
            if let FlagStateOwner::Owned { owner } = &f.owner {
                owner.sauer_write(out);
            } else {
                (-1i32).sauer_write(out);
            }
            (if f.invis { 1i32 } else { 0i32 }).sauer_write(out);
            if let Some(hold) = &f.hold {
                match hold {
                    FlagStateHoldTeam::None => (-1i32).sauer_write(out),
                    FlagStateHoldTeam::Team { team, hold_time } => {
                        team.sauer_write(out);
                        hold_time.sauer_write(out);
                    }
                }
            }
        }
    }
}
impl SauerRead for InitFlagsServer {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_INITFLAGS)?;
        let scores = [i32::sauer_read(reader)?, i32::sauer_read(reader)?];
        let num_flags = i32::sauer_read(reader)?;
        if num_flags < 0 || num_flags > 256 {
            return Err(ParseError::ValueOutOfRange {
                field: "num_flags",
                value: num_flags,
                max: 256,
            });
        }
        let mut flags = Vec::with_capacity(num_flags as usize);
        for _ in 0..num_flags {
            let version = i32::sauer_read(reader)?;
            let spawn_index = i32::sauer_read(reader)?;
            let owner_id = i32::sauer_read(reader)?;
            let invis = i32::sauer_read(reader)? != 0;
            let owner = if owner_id < 0 {
                FlagStateOwner::Dropped {
                    dropped: i32::sauer_read(reader)? != 0,
                    drop_loc: data::vec3::read_quantized::<DMF>(reader)?,
                }
            } else {
                FlagStateOwner::Owned { owner: owner_id }
            };
            let hold = if reader.is_empty() {
                None
            } else {
                let team = i32::sauer_read(reader)?;
                Some(if team < 0 {
                    FlagStateHoldTeam::None
                } else {
                    FlagStateHoldTeam::Team {
                        team,
                        hold_time: i32::sauer_read(reader)?,
                    }
                })
            };
            flags.push(FlagState {
                version,
                spawn_index,
                owner,
                invis,
                hold,
            });
        }
        Ok(Self { scores, flags })
    }
}

make_struct!(
/// `N_DROPFLAG` A player dropped a flag.
[N_DROPFLAG] DropFlag {
    client_num: i32,
    flag: i32,
    version: i32,
    drop_loc: Vec3<f32> => data::vec3::write_quantized::<DMF>, data::vec3::read_quantized::<DMF>,
});

make_struct!(
/// `N_SCOREFLAG` A player scored with a flag.
[N_SCOREFLAG] ScoreFlag {
    client_num: i32,
    relay_flag: i32,
    relay_version: i32,
    goal_flag: i32,
    goal_version: i32,
    goal_spawn: i32,
    team: i32,
    score: i32,
    oflags: i32,
});

make_struct!(
/// `N_RETURNFLAG` A player returned a flag to its base.
[N_RETURNFLAG] ReturnFlag {
    client_num: i32,
    flag: i32,
    version: i32,
});

make_struct!(
/// `N_TAKEFLAG` A player picked up a flag.
[N_TAKEFLAG] TakeFlagServer {
    client_num: i32,
    flag: i32,
    version: i32,
});

make_struct!(
/// `N_RESETFLAG` A flag was reset to its spawn location.
[N_RESETFLAG] ResetFlag {
    flag: i32,
    version: i32,
    spawn_index: i32,
    team: i32,
    score: i32,
});

make_struct!(
/// `N_INVISFLAG` Marks a flag as invisible or visible.
[N_INVISFLAG] InvisFlag {
    flag: i32,
    invis: bool,
});

/// `N_INITTOKENS` Sends initial token states to joining clients (Collect mode).
#[derive(Debug, Clone, PartialEq)]
pub struct InitTokensServer {
    pub scores: [i32; 2],
    pub tokens: Vec<TokenState>,
    pub client_tokens: Vec<ClientTokenState>,
}
impl SauerWrite for InitTokensServer {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_INITTOKENS.sauer_write(out);
        self.scores[0].sauer_write(out);
        self.scores[1].sauer_write(out);
        (self.tokens.len() as i32).sauer_write(out);
        for t in &self.tokens {
            t.sauer_write(out);
        }
        for ct in &self.client_tokens {
            ct.sauer_write(out);
        }
        (-1i32).sauer_write(out);
    }
}
impl SauerRead for InitTokensServer {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_INITTOKENS)?;
        let scores = [i32::sauer_read(reader)?, i32::sauer_read(reader)?];
        let num_tokens = i32::sauer_read(reader)?;
        if num_tokens < 0 || num_tokens > 256 {
            return Err(ParseError::ValueOutOfRange {
                field: "num_tokens",
                value: num_tokens,
                max: 256,
            });
        }
        let mut tokens = Vec::with_capacity(num_tokens as usize);
        for _ in 0..num_tokens {
            tokens.push(TokenState::sauer_read(reader)?);
        }
        let mut client_tokens = Vec::new();
        loop {
            // peek if -1
            let mut temp_reader = *reader;
            if i32::sauer_read(&mut temp_reader)? < 0 {
                break;
            }

            client_tokens.push(ClientTokenState::sauer_read(reader)?);
        }
        Ok(Self {
            scores,
            tokens,
            client_tokens,
        })
    }
}

make_struct!(
/// `N_TAKETOKEN` A player collected a token.
[N_TAKETOKEN] TakeTokenServer {
    client_num: i32,
    id: i32,
    total: i32,
});

/// `N_EXPIRETOKENS` Notifies that specific tokens have expired.
#[derive(Debug, Clone, PartialEq)]
pub struct ExpireTokens {
    pub tokens: Vec<i32>,
}
impl SauerWrite for ExpireTokens {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_EXPIRETOKENS.sauer_write(out);
        for &t in &self.tokens {
            t.sauer_write(out);
        }
        (-1i32).sauer_write(out);
    }
}
impl SauerRead for ExpireTokens {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_EXPIRETOKENS)?;
        let mut tokens = Vec::new();
        loop {
            let id = i32::sauer_read(reader)?;
            if id < 0 {
                break;
            }
            tokens.push(id);
        }
        Ok(Self { tokens })
    }
}

/// `N_DROPTOKENS` A player dropped their held tokens.
#[derive(Debug, Clone, PartialEq)]
pub struct DropTokens {
    pub client_num: i32,
    pub drop_loc: Vec3<f32>,
    pub tokens: Vec<DroppedToken>,
}
impl SauerWrite for DropTokens {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_DROPTOKENS.sauer_write(out);
        self.client_num.sauer_write(out);
        data::vec3::write_quantized::<DMF>(&self.drop_loc, out);
        for t in &self.tokens {
            t.sauer_write(out);
        }
        (-1i32).sauer_write(out);
    }
}
impl SauerRead for DropTokens {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_DROPTOKENS)?;
        let client_num = i32::sauer_read(reader)?;
        let drop_loc = data::vec3::read_quantized::<DMF>(reader)?;
        let mut tokens = Vec::new();
        loop {
            // peek if -1
            let mut temp_reader = *reader;
            if i32::sauer_read(&mut temp_reader)? < 0 {
                break;
            }

            tokens.push(DroppedToken::sauer_read(reader)?);
        }
        Ok(Self {
            client_num,
            drop_loc,
            tokens,
        })
    }
}

/// `N_STEALTOKENS` A player stole tokens from the enemy team.
#[derive(Debug, Clone, PartialEq)]
pub struct StealTokens {
    pub client_num: i32,
    pub team: i32,
    pub basenum: i32,
    pub enemy_team: i32,
    pub score: i32,
    pub drop_loc: Vec3<f32>,
    pub tokens: Vec<StolenToken>,
}
impl SauerWrite for StealTokens {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_STEALTOKENS.sauer_write(out);
        self.client_num.sauer_write(out);
        self.team.sauer_write(out);
        self.basenum.sauer_write(out);
        self.enemy_team.sauer_write(out);
        self.score.sauer_write(out);
        data::vec3::write_quantized::<DMF>(&self.drop_loc, out);
        for t in &self.tokens {
            t.sauer_write(out);
        }
        (-1i32).sauer_write(out);
    }
}
impl SauerRead for StealTokens {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_STEALTOKENS)?;
        let client_num = i32::sauer_read(reader)?;
        let team = i32::sauer_read(reader)?;
        let basenum = i32::sauer_read(reader)?;
        let enemy_team = i32::sauer_read(reader)?;
        let score = i32::sauer_read(reader)?;
        let drop_loc = data::vec3::read_quantized::<DMF>(reader)?;
        let mut tokens = Vec::new();
        loop {
            // peek if -1
            let mut temp_reader = *reader;
            if i32::sauer_read(&mut temp_reader)? < 0 {
                break;
            }

            tokens.push(StolenToken::sauer_read(reader)?);
        }
        Ok(Self {
            client_num,
            team,
            basenum,
            enemy_team,
            score,
            drop_loc,
            tokens,
        })
    }
}

make_struct!(
/// `N_DEPOSITTOKENS` A player deposited tokens at a base.
[N_DEPOSITTOKENS] DepositTokensServer {
    client_num: i32,
    base: i32,
    deposited: i32,
    team: i32,
    score: i32,
    flags: i32,
});

// =====================================================================
// CLIENT-PACKET ONLY STRUCTS (Wrapped in N_CLIENT)
// =====================================================================

make_struct!(
/// `N_SPAWN` Sent to instruct a client to spawn in the game.
[N_SPAWN] SpawnServer {
    state: PlayerState,
});

make_struct!(
/// `N_TAUNT` Indicates that a player is taunting.
[N_TAUNT] Taunt {}
);

make_struct!(
/// `N_EDITENT` Coop Edit: Modification of map entities.
[N_EDITENT] EditEntServer {
    ent_id: i32,
    pos: Vec3<f32> => data::vec3::write_quantized::<DMF>, data::vec3::read_quantized::<DMF>,
    type_: EntityType,
    attr1: i32,
    attr2: i32,
    attr3: i32,
    attr4: i32,
    attr5: i32,
});

make_struct!(
/// `N_EDITMODE` Notifies that a player entered or left Edit Mode.
[N_EDITMODE] EditModeServer {
    is_editing: bool,
});

make_struct!(
/// `N_COPY` Coop Edit: Copy geometry.
[N_COPY] CopyServer {
    sel: SelInfo,
});

make_struct!(
/// `N_PASTE` Coop Edit: Paste geometry.
[N_PASTE] PasteServer {
    sel: SelInfo,
});

// =====================================================================
// CLIENT-SPECIFIC STRUCTS
// =====================================================================

make_struct!(
/// `N_CONNECT` Attempts to connect to the server with user details and passwords.
[N_CONNECT] Connect {
    name: CubeString,
    player_model: PlayerModel,
    password_hash: CubeString,
    auth_desc: CubeString,
    auth_name: CubeString,
});

make_struct!(
/// `N_AUTHANS` Provides the mathematical answer to a server's AuthChal.
[N_AUTHANS] AuthAnswer {
    desc: CubeString,
    id: u32,
    answer: CubeString,
});

make_struct!(
/// `N_PING` Measures the roundtrip time.
[N_PING] Ping {
    time: i32,
});

make_struct!(
/// `N_FROMAI` Alters the context of subsequent messages in the packet to act as if they came from the specified AI bot.
[N_FROMAI] FromAi {
    qcn: i32,
});

make_struct!(
/// `N_EDITMODE` The client toggled cooperative edit mode locally.
[N_EDITMODE] EditModeClient {
    val: bool,
});

make_struct!(
/// `N_MAPCRC` The client verifies the CRC of the map they currently have loaded.
[N_MAPCRC] MapCrc {
    map_name: CubeString,
    crc: i32,
});

make_struct!(
/// `N_CHECKMAPS` Forces a manual check of map CRCs on the server.
[N_CHECKMAPS] CheckMaps {}
);

make_struct!(
/// `N_TRYSPAWN` Requests to spawn from the server.
[N_TRYSPAWN] TrySpawn {}
);

make_struct!(
/// `N_SPAWN` The client actually spawns.
[N_SPAWN] SpawnClient {
    life_sequence: i32,
    gun_select: Gun,
});

make_struct!(
/// `N_SUICIDE` The client forcefully kills themselves.
[N_SUICIDE] Suicide {}
);

/// `N_SHOOT` The client fired a weapon.
#[derive(Debug, Clone, PartialEq)]
pub struct Shoot {
    pub id: i32,
    pub gun: Gun,
    pub from: Vec3<f32>,
    pub to: Vec3<f32>,
    pub hits: Vec<HitInfo>,
}
impl SauerWrite for Shoot {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_SHOOT.sauer_write(out);
        self.id.sauer_write(out);
        (self.gun as i32).sauer_write(out);
        data::vec3::write_quantized::<DMF>(&self.from, out);
        data::vec3::write_quantized::<DMF>(&self.to, out);
        (self.hits.len() as i32).sauer_write(out);
        for h in &self.hits {
            h.sauer_write(out);
        }
    }
}
impl SauerRead for Shoot {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_SHOOT)?;
        let id = i32::sauer_read(reader)?;
        let gun = Gun::try_from(i32::sauer_read(reader)?).unwrap_or(Gun::Fist);
        let from = data::vec3::read_quantized::<DMF>(reader)?;
        let to = data::vec3::read_quantized::<DMF>(reader)?;
        let hits_count = i32::sauer_read(reader)?;
        if hits_count < 0 || hits_count > 128 {
            return Err(ParseError::ValueOutOfRange {
                field: "hits_count",
                value: hits_count,
                max: 128,
            });
        }
        let mut hits = Vec::with_capacity(hits_count as usize);
        for _ in 0..hits_count {
            hits.push(HitInfo::sauer_read(reader)?);
        }
        Ok(Self {
            id,
            gun,
            from,
            to,
            hits,
        })
    }
}

/// `N_EXPLODE` Projectile explosion triggered by the client.
#[derive(Debug, Clone, PartialEq)]
pub struct Explode {
    pub millis: i32,
    pub gun: Gun,
    pub id: i32,
    pub hits: Vec<HitInfo>,
}
impl SauerWrite for Explode {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_EXPLODE.sauer_write(out);
        self.millis.sauer_write(out);
        (self.gun as i32).sauer_write(out);
        self.id.sauer_write(out);
        (self.hits.len() as i32).sauer_write(out);
        for h in &self.hits {
            h.sauer_write(out);
        }
    }
}
impl SauerRead for Explode {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_EXPLODE)?;
        let millis = i32::sauer_read(reader)?;
        let gun = Gun::try_from(i32::sauer_read(reader)?).unwrap_or(Gun::Fist);
        let id = i32::sauer_read(reader)?;
        let hits_count = i32::sauer_read(reader)?;
        if hits_count < 0 || hits_count > 128 {
            return Err(ParseError::ValueOutOfRange {
                field: "hits_count",
                value: hits_count,
                max: 128,
            });
        }
        let mut hits = Vec::with_capacity(hits_count as usize);
        for _ in 0..hits_count {
            hits.push(HitInfo::sauer_read(reader)?);
        }
        Ok(Self {
            millis,
            gun,
            id,
            hits,
        })
    }
}

make_struct!(
/// `N_ITEMPICKUP` The client requested to pick up an item.
[N_ITEMPICKUP] ItemPickup {
    item_index: i32,
});

make_struct!(
/// `N_SAYTEAM` Submits a team chat message.
[N_SAYTEAM] SayTeamClient {
    text: CubeString,
});

make_struct!(
/// `N_SWITCHTEAM` The client requests to switch teams.
[N_SWITCHTEAM] SwitchTeam {
    team: CubeString,
});

make_struct!(
/// `N_MAPVOTE` The client votes to change the map and mode.
[N_MAPVOTE] MapVote {
    map_name: CubeString,
    mode: GameMode,
});

make_struct!(
/// `N_EDITENT` Coop Edit: A map entity was modified.
[N_EDITENT] EditEntClient {
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
});

make_struct!(
/// `N_MASTERMODE` Requests to change the global Master Mode.
[N_MASTERMODE] MasterModeClient {
    mode: MasterMode,
});

make_struct!(
/// `N_CLEARBANS` Requests to clear all bans.
[N_CLEARBANS] ClearBans {}
);

make_struct!(
/// `N_KICK` Requests to kick a player.
[N_KICK] Kick {
    victim_cn: i32,
    reason: CubeString,
});

make_struct!(
/// `N_SPECTATOR` Requests to move someone (or self) to/from the spectator team.
[N_SPECTATOR] SpectatorClient {
    spectator_cn: i32,
    val: bool,
});

make_struct!(
/// `N_SETTEAM` Requests to force someone to a specific team.
[N_SETTEAM] SetTeamClient {
    client_num: i32,
    team: CubeString,
});

make_struct!(
/// `N_FORCEINTERMISSION` Requests to force the current match to end.
[N_FORCEINTERMISSION] ForceIntermission {}
);

make_struct!(
/// `N_RECORDDEMO` Toggles automatic demo recording for the next match.
[N_RECORDDEMO] RecordDemo {
    val: bool,
});

make_struct!(
/// `N_STOPDEMO` Stops the current demo recording.
[N_STOPDEMO] StopDemo {}
);

make_struct!(
/// `N_CLEARDEMOS` Clears a specific demo (or all) from server memory.
[N_CLEARDEMOS] ClearDemos {
    demo_id: i32,
});

make_struct!(
/// `N_LISTDEMOS` Requests a listing of all available demos on the server.
[N_LISTDEMOS] ListDemos {}
);

make_struct!(
/// `N_GETDEMO` Requests to download a specific demo from the server.
[N_GETDEMO] GetDemo {
    demo_id: i32,
    tag: i32,
});

make_struct!(
/// `N_GETMAP` Requests to download the current map from the server (Coop Edit).
[N_GETMAP] GetMap {}
);

make_struct!(
/// `N_SETMASTER` Requests to set master privileges using a password hash.
[N_SETMASTER] SetMaster {
    client_num: i32,
    val: bool,
    hash: CubeString,
});

make_struct!(
/// `N_ADDBOT` Requests to add a bot with specific skill.
[N_ADDBOT] AddBot {
    skill: i32,
});

make_struct!(
/// `N_DELBOT` Requests to delete a bot.
[N_DELBOT] DelBot {}
);

make_struct!(
/// `N_BOTLIMIT` Modifies the global bot limit.
[N_BOTLIMIT] BotLimit {
    limit: i32,
});

make_struct!(
/// `N_BOTBALANCE` Modifies the global bot auto-balance flag.
[N_BOTBALANCE] BotBalance {
    balance: bool,
});

make_struct!(
/// `N_AUTHTRY` Submits a request to authenticate and claim privileges via cryptographic keys.
[N_AUTHTRY] AuthTry {
    desc: CubeString,
    name: CubeString,
});

make_struct!(
/// `N_AUTHKICK` Submits a request to kick an individual via cryptographic auth verification.
[N_AUTHKICK] AuthKick {
    desc: CubeString,
    name: CubeString,
    victim_cn: i32,
    reason: CubeString,
});

make_struct!(
/// `N_PAUSEGAME` Toggles game paused state.
[N_PAUSEGAME] PauseGameClient {
    val: bool,
});

make_struct!(
/// `N_GAMESPEED` Changes global game speed.
[N_GAMESPEED] GameSpeedClient {
    speed: i32,
});

make_struct!(
/// `N_COPY` Coop Edit: Client copies geometry to their clipboard.
[N_COPY] CopyClient {}
);

make_struct!(
/// `N_PASTE` Coop Edit: Client requests to paste geometry from their clipboard.
[N_PASTE] PasteClient {}
);

/// `N_CLIPBOARD` Coop Edit: Uploads raw clipboard geometry data.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipboardClient {
    pub unpack_len: i32,
    pub pack_len: i32,
    pub data: Vec<u8>,
}
impl SauerWrite for ClipboardClient {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_CLIPBOARD.sauer_write(out);
        self.unpack_len.sauer_write(out);
        self.pack_len.sauer_write(out);
        out.extend_from_slice(&self.data);
    }
}
impl SauerRead for ClipboardClient {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_CLIPBOARD)?;
        let unpack_len = i32::sauer_read(reader)?;
        let pack_len = i32::sauer_read(reader)?;
        let len = if pack_len > 0 { pack_len as usize } else { 0 };
        let data = read_slice(reader, len)?.to_vec();

        Ok(Self {
            unpack_len,
            pack_len,
            data,
        })
    }
}

/// `N_UNDO` Coop Edit: Undo event.
#[derive(Debug, Clone, PartialEq)]
pub struct UndoClient {
    pub unpack_len: i32,
    pub pack_len: i32,
    pub data: Vec<u8>,
}
impl SauerWrite for UndoClient {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_UNDO.sauer_write(out);
        self.unpack_len.sauer_write(out);
        self.pack_len.sauer_write(out);
        out.extend_from_slice(&self.data);
    }
}
impl SauerRead for UndoClient {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_UNDO)?;
        let unpack_len = i32::sauer_read(reader)?;
        let pack_len = i32::sauer_read(reader)?;
        let len = if pack_len > 0 { pack_len as usize } else { 0 };
        let data = read_slice(reader, len)?.to_vec();

        Ok(Self {
            unpack_len,
            pack_len,
            data,
        })
    }
}

/// `N_REDO` Coop Edit: Redo event.
#[derive(Debug, Clone, PartialEq)]
pub struct RedoClient {
    pub unpack_len: i32,
    pub pack_len: i32,
    pub data: Vec<u8>,
}
impl SauerWrite for RedoClient {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_REDO.sauer_write(out);
        self.unpack_len.sauer_write(out);
        self.pack_len.sauer_write(out);
        out.extend_from_slice(&self.data);
    }
}
impl SauerRead for RedoClient {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_REDO)?;
        let unpack_len = i32::sauer_read(reader)?;
        let pack_len = i32::sauer_read(reader)?;
        let len = if pack_len > 0 { pack_len as usize } else { 0 };
        let data = read_slice(reader, len)?.to_vec();

        Ok(Self {
            unpack_len,
            pack_len,
            data,
        })
    }
}

/// `N_BASES` Sends the list of capture base spawns to the server (Capture / Regen Capture).
#[derive(Debug, Clone, PartialEq)]
pub struct BasesClient {
    pub bases: Vec<BaseSpawn>,
}
impl SauerWrite for BasesClient {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_BASES.sauer_write(out);
        (self.bases.len() as i32).sauer_write(out);
        for b in &self.bases {
            b.ammotype.map_or(0, |t| t as i32).sauer_write(out);
            data::vec3::write_quantized::<DMF>(&b.pos, out);
        }
    }
}
impl SauerRead for BasesClient {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_BASES)?;
        let num_bases = i32::sauer_read(reader)?;
        if num_bases < 0 || num_bases > 256 {
            return Err(ParseError::ValueOutOfRange {
                field: "num_bases",
                value: num_bases,
                max: 256,
            });
        }
        let mut bases = Vec::with_capacity(num_bases as usize);
        for _ in 0..num_bases {
            let ammotype_val = i32::sauer_read(reader)?;
            let ammotype = if ammotype_val == 0 {
                None
            } else {
                Some(ItemType::try_from(ammotype_val).map_err(|_| {
                    ParseError::InvalidEnumValue {
                        value: ammotype_val,
                        enum_name: "ItemType",
                    }
                })?)
            };
            bases.push(BaseSpawn {
                ammotype,
                pos: data::vec3::read_quantized::<DMF>(reader)?,
            });
        }
        Ok(Self { bases })
    }
}

make_struct!(
/// `N_REPAMMO` Requests ammo replenishment (Regen Capture).
[N_REPAMMO] RepAmmoClient {}
);

make_struct!(
/// `N_TRYDROPFLAG` Client attempts to drop the flag they are carrying.
[N_TRYDROPFLAG] TryDropFlag {}
);

make_struct!(
/// `N_TAKEFLAG` Client acknowledges picking up a flag.
[N_TAKEFLAG] TakeFlagClient {
    flag: i32,
    version: i32,
});

/// `N_INITFLAGS` Sends initial flag spawn positions to the server (CTF modes).
#[derive(Debug, Clone, PartialEq)]
pub struct InitFlagsClient {
    pub flags: Vec<FlagSpawn>,
}
impl SauerWrite for InitFlagsClient {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_INITFLAGS.sauer_write(out);
        (self.flags.len() as i32).sauer_write(out);
        for f in &self.flags {
            f.sauer_write(out);
        }
    }
}
impl SauerRead for InitFlagsClient {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_INITFLAGS)?;
        let num_flags = i32::sauer_read(reader)?;
        if num_flags < 0 || num_flags > 256 {
            return Err(ParseError::ValueOutOfRange {
                field: "num_flags",
                value: num_flags,
                max: 256,
            });
        }
        let mut flags = Vec::with_capacity(num_flags as usize);
        for _ in 0..num_flags {
            flags.push(FlagSpawn::sauer_read(reader)?);
        }
        Ok(Self { flags })
    }
}

/// `N_INITTOKENS` Sends initial token base spawn data to the server (Collect mode).
#[derive(Debug, Clone, PartialEq)]
pub struct InitTokensClient {
    pub bases: Vec<TokenBaseSpawn>,
}
impl SauerWrite for InitTokensClient {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        N_INITTOKENS.sauer_write(out);
        (self.bases.len() as i32).sauer_write(out);
        for b in &self.bases {
            b.sauer_write(out);
        }
    }
}
impl SauerRead for InitTokensClient {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        check_tag(reader, N_INITTOKENS)?;
        let num_bases = i32::sauer_read(reader)?;
        if num_bases < 0 || num_bases > 256 {
            return Err(ParseError::ValueOutOfRange {
                field: "num_bases",
                value: num_bases,
                max: 256,
            });
        }
        let mut bases = Vec::with_capacity(num_bases as usize);
        for _ in 0..num_bases {
            bases.push(TokenBaseSpawn::sauer_read(reader)?);
        }
        Ok(Self { bases })
    }
}

make_struct!(
/// `N_TAKETOKEN` Client collected a token.
[N_TAKETOKEN] TakeTokenClient {
    id: i32,
});

make_struct!(
/// `N_DEPOSITTOKENS` Client deposited tokens at a base.
[N_DEPOSITTOKENS] DepositTokensClient {
    base: i32,
});
