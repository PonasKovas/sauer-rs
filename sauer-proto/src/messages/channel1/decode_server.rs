use super::*;
use crate::{
    Chan1Server, ParseError,
    data::{
        AiType, ClientResumeState, ClientState, ClientTokenState, DroppedToken, EditFaceMode,
        EditVarValue, EntityType, FlagState, FlagStateHoldTeam, FlagStateOwner, GameMode, Gun,
        ItemType, MasterClient, MasterMode, PlayerModel, PlayerState, Privilege, SelInfo,
        StolenToken, TokenState, Vector3,
    },
    internal::{Decode, ParseResultExt, ReaderCore, constants::*},
};

fn packet_name(msg_type: i32) -> &'static str {
    match msg_type {
        N_DEMOPACKET => "DemoPacket",
        N_SERVINFO => "ServInfo",
        N_WELCOME => "Welcome",
        N_PAUSEGAME => "PauseGame",
        N_GAMESPEED => "GameSpeed",
        N_SOUND => "Sound",
        N_TEXT => "Text",
        N_SAYTEAM => "SayTeam",
        N_MAPCHANGE => "MapChange",
        N_FORCEDEATH => "ForceDeath",
        N_ITEMLIST => "ItemList",
        N_INITCLIENT => "InitClient",
        N_SWITCHNAME => "SwitchName",
        N_SWITCHMODEL => "SwitchModel",
        N_CDIS => "ClientDisconnected",
        N_SPAWN => "Spawn",
        N_SPAWNSTATE => "SpawnState",
        N_SHOTFX => "ShotFx",
        N_EXPLODEFX => "ExplodeFx",
        N_DAMAGE => "Damage",
        N_HITPUSH => "HitPush",
        N_DIED => "Died",
        N_TEAMINFO => "TeamInfo",
        N_GUNSELECT => "GunSelect",
        N_TAUNT => "Taunt",
        N_RESUME => "Resume",
        N_ITEMSPAWN => "ItemSpawn",
        N_ITEMACC => "ItemAck",
        N_CLIPBOARD => "Clipboard",
        N_UNDO => "Undo",
        N_REDO => "Redo",
        N_EDITF => "EditFace",
        N_EDITT => "EditTex",
        N_EDITM => "EditMat",
        N_FLIP => "Flip",
        N_COPY => "Copy",
        N_PASTE => "Paste",
        N_ROTATE => "Rotate",
        N_REPLACE => "Replace",
        N_DELCUBE => "DelCube",
        N_EDITVSLOT => "EditVSlot",
        N_REMIP => "Remip",
        N_EDITENT => "EditEnt",
        N_EDITVAR => "EditVar",
        N_PONG => "Pong",
        N_CLIENTPING => "ClientPing",
        N_TIMEUP => "TimeUp",
        N_SERVMSG => "ServMsg",
        N_SENDDEMOLIST => "SendDemoList",
        N_DEMOPLAYBACK => "DemoPlayback",
        N_CURRENTMASTER => "CurrentMaster",
        N_MASTERMODE => "MasterMode",
        N_EDITMODE => "EditMode",
        N_SPECTATOR => "Spectator",
        N_SETTEAM => "SetTeam",
        N_ANNOUNCE => "Announce",
        N_NEWMAP => "NewMap",
        N_REQAUTH => "ReqAuth",
        N_AUTHCHAL => "AuthChal",
        N_INITAI => "InitAi",
        N_SERVCMD => "ServCmd",
        N_INITFLAGS => "InitFlags",
        N_DROPFLAG => "DropFlag",
        N_SCOREFLAG => "ScoreFlag",
        N_RETURNFLAG => "ReturnFlag",
        N_TAKEFLAG => "TakeFlag",
        N_RESETFLAG => "ResetFlag",
        N_INVISFLAG => "InvisFlag",
        N_BASES => "Bases",
        N_BASEINFO => "BaseInfo",
        N_BASESCORE => "BaseScore",
        N_REPAMMO => "RepAmmo",
        N_BASEREGEN => "BaseRegen",
        N_INITTOKENS => "InitTokens",
        N_TAKETOKEN => "TakeToken",
        N_EXPIRETOKENS => "ExpireTokens",
        N_DROPTOKENS => "DropTokens",
        N_STEALTOKENS => "StealTokens",
        N_DEPOSITTOKENS => "DepositTokens",
        _ => "unknown",
    }
}

