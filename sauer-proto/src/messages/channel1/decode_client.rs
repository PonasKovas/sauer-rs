use super::*;
use crate::{
    Decode, ParseError,
    data::{
        BaseSpawn, EditFaceMode, EditVarValue, EntityType, FlagSpawn, GameMode, Gun, ItemSpawn,
        ItemType, MasterMode, PlayerModel, SelInfo, TokenBaseSpawn, Vector3,
    },
    internal::{DecodeCore, ParseResultExt, ReaderCore, constants::*},
    route::{C2S, Chan1},
};

impl Decode<C2S<Chan1>> for Chan1Client {
    fn decode(msg_type: i32, r: &mut ReaderCore) -> Result<Self, ParseError> {
        decode_inner(msg_type, r).in_packet(packet_name(msg_type))
    }
}

fn decode_inner(msg_type: i32, r: &mut ReaderCore) -> Result<Chan1Client, ParseError> {
    match msg_type {
        N_CONNECT => {
            let name = r.get_string()?;
            let pm_val = r.get_int()?;
            let player_model =
                PlayerModel::try_from(pm_val).map_err(|_| ParseError::InvalidEnumValue {
                    value: pm_val,
                    enum_name: "PlayerModel",
                })?;
            let password_hash = r.get_string()?;
            let auth_desc = r.get_string()?;
            let auth_name = r.get_string()?;
            Ok(Chan1Client::Connect(Connect {
                name,
                player_model,
                password_hash,
                auth_desc,
                auth_name,
            }))
        }
        N_AUTHANS => {
            let desc = r.get_string()?;
            let id = r.get_int()? as u32;
            let answer = r.get_string()?;
            Ok(Chan1Client::AuthAnswer(AuthAnswer { desc, id, answer }))
        }
        N_PING => {
            let time = r.get_int()?;
            Ok(Chan1Client::Ping(Ping { time }))
        }
        N_FROMAI => {
            let qcn = r.get_int()?;
            Ok(Chan1Client::FromAi(FromAi { qcn }))
        }
        N_EDITMODE => Ok(Chan1Client::EditMode(EditModeClient {
            val: r.get_int()? != 0,
        })),
        N_MAPCRC => {
            let map_name = r.get_string()?;
            let crc = r.get_int()?;
            Ok(Chan1Client::MapCrc(MapCrc { map_name, crc }))
        }
        N_CHECKMAPS => Ok(Chan1Client::CheckMaps(CheckMaps)),
        N_TRYSPAWN => Ok(Chan1Client::TrySpawn(TrySpawn)),
        N_GUNSELECT => {
            let gun_val = r.get_int()?;
            let gun = Gun::try_from(gun_val).map_err(|_| ParseError::InvalidEnumValue {
                value: gun_val,
                enum_name: "Gun",
            })?;
            Ok(Chan1Client::GunSelect(GunSelect { gun }))
        }
        N_SPAWN => {
            let life_sequence = r.get_int()?;
            let gun_val = r.get_int()?;
            let gun_select = Gun::try_from(gun_val).map_err(|_| ParseError::InvalidEnumValue {
                value: gun_val,
                enum_name: "Gun",
            })?;
            Ok(Chan1Client::Spawn(SpawnClient {
                life_sequence,
                gun_select,
            }))
        }
        N_SUICIDE => Ok(Chan1Client::Suicide(Suicide)),
        N_SHOOT => {
            let id = r.get_int()?;
            let gun_val = r.get_int()?;
            let gun = Gun::try_from(gun_val).map_err(|_| ParseError::InvalidEnumValue {
                value: gun_val,
                enum_name: "Gun",
            })?;

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

            let hits_count = r.get_int()?;
            if hits_count < 0 || hits_count > 128 {
                return Err(ParseError::ValueOutOfRange {
                    field: "hits_count",
                    value: hits_count,
                    max: 128,
                });
            }
            let mut hits = Vec::with_capacity(hits_count as usize);
            for _ in 0..hits_count {
                hits.push(DecodeCore::decode(r).in_field("hit")?);
            }

            Ok(Chan1Client::Shoot(Shoot {
                id,
                gun,
                from,
                to,
                hits,
            }))
        }
        N_EXPLODE => {
            let millis = r.get_int()?;
            let gun_val = r.get_int()?;
            let gun = Gun::try_from(gun_val).map_err(|_| ParseError::InvalidEnumValue {
                value: gun_val,
                enum_name: "Gun",
            })?;
            let id = r.get_int()?;

            let hits_count = r.get_int()?;
            if hits_count < 0 || hits_count > 128 {
                return Err(ParseError::ValueOutOfRange {
                    field: "hits_count",
                    value: hits_count,
                    max: 128,
                });
            }
            let mut hits = Vec::with_capacity(hits_count as usize);
            for _ in 0..hits_count {
                hits.push(DecodeCore::decode(r).in_field("hit")?);
            }

            Ok(Chan1Client::Explode(Explode {
                millis,
                gun,
                id,
                hits,
            }))
        }
        N_ITEMPICKUP => {
            let item_index = r.get_int()?;
            Ok(Chan1Client::ItemPickup(ItemPickup { item_index }))
        }
        N_TEXT => {
            let text = r.get_string()?;
            Ok(Chan1Client::Text(Text { text }))
        }
        N_SAYTEAM => {
            let text = r.get_string()?;
            Ok(Chan1Client::SayTeam(SayTeamClient { text }))
        }
        N_SWITCHNAME => {
            let name = r.get_string()?;
            Ok(Chan1Client::SwitchName(SwitchName { name }))
        }
        N_SWITCHMODEL => {
            let model_val = r.get_int()?;
            Ok(Chan1Client::SwitchModel(SwitchModel {
                model: PlayerModel::try_from(model_val).map_err(|_| {
                    ParseError::InvalidEnumValue {
                        value: model_val,
                        enum_name: "PlayerModel",
                    }
                })?,
            }))
        }
        N_SWITCHTEAM => {
            let team = r.get_string()?;
            Ok(Chan1Client::SwitchTeam(SwitchTeam { team }))
        }
        N_MAPVOTE => {
            let map_name = r.get_string()?;
            let mode_val = r.get_int()?;
            Ok(Chan1Client::MapVote(MapVote {
                map_name,
                mode: GameMode::try_from(mode_val).map_err(|_| ParseError::InvalidEnumValue {
                    value: mode_val,
                    enum_name: "GameMode",
                })?,
            }))
        }
        N_ITEMLIST => {
            let mut items = Vec::new();
            loop {
                let item_index = r.get_int()?;
                if item_index < 0 {
                    break;
                }
                let item_type_val = r.get_int()?;
                let item_type = ItemType::try_from(item_type_val).map_err(|_| {
                    ParseError::InvalidEnumValue {
                        value: item_type_val,
                        enum_name: "ItemType",
                    }
                })?;
                items.push(ItemSpawn {
                    item_index,
                    item_type,
                });
            }
            Ok(Chan1Client::ItemList(ItemList { items }))
        }
        N_EDITENT => {
            let ent_id = r.get_int()?;
            let x = r.get_int()?;
            let y = r.get_int()?;
            let z = r.get_int()?;
            let type_val = r.get_int()?;
            let attr1 = r.get_int()?;
            let attr2 = r.get_int()?;
            let attr3 = r.get_int()?;
            let attr4 = r.get_int()?;
            let attr5 = r.get_int()?;
            Ok(Chan1Client::EditEnt(EditEntClient {
                ent_id,
                x,
                y,
                z,
                type_: EntityType::try_from(type_val).map_err(|_| {
                    ParseError::InvalidEnumValue {
                        value: type_val,
                        enum_name: "EntityType",
                    }
                })?,
                attr1,
                attr2,
                attr3,
                attr4,
                attr5,
            }))
        }
        N_EDITVAR => {
            let var_type = r.get_int()?;
            let name = r.get_string()?;
            let val = match var_type {
                0 => EditVarValue::Int(r.get_int()?),
                1 => EditVarValue::Float(r.get_float()?),
                2 => EditVarValue::String(r.get_string()?),
                _ => {
                    return Err(ParseError::InvalidEnumValue {
                        value: var_type,
                        enum_name: "EditVarType",
                    });
                }
            };
            Ok(Chan1Client::EditVar(EditVar { name, val }))
        }
        N_CLIENTPING => {
            let ping = r.get_int()?;
            Ok(Chan1Client::ClientPing(ClientPing { ping }))
        }
        N_MASTERMODE => {
            let mode_val = r.get_int()?;
            let mode =
                MasterMode::try_from(mode_val).map_err(|_| ParseError::InvalidEnumValue {
                    value: mode_val,
                    enum_name: "MasterMode",
                })?;
            Ok(Chan1Client::MasterMode(MasterModeClient { mode }))
        }
        N_CLEARBANS => Ok(Chan1Client::ClearBans(ClearBans)),
        N_KICK => {
            let victim_cn = r.get_int()?;
            let reason = r.get_string()?;
            Ok(Chan1Client::Kick(Kick { victim_cn, reason }))
        }
        N_SPECTATOR => {
            let spectator_cn = r.get_int()?;
            let val = r.get_int()? != 0;
            Ok(Chan1Client::Spectator(SpectatorClient {
                spectator_cn,
                val,
            }))
        }
        N_SETTEAM => {
            let client_num = r.get_int()?;
            let team = r.get_string()?;
            Ok(Chan1Client::SetTeam(SetTeamClient { client_num, team }))
        }
        N_FORCEINTERMISSION => Ok(Chan1Client::ForceIntermission(ForceIntermission)),
        N_RECORDDEMO => Ok(Chan1Client::RecordDemo(RecordDemo {
            val: r.get_int()? != 0,
        })),
        N_STOPDEMO => Ok(Chan1Client::StopDemo(StopDemo)),
        N_CLEARDEMOS => {
            let demo_id = r.get_int()?;
            Ok(Chan1Client::ClearDemos(ClearDemos { demo_id }))
        }
        N_LISTDEMOS => Ok(Chan1Client::ListDemos(ListDemos)),
        N_GETDEMO => {
            let demo_id = r.get_int()?;
            let tag = r.get_int()?;
            Ok(Chan1Client::GetDemo(GetDemo { demo_id, tag }))
        }
        N_GETMAP => Ok(Chan1Client::GetMap(GetMap)),
        N_NEWMAP => {
            let size = r.get_int()?;
            Ok(Chan1Client::NewMap(NewMap { size }))
        }
        N_SETMASTER => {
            let client_num = r.get_int()?;
            let val = r.get_int()? != 0;
            let hash = r.get_string()?;
            Ok(Chan1Client::SetMaster(SetMaster {
                client_num,
                val,
                hash,
            }))
        }
        N_ADDBOT => {
            let skill = r.get_int()?;
            Ok(Chan1Client::AddBot(AddBot { skill }))
        }
        N_DELBOT => Ok(Chan1Client::DelBot(DelBot)),
        N_BOTLIMIT => {
            let limit = r.get_int()?;
            Ok(Chan1Client::BotLimit(BotLimit { limit }))
        }
        N_BOTBALANCE => Ok(Chan1Client::BotBalance(BotBalance {
            balance: r.get_int()? != 0,
        })),
        N_AUTHTRY => {
            let desc = r.get_string()?;
            let name = r.get_string()?;
            Ok(Chan1Client::AuthTry(AuthTry { desc, name }))
        }
        N_AUTHKICK => {
            let desc = r.get_string()?;
            let name = r.get_string()?;
            let victim_cn = r.get_int()?;
            let reason = r.get_string()?;
            Ok(Chan1Client::AuthKick(AuthKick {
                desc,
                name,
                victim_cn,
                reason,
            }))
        }
        N_PAUSEGAME => Ok(Chan1Client::PauseGame(PauseGameClient {
            val: r.get_int()? != 0,
        })),
        N_GAMESPEED => {
            let speed = r.get_int()?;
            Ok(Chan1Client::GameSpeed(GameSpeedClient { speed }))
        }
        N_COPY => Ok(Chan1Client::Copy(CopyClient)),
        N_PASTE => Ok(Chan1Client::Paste(PasteClient)),
        N_CLIPBOARD => {
            let unpack_len = r.get_int()?;
            let pack_len = r.get_int()?;
            let data = r.get_bytes(pack_len.max(0) as usize)?.to_vec();
            Ok(Chan1Client::Clipboard(ClipboardClient {
                unpack_len,
                pack_len,
                data,
            }))
        }
        N_UNDO => {
            let unpack_len = r.get_int()?;
            let pack_len = r.get_int()?;
            let data = r.get_bytes(pack_len.max(0) as usize)?.to_vec();
            Ok(Chan1Client::Undo(UndoClient {
                unpack_len,
                pack_len,
                data,
            }))
        }
        N_REDO => {
            let unpack_len = r.get_int()?;
            let pack_len = r.get_int()?;
            let data = r.get_bytes(pack_len.max(0) as usize)?.to_vec();
            Ok(Chan1Client::Redo(RedoClient {
                unpack_len,
                pack_len,
                data,
            }))
        }
        N_EDITF => {
            let sel: SelInfo = DecodeCore::decode(r).in_field("sel")?;
            let dir = r.get_int()?;
            let mode_val = r.get_int()?;
            Ok(Chan1Client::EditFace(EditFace {
                sel,
                dir,
                mode: EditFaceMode::try_from(mode_val).map_err(|_| {
                    ParseError::InvalidEnumValue {
                        value: mode_val,
                        enum_name: "EditFaceMode",
                    }
                })?,
            }))
        }
        N_EDITT => {
            let sel: SelInfo = DecodeCore::decode(r).in_field("sel")?;
            let tex = r.get_int()?;
            let all_faces = r.get_int()? != 0;

            let len_lower = r.get_byte()? as u16;
            let len_upper = r.get_byte()? as u16;
            let extra_len = len_lower | (len_upper << 8);
            let extra_data = r.get_bytes(extra_len as usize)?.to_vec();

            Ok(Chan1Client::EditTex(EditTex {
                sel,
                tex,
                all_faces,
                extra_data,
            }))
        }
        N_EDITM => {
            let sel: SelInfo = DecodeCore::decode(r).in_field("sel")?;
            let mat = r.get_int()?;
            let filter = r.get_int()?;
            Ok(Chan1Client::EditMat(EditMat { sel, mat, filter }))
        }
        N_FLIP => {
            let sel: SelInfo = DecodeCore::decode(r).in_field("sel")?;
            Ok(Chan1Client::Flip(Flip { sel }))
        }
        N_ROTATE => {
            let sel: SelInfo = DecodeCore::decode(r).in_field("sel")?;
            let dir = r.get_int()?;
            Ok(Chan1Client::Rotate(Rotate { sel, dir }))
        }
        N_REPLACE => {
            let sel: SelInfo = DecodeCore::decode(r).in_field("sel")?;
            let old_tex = r.get_int()?;
            let new_tex = r.get_int()?;
            let in_sel = r.get_int()?;

            let len_lower = r.get_byte()? as u16;
            let len_upper = r.get_byte()? as u16;
            let extra_len = len_lower | (len_upper << 8);
            let extra_data = r.get_bytes(extra_len as usize)?.to_vec();

            Ok(Chan1Client::Replace(Replace {
                sel,
                old_tex,
                new_tex,
                in_sel,
                extra_data,
            }))
        }
        N_DELCUBE => {
            let sel: SelInfo = DecodeCore::decode(r).in_field("sel")?;
            Ok(Chan1Client::DelCube(DelCube { sel }))
        }
        N_EDITVSLOT => {
            let sel: SelInfo = DecodeCore::decode(r).in_field("sel")?;
            let delta = r.get_int()?;
            let all_faces = r.get_int()? != 0;

            let len_lower = r.get_byte()? as u16;
            let len_upper = r.get_byte()? as u16;
            let extra_len = len_lower | (len_upper << 8);
            let extra_data = r.get_bytes(extra_len as usize)?.to_vec();

            Ok(Chan1Client::EditVSlot(EditVSlot {
                sel,
                delta,
                all_faces,
                extra_data,
            }))
        }
        N_REMIP => Ok(Chan1Client::Remip(Remip)),
        N_SERVCMD => {
            let cmd = r.get_string()?;
            Ok(Chan1Client::ServCmd(ServCmd { cmd }))
        }
        N_TRYDROPFLAG => Ok(Chan1Client::TryDropFlag(TryDropFlag)),
        N_TAKEFLAG => Ok(Chan1Client::TakeFlag(TakeFlagClient {
            flag: r.get_int()?,
            version: r.get_int()?,
        })),
        N_INITFLAGS => {
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
                let team = r.get_int()?;
                let pos = Vector3 {
                    x: r.get_int()? as f32 / DMF,
                    y: r.get_int()? as f32 / DMF,
                    z: r.get_int()? as f32 / DMF,
                };

                flags.push(FlagSpawn { team, pos });
            }

            Ok(Chan1Client::InitFlags(InitFlagsClient { flags }))
        }
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
                bases.push(BaseSpawn {
                    ammotype,
                    pos: Vector3 {
                        x: r.get_int()? as f32 / DMF,
                        y: r.get_int()? as f32 / DMF,
                        z: r.get_int()? as f32 / DMF,
                    },
                });
            }
            Ok(Chan1Client::Bases(BasesClient { bases }))
        }
        N_REPAMMO => Ok(Chan1Client::RepAmmo(RepAmmoClient)),
        N_INITTOKENS => {
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
                bases.push(TokenBaseSpawn {
                    team: r.get_int()?,
                    pos: Vector3 {
                        x: r.get_int()? as f32 / DMF,
                        y: r.get_int()? as f32 / DMF,
                        z: r.get_int()? as f32 / DMF,
                    },
                });
            }
            Ok(Chan1Client::InitTokens(InitTokensClient { bases }))
        }
        N_TAKETOKEN => Ok(Chan1Client::TakeToken(TakeTokenClient { id: r.get_int()? })),
        N_DEPOSITTOKENS => Ok(Chan1Client::DepositTokens(DepositTokensClient {
            base: r.get_int()?,
        })),
        N_SOUND => Ok(Chan1Client::Sound(Sound {
            sound_id: r.get_int()?,
        })),
        _ => Err(ParseError::UnknownMessageType {
            channel: 1,
            id: msg_type,
        }),
    }
}