impl Decode for Chan1Server {
    fn decode(msg_type: i32, r: &mut ReaderCore) -> Result<Self, ParseError> {
        decode_inner(msg_type, r).in_packet(packet_name(msg_type))
    }
}

fn decode_inner(msg_type: i32, r: &mut ReaderCore) -> Result<Chan1Server, ParseError> {
    match msg_type {
        N_DEMOPACKET => Ok(Chan1Server::DemoPacket(DemoPacket)),

        N_SERVINFO => Ok(Chan1Server::ServInfo(ServInfo {
            client_num: r.get_int()?,
            protocol: r.get_int()?,
            session_id: r.get_int()?,
            has_password: r.get_int()? > 0,
            server_desc: r.get_string()?,
            server_auth: r.get_string()?,
        })),

        N_WELCOME => Ok(Chan1Server::Welcome(Welcome)),

        N_PAUSEGAME => Ok(Chan1Server::PauseGame(PauseGameServer {
            is_paused: r.get_int()? > 0,
            client_num: r.get_int()?,
        })),

        N_GAMESPEED => Ok(Chan1Server::GameSpeed(GameSpeedServer {
            speed: r.get_int()?,
            client_num: r.get_int()?,
        })),

        N_SOUND => Ok(Chan1Server::Client(Client {
            client_num: r.active_client_num(packet_name(msg_type))?,
            packet: ClientPacket::Sound(Sound {
                sound_id: r.get_int()?,
            }),
        })),

        N_TEXT => Ok(Chan1Server::Client(Client {
            client_num: r.active_client_num(packet_name(msg_type))?,
            packet: ClientPacket::Text(Text {
                text: r.get_string()?,
            }),
        })),

        N_SAYTEAM => Ok(Chan1Server::SayTeam(SayTeamServer {
            client_num: r.get_int()?,
            text: r.get_string()?,
        })),

        N_MAPCHANGE => {
            let map_name = r.get_string()?;
            let mode_val = r.get_int()?;
            let mode = GameMode::try_from(mode_val).map_err(|_| ParseError::InvalidEnumValue {
                value: mode_val,
                enum_name: "GameMode",
            })?;
            Ok(Chan1Server::MapChange(MapChange {
                map_name,
                mode,
                has_items: r.get_int()? != 0,
            }))
        }

        N_FORCEDEATH => Ok(Chan1Server::ForceDeath(ForceDeath {
            client_num: r.get_int()?,
        })),

        N_ITEMLIST => {
            let mut items = Vec::new();
            loop {
                let item_index = r.get_int()?;
                if item_index < 0 {
                    break;
                }
                let item_type_val = r.get_int()?;
                items.push(data::ItemSpawn {
                    item_index,
                    item_type: ItemType::try_from(item_type_val).map_err(|_| {
                        ParseError::InvalidEnumValue {
                            value: item_type_val,
                            enum_name: "ItemType",
                        }
                    })?,
                });
            }
            Ok(Chan1Server::ItemList(ItemList { items }))
        }

        N_INITCLIENT => {
            let client_num = r.get_int()?;
            let name = r.get_string()?;
            let team = r.get_string()?;
            let pm_val = r.get_int()?;
            let player_model =
                PlayerModel::try_from(pm_val).map_err(|_| ParseError::InvalidEnumValue {
                    value: pm_val,
                    enum_name: "PlayerModel",
                })?;
            Ok(Chan1Server::InitClient(InitClient {
                client_num,
                name,
                team,
                player_model,
            }))
        }

        N_SWITCHNAME => Ok(Chan1Server::Client(Client {
            client_num: r.active_client_num(packet_name(msg_type))?,
            packet: ClientPacket::SwitchName(SwitchName {
                name: r.get_string()?,
            }),
        })),

        N_SWITCHMODEL => {
            let model_val = r.get_int()?;
            Ok(Chan1Server::Client(Client {
                client_num: r.active_client_num(packet_name(msg_type))?,
                packet: ClientPacket::SwitchModel(SwitchModel {
                    model: PlayerModel::try_from(model_val).map_err(|_| {
                        ParseError::InvalidEnumValue {
                            value: model_val,
                            enum_name: "PlayerModel",
                        }
                    })?,
                }),
            }))
        }

        N_CDIS => Ok(Chan1Server::ClientDisconnected(ClientDisconnected {
            client_num: r.get_int()?,
        })),

        N_SPAWN => Ok(Chan1Server::Client(Client {
            client_num: r.active_client_num(packet_name(msg_type))?,
            packet: ClientPacket::Spawn(SpawnServer {
                state: PlayerState::decode(r).in_field("state")?,
            }),
        })),

        N_SPAWNSTATE => Ok(Chan1Server::SpawnState(SpawnState {
            client_num: r.get_int()?,
            state: PlayerState::decode(r).in_field("state")?,
        })),

        N_SHOTFX => {
            let client_num = r.get_int()?;
            let gun_val = r.get_int()?;
            let id = r.get_int()?;
            let from = Vector3 {
                x: r.get_int()? as f32 / DMF,
                y: r.get_int()? as f32 / DMF,
                z: r.get_int()? as f32 / DMF,
            };
            let to = Vector3 {
                x: r.get_int()? as f32 / DMF,
                y: r.get_int()? as f32 / DMF,
                z: r.get_int()? as f32 / DMF,
            };
            Ok(Chan1Server::ShotFx(ShotFx {
                client_num,
                gun: Gun::try_from(gun_val).unwrap_or(Gun::Fist),
                id,
                from,
                to,
            }))
        }

        N_EXPLODEFX => Ok(Chan1Server::ExplodeFx(ExplodeFx {
            client_num: r.get_int()?,
            gun: Gun::try_from(r.get_int()?).unwrap_or(Gun::Fist),
            id: r.get_int()?,
        })),

        N_DAMAGE => Ok(Chan1Server::Damage(Damage {
            target_cn: r.get_int()?,
            actor_cn: r.get_int()?,
            damage: r.get_int()?,
            armour: r.get_int()?,
            health: r.get_int()?,
        })),

        N_HITPUSH => Ok(Chan1Server::HitPush(HitPush {
            target_cn: r.get_int()?,
            gun: Gun::try_from(r.get_int()?).unwrap_or(Gun::Fist),
            damage: r.get_int()?,
            dir: Vector3 {
                x: r.get_int()? as f32 / DMF,
                y: r.get_int()? as f32 / DMF,
                z: r.get_int()? as f32 / DMF,
            },
        })),

        N_DIED => Ok(Chan1Server::Died(Died {
            victim_cn: r.get_int()?,
            actor_cn: r.get_int()?,
            actor_frags: r.get_int()?,
            team_frags: r.get_int()?,
        })),

        N_TEAMINFO => {
            let mut teams = Vec::new();
            loop {
                let team = r.get_string()?;
                if team.is_empty() {
                    break;
                }
                let frags = r.get_int()?;
                teams.push(data::TeamInfo { team, frags });
            }
            Ok(Chan1Server::TeamInfo(TeamInfo { teams }))
        }

        N_GUNSELECT => Ok(Chan1Server::Client(Client {
            client_num: r.active_client_num(packet_name(msg_type))?,
            packet: ClientPacket::GunSelect(GunSelect {
                gun: Gun::try_from(r.get_int()?).unwrap_or(Gun::Fist),
            }),
        })),

        N_TAUNT => Ok(Chan1Server::Client(Client {
            client_num: r.active_client_num(packet_name(msg_type))?,
            packet: ClientPacket::Taunt(Taunt),
        })),

        N_RESUME => {
            let mut clients = Vec::new();
            loop {
                let cn = r.get_int()?;
                if cn < 0 {
                    break;
                }
                let state_val = r.get_int()?;
                let state =
                    ClientState::try_from(state_val).map_err(|_| ParseError::InvalidEnumValue {
                        value: state_val,
                        enum_name: "ClientState",
                    })?;
                let frags = r.get_int()?;
                let flags = r.get_int()?;
                let deaths = r.get_int()?;
                let quad_millis = r.get_int()?;
                let player_state = PlayerState::decode(r).in_field("player_state")?;

                clients.push(ClientResumeState {
                    client_num: cn,
                    state,
                    frags,
                    flags,
                    deaths,
                    quad_millis,
                    player_state,
                });
            }
            Ok(Chan1Server::Resume(Resume { clients }))
        }

        N_ITEMSPAWN => Ok(Chan1Server::ItemSpawn(ItemSpawn {
            item_index: r.get_int()?,
        })),

        N_ITEMACC => Ok(Chan1Server::ItemAck(ItemAck {
            item_index: r.get_int()?,
            client_num: r.get_int()?,
        })),

        N_CLIPBOARD => {
            let client_num = r.get_int()?;
            let unpack_len = r.get_int()?;
            let pack_len = r.get_int()?;
            let data = r
                .get_bytes(if pack_len > 0 { pack_len as usize } else { 0 })?
                .to_vec();
            Ok(Chan1Server::Clipboard(ClipboardServer {
                client_num,
                unpack_len,
                pack_len,
                data,
            }))
        }

        N_UNDO => {
            let client_num = r.get_int()?;
            let unpack_len = r.get_int()?;
            let pack_len = r.get_int()?;
            let data = r
                .get_bytes(if pack_len > 0 { pack_len as usize } else { 0 })?
                .to_vec();
            Ok(Chan1Server::Undo(UndoServer {
                client_num,
                unpack_len,
                pack_len,
                data,
            }))
        }

        N_REDO => {
            let client_num = r.get_int()?;
            let unpack_len = r.get_int()?;
            let pack_len = r.get_int()?;
            let data = r
                .get_bytes(if pack_len > 0 { pack_len as usize } else { 0 })?
                .to_vec();
            Ok(Chan1Server::Redo(RedoServer {
                client_num,
                unpack_len,
                pack_len,
                data,
            }))
        }

        N_EDITF => {
            let sel = SelInfo::decode(r).in_field("sel")?;
            let dir = r.get_int()?;
            let mode_val = r.get_int()?;
            Ok(Chan1Server::Client(Client {
                client_num: r.active_client_num(packet_name(msg_type))?,
                packet: ClientPacket::EditFace(EditFace {
                    sel,
                    dir,
                    mode: EditFaceMode::try_from(mode_val).map_err(|_| {
                        ParseError::InvalidEnumValue {
                            value: mode_val,
                            enum_name: "EditFaceMode",
                        }
                    })?,
                }),
            }))
        }

        N_EDITT => {
            let sel = SelInfo::decode(r).in_field("sel")?;
            let tex = r.get_int()?;
            let all_faces = r.get_int()? != 0;
            let extra_lo = r.get_byte()? as u16;
            let extra_hi = r.get_byte()? as u16;
            let extra = extra_lo | (extra_hi << 8);
            let extra_data = r.get_bytes(extra as usize)?.to_vec();
            Ok(Chan1Server::Client(Client {
                client_num: r.active_client_num(packet_name(msg_type))?,
                packet: ClientPacket::EditTex(EditTex {
                    sel,
                    tex,
                    all_faces,
                    extra_data,
                }),
            }))
        }

        N_EDITM => Ok(Chan1Server::Client(Client {
            client_num: r.active_client_num(packet_name(msg_type))?,
            packet: ClientPacket::EditMat(EditMat {
                sel: SelInfo::decode(r).in_field("sel")?,
                mat: r.get_int()?,
                filter: r.get_int()?,
            }),
        })),

        N_FLIP => Ok(Chan1Server::Client(Client {
            client_num: r.active_client_num(packet_name(msg_type))?,
            packet: ClientPacket::Flip(Flip {
                sel: SelInfo::decode(r).in_field("sel")?,
            }),
        })),
        N_COPY => Ok(Chan1Server::Client(Client {
            client_num: r.active_client_num(packet_name(msg_type))?,
            packet: ClientPacket::Copy(CopyServer {
                sel: SelInfo::decode(r).in_field("sel")?,
            }),
        })),
        N_PASTE => Ok(Chan1Server::Client(Client {
            client_num: r.active_client_num(packet_name(msg_type))?,
            packet: ClientPacket::Paste(PasteServer {
                sel: SelInfo::decode(r).in_field("sel")?,
            }),
        })),

        N_ROTATE => Ok(Chan1Server::Client(Client {
            client_num: r.active_client_num(packet_name(msg_type))?,
            packet: ClientPacket::Rotate(Rotate {
                sel: SelInfo::decode(r).in_field("sel")?,
                dir: r.get_int()?,
            }),
        })),

        N_REPLACE => {
            let client_num = r.active_client_num(packet_name(msg_type))?;
            let sel = SelInfo::decode(r).in_field("sel")?;
            let old_tex = r.get_int()?;
            let new_tex = r.get_int()?;
            let in_sel = r.get_int()?;
            let extra_lo = r.get_byte()? as u16;
            let extra_hi = r.get_byte()? as u16;
            let extra = extra_lo | (extra_hi << 8);
            let extra_data = r.get_bytes(extra as usize)?.to_vec();
            Ok(Chan1Server::Client(Client {
                client_num,
                packet: ClientPacket::Replace(Replace {
                    sel,
                    old_tex,
                    new_tex,
                    in_sel,
                    extra_data,
                }),
            }))
        }

        N_DELCUBE => Ok(Chan1Server::Client(Client {
            client_num: r.active_client_num(packet_name(msg_type))?,
            packet: ClientPacket::DelCube(DelCube {
                sel: SelInfo::decode(r).in_field("sel")?,
            }),
        })),

        N_EDITVSLOT => {
            let client_num = r.active_client_num(packet_name(msg_type))?;
            let sel = SelInfo::decode(r).in_field("sel")?;
            let delta = r.get_int()?;
            let all_faces = r.get_int()? != 0;
            let extra_lo = r.get_byte()? as u16;
            let extra_hi = r.get_byte()? as u16;
            let extra = extra_lo | (extra_hi << 8);
            let extra_data = r.get_bytes(extra as usize)?.to_vec();
            Ok(Chan1Server::Client(Client {
                client_num,
                packet: ClientPacket::EditVSlot(EditVSlot {
                    sel,
                    delta,
                    all_faces,
                    extra_data,
                }),
            }))
        }

        N_REMIP => Ok(Chan1Server::Client(Client {
            client_num: r.active_client_num(packet_name(msg_type))?,
            packet: ClientPacket::Remip(Remip {}),
        })),

        N_EDITENT => {
            let client_num = r.active_client_num(packet_name(msg_type))?;
            let ent_id = r.get_int()?;
            let pos = Vector3 {
                x: r.get_int()? as f32 / DMF,
                y: r.get_int()? as f32 / DMF,
                z: r.get_int()? as f32 / DMF,
            };
            let type_val = r.get_int()?;
            Ok(Chan1Server::Client(Client {
                client_num,
                packet: ClientPacket::EditEnt(EditEntServer {
                    ent_id,
                    pos,
                    type_: EntityType::try_from(type_val).map_err(|_| {
                        ParseError::InvalidEnumValue {
                            value: type_val,
                            enum_name: "EntityType",
                        }
                    })?,
                    attr1: r.get_int()?,
                    attr2: r.get_int()?,
                    attr3: r.get_int()?,
                    attr4: r.get_int()?,
                    attr5: r.get_int()?,
                }),
            }))
        }

        N_EDITVAR => {
            let client_num = r.active_client_num(packet_name(msg_type))?;
            let var_type = r.get_int()?;
            let name = r.get_string()?;
            let val = match var_type {
                0 => EditVarValue::Int(r.get_int()?),
                1 => EditVarValue::Float(r.get_float()?),
                2 => EditVarValue::String(r.get_string()?),
                _ => {
                    return Err(ParseError::InvalidEnumValue {
                        value: var_type,
                        enum_name: "EditVarValue",
                    });
                }
            };
            Ok(Chan1Server::Client(Client {
                client_num,
                packet: ClientPacket::EditVar(EditVar { name, val }),
            }))
        }

        N_PONG => Ok(Chan1Server::Pong(Pong { ping: r.get_int()? })),

        N_CLIENTPING => Ok(Chan1Server::Client(Client {
            client_num: r.active_client_num(packet_name(msg_type))?,
            packet: ClientPacket::ClientPing(ClientPing { ping: r.get_int()? }),
        })),

        N_TIMEUP => Ok(Chan1Server::TimeUp(TimeUp {
            seconds_remaining: r.get_int()?,
        })),

        N_SERVMSG => Ok(Chan1Server::ServMsg(ServMsg {
            text: r.get_string()?,
        })),

        N_SENDDEMOLIST => {
            let count = r.get_int()?;
            let mut demos = Vec::new();
            for _ in 0..count {
                demos.push(r.get_string()?);
            }
            Ok(Chan1Server::SendDemoList(SendDemoList { demos }))
        }

        N_DEMOPLAYBACK => Ok(Chan1Server::DemoPlayback(DemoPlayback {
            is_on: r.get_int()? > 0,
            client_num: r.get_int()?,
        })),

        N_CURRENTMASTER => {
            let mm_val = r.get_int()?;
            let master_mode =
                MasterMode::try_from(mm_val).map_err(|_| ParseError::InvalidEnumValue {
                    value: mm_val,
                    enum_name: "MasterMode",
                })?;

            let mut clients = Vec::new();
            loop {
                let mn = r.get_int()?;
                if mn < 0 {
                    break;
                }
                let priv_val = r.get_int()?;
                clients.push(MasterClient {
                    client_num: mn,
                    privilege: Privilege::try_from(priv_val).map_err(|_| {
                        ParseError::InvalidEnumValue {
                            value: priv_val,
                            enum_name: "Privilege",
                        }
                    })?,
                });
            }
            Ok(Chan1Server::CurrentMaster(CurrentMaster {
                master_mode,
                clients,
            }))
        }

        N_MASTERMODE => {
            let mm_val = r.get_int()?;
            Ok(Chan1Server::MasterMode(MasterModeServer {
                master_mode: MasterMode::try_from(mm_val).map_err(|_| {
                    ParseError::InvalidEnumValue {
                        value: mm_val,
                        enum_name: "MasterMode",
                    }
                })?,
            }))
        }

        N_EDITMODE => Ok(Chan1Server::Client(Client {
            client_num: r.active_client_num(packet_name(msg_type))?,
            packet: ClientPacket::EditMode(EditModeServer {
                is_editing: r.get_int()? > 0,
            }),
        })),

        N_SPECTATOR => Ok(Chan1Server::Spectator(SpectatorServer {
            client_num: r.get_int()?,
            is_spectator: r.get_int()? > 0,
        })),

        N_SETTEAM => Ok(Chan1Server::SetTeam(SetTeamServer {
            client_num: r.get_int()?,
            team: r.get_string()?,
            reason: r.get_int()?,
        })),

        N_ANNOUNCE => {
            let t = r.get_int()?;
            Ok(Chan1Server::Announce(Announce {
                item_type: ItemType::try_from(t).map_err(|_| ParseError::InvalidEnumValue {
                    value: t,
                    enum_name: "ItemType",
                })?,
            }))
        }

        N_NEWMAP => Ok(match r.active_client_num("NewMap") {
            Ok(client_num) => Chan1Server::Client(Client {
                client_num,
                packet: ClientPacket::NewMap(NewMap { size: r.get_int()? }),
            }),
            Err(_) => Chan1Server::NewMap(NewMap { size: r.get_int()? }),
        }),

        N_REQAUTH => Ok(Chan1Server::ReqAuth(ReqAuth {
            auth_domain: r.get_string()?,
        })),

        N_AUTHCHAL => Ok(Chan1Server::AuthChal(AuthChal {
            desc: r.get_string()?,
            id: r.get_int()? as u32,
            challenge: r.get_string()?,
        })),

        N_INITAI => {
            let bot_cn = r.get_int()?;
            let owner_cn = r.get_int()?;
            let ai_type_val = r.get_int()?;
            let skill = r.get_int()?;
            let pm_val = r.get_int()?;
            Ok(Chan1Server::InitAi(InitAi {
                bot_cn,
                owner_cn,
                ai_type: AiType::try_from(ai_type_val).map_err(|_| {
                    ParseError::InvalidEnumValue {
                        value: ai_type_val,
                        enum_name: "AiType",
                    }
                })?,
                skill,
                player_model: PlayerModel::try_from(pm_val).map_err(|_| {
                    ParseError::InvalidEnumValue {
                        value: pm_val,
                        enum_name: "PlayerModel",
                    }
                })?,
                name: r.get_string()?,
                team: r.get_string()?,
            }))
        }

        N_SERVCMD => Ok(Chan1Server::ServCmd(ServCmd {
            cmd: r.get_string()?,
        })),

        N_INITFLAGS => {
            let scores = [r.get_int()?, r.get_int()?];
            let num_flags = r.get_int()?;
            if num_flags < 0 || num_flags > 256 {
                return Err(ParseError::ValueOutOfRange {
                    field: "num_flags",
                    value: num_flags,
                    max: 256,
                });
            }

            let mut flags = Vec::with_capacity(num_flags as usize);

            for _ in 0..num_flags {
                let version = r.get_int()?;
                let spawn_index = r.get_int()?;
                let owner_id = r.get_int()?;
                let invis = r.get_int()? != 0;

                let owner = if owner_id < 0 {
                    FlagStateOwner::Dropped {
                        dropped: r.get_int()? != 0,
                        drop_loc: Vector3 {
                            x: r.get_int()? as f32 / DMF,
                            y: r.get_int()? as f32 / DMF,
                            z: r.get_int()? as f32 / DMF,
                        },
                    }
                } else {
                    FlagStateOwner::Owned { owner: owner_id }
                };

                let hold = if r.peek(1).is_none() {
                    None
                } else {
                    let team = r.get_int()?;

                    Some(if team < 0 {
                        FlagStateHoldTeam::None
                    } else {
                        FlagStateHoldTeam::Team {
                            team,
                            hold_time: r.get_int()?,
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

            Ok(Chan1Server::InitFlags(InitFlagsServer { scores, flags }))
        }

        N_DROPFLAG => Ok(Chan1Server::DropFlag(DropFlag {
            client_num: r.get_int()?,
            flag: r.get_int()?,
            version: r.get_int()?,
            drop_loc: Vector3 {
                x: r.get_int()? as f32 / DMF,
                y: r.get_int()? as f32 / DMF,
                z: r.get_int()? as f32 / DMF,
            },
        })),

        N_SCOREFLAG => Ok(Chan1Server::ScoreFlag(ScoreFlag {
            client_num: r.get_int()?,
            relay_flag: r.get_int()?,
            relay_version: r.get_int()?,
            goal_flag: r.get_int()?,
            goal_version: r.get_int()?,
            goal_spawn: r.get_int()?,
            team: r.get_int()?,
            score: r.get_int()?,
            oflags: r.get_int()?,
        })),

        N_RETURNFLAG => Ok(Chan1Server::ReturnFlag(ReturnFlag {
            client_num: r.get_int()?,
            flag: r.get_int()?,
            version: r.get_int()?,
        })),

        N_TAKEFLAG => Ok(Chan1Server::TakeFlag(TakeFlagServer {
            client_num: r.get_int()?,
            flag: r.get_int()?,
            version: r.get_int()?,
        })),

        N_RESETFLAG => Ok(Chan1Server::ResetFlag(ResetFlag {
            flag: r.get_int()?,
            version: r.get_int()?,
            spawn_index: r.get_int()?,
            team: r.get_int()?,
            score: r.get_int()?,
        })),

        N_INVISFLAG => Ok(Chan1Server::InvisFlag(InvisFlag {
            flag: r.get_int()?,
            invis: r.get_int()? != 0,
        })),

        N_BASES => {
            let num_bases = r.get_int()?;
            if num_bases < 0 || num_bases > 256 {
                return Err(ParseError::ValueOutOfRange {
                    field: "num_bases",
                    value: num_bases,
                    max: 256,
                });
            }
            let mut bases = Vec::with_capacity(num_bases as usize);
            for _ in 0..num_bases {
                let ammotype_val = r.get_int()?;
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
                bases.push(data::BaseInfo {
                    ammotype,
                    owner: r.get_string()?,
                    enemy: r.get_string()?,
                    converted: r.get_int()? != 0,
                    ammo: r.get_int()?,
                });
            }
            Ok(Chan1Server::Bases(BasesServer { bases }))
        }

        N_BASEINFO => {
            let base = r.get_int()?;
            let owner = r.get_string()?;
            let enemy = r.get_string()?;
            let converted = r.get_int()? != 0;
            let ammo = r.get_int()?;
            Ok(Chan1Server::BaseInfo(BaseInfo {
                base,
                owner,
                enemy,
                converted,
                ammo,
            }))
        }

        N_BASESCORE => Ok(Chan1Server::BaseScore(BaseScore {
            base: r.get_int()?,
            team: r.get_string()?,
            total: r.get_int()?,
        })),

        N_REPAMMO => {
            let client_num = r.get_int()?;
            let ammotype_val = r.get_int()?;
            Ok(Chan1Server::RepAmmo(RepAmmoServer {
                client_num,
                ammotype: ItemType::try_from(ammotype_val).map_err(|_| {
                    ParseError::InvalidEnumValue {
                        value: ammotype_val,
                        enum_name: "ItemType",
                    }
                })?,
            }))
        }

        N_BASEREGEN => {
            let client_num = r.get_int()?;
            let health = r.get_int()?;
            let armour = r.get_int()?;
            let ammotype_val = r.get_int()?;
            Ok(Chan1Server::BaseRegen(BaseRegen {
                client_num,
                health,
                armour,
                ammotype: ItemType::try_from(ammotype_val).map_err(|_| {
                    ParseError::InvalidEnumValue {
                        value: ammotype_val,
                        enum_name: "ItemType",
                    }
                })?,
                ammo: r.get_int()?,
            }))
        }

        N_INITTOKENS => {
            let scores = [r.get_int()?, r.get_int()?];
            let num_tokens = r.get_int()?;
            if num_tokens < 0 || num_tokens > 256 {
                return Err(ParseError::ValueOutOfRange {
                    field: "num_tokens",
                    value: num_tokens,
                    max: 256,
                });
            }
            let mut tokens = Vec::with_capacity(num_tokens as usize);
            for _ in 0..num_tokens {
                tokens.push(TokenState {
                    id: r.get_int()?,
                    team: r.get_int()?,
                    yaw: r.get_int()?,
                    pos: Vector3 {
                        x: r.get_int()? as f32 / DMF,
                        y: r.get_int()? as f32 / DMF,
                        z: r.get_int()? as f32 / DMF,
                    },
                });
            }
            let mut client_tokens = Vec::new();
            loop {
                let client_num = r.get_int()?;
                if client_num < 0 {
                    break;
                }
                client_tokens.push(ClientTokenState {
                    client_num,
                    tokens: r.get_int()?,
                });
            }
            Ok(Chan1Server::InitTokens(InitTokensServer {
                scores,
                tokens,
                client_tokens,
            }))
        }

        N_TAKETOKEN => Ok(Chan1Server::TakeToken(TakeTokenServer {
            client_num: r.get_int()?,
            id: r.get_int()?,
            total: r.get_int()?,
        })),

        N_EXPIRETOKENS => {
            let mut tokens = Vec::new();
            loop {
                let id = r.get_int()?;
                if id < 0 {
                    break;
                }
                tokens.push(id);
            }
            Ok(Chan1Server::ExpireTokens(ExpireTokens { tokens }))
        }

        N_DROPTOKENS => {
            let client_num = r.get_int()?;
            let drop_loc = Vector3 {
                x: r.get_int()? as f32 / DMF,
                y: r.get_int()? as f32 / DMF,
                z: r.get_int()? as f32 / DMF,
            };
            let mut tokens = Vec::new();
            loop {
                let id = r.get_int()?;
                if id < 0 {
                    break;
                }
                tokens.push(DroppedToken {
                    id,
                    team: r.get_int()?,
                    yaw: r.get_int()?,
                });
            }
            Ok(Chan1Server::DropTokens(DropTokens {
                client_num,
                drop_loc,
                tokens,
            }))
        }

        N_STEALTOKENS => {
            let client_num = r.get_int()?;
            let team = r.get_int()?;
            let basenum = r.get_int()?;
            let enemy_team = r.get_int()?;
            let score = r.get_int()?;
            let drop_loc = Vector3 {
                x: r.get_int()? as f32 / DMF,
                y: r.get_int()? as f32 / DMF,
                z: r.get_int()? as f32 / DMF,
            };
            let mut tokens = Vec::new();
            loop {
                let id = r.get_int()?;
                if id < 0 {
                    break;
                }
                tokens.push(StolenToken {
                    id,
                    yaw: r.get_int()?,
                });
            }
            Ok(Chan1Server::StealTokens(StealTokens {
                client_num,
                team,
                basenum,
                enemy_team,
                score,
                drop_loc,
                tokens,
            }))
        }

        N_DEPOSITTOKENS => Ok(Chan1Server::DepositTokens(DepositTokensServer {
            client_num: r.get_int()?,
            base: r.get_int()?,
            deposited: r.get_int()?,
            team: r.get_int()?,
            score: r.get_int()?,
            flags: r.get_int()?,
        })),

        _ => Err(ParseError::UnknownMessageType {
            channel: 1,
            id: msg_type,
        }),
    }
}
