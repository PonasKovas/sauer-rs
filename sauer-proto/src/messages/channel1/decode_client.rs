use crate::{
    Chan1Client, ParseError,
    data::{
        BaseSpawn, EditVarValue, FlagSpawn, Gun, HitInfo, ItemSpawn, ItemType, MasterMode, SelInfo,
        TokenBaseSpawn, Vector3,
    },
    internal::{Decode, ReaderCore, constants::*},
};

impl Decode for Chan1Client {
    fn decode(msg_type: i32, r: &mut ReaderCore) -> Result<Self, ParseError> {
        match msg_type {
            N_CONNECT => {
                let name = r.get_string()?;
                let player_model = r.get_int()?;
                let password_hash = r.get_string()?;
                let auth_desc = r.get_string()?;
                let auth_name = r.get_string()?;
                Ok(Chan1Client::Connect {
                    name,
                    player_model,
                    password_hash,
                    auth_desc,
                    auth_name,
                })
            }
            N_AUTHANS => {
                let desc = r.get_string()?;
                let id = r.get_int()? as u32;
                let answer = r.get_string()?;
                Ok(Chan1Client::AuthAnswer { desc, id, answer })
            }
            N_PING => {
                let time = r.get_int()?;
                Ok(Chan1Client::Ping { time })
            }
            N_FROMAI => {
                let qcn = r.get_int()?;
                Ok(Chan1Client::FromAi { qcn })
            }
            N_EDITMODE => {
                let val = r.get_int()?;
                Ok(Chan1Client::EditMode { val })
            }
            N_MAPCRC => {
                let map_name = r.get_string()?;
                let crc = r.get_int()?;
                Ok(Chan1Client::MapCrc { map_name, crc })
            }
            N_CHECKMAPS => Ok(Chan1Client::CheckMaps),
            N_TRYSPAWN => Ok(Chan1Client::TrySpawn),
            N_GUNSELECT => {
                let gun_val = r.get_int()?;
                let gun = Gun::try_from(gun_val).map_err(|_| ParseError::InvalidEnumValue {
                    value: gun_val,
                    enum_name: "Gun",
                })?;
                Ok(Chan1Client::GunSelect { gun })
            }
            N_SPAWN => {
                let life_sequence = r.get_int()?;
                let gun_val = r.get_int()?;
                let gun_select =
                    Gun::try_from(gun_val).map_err(|_| ParseError::InvalidEnumValue {
                        value: gun_val,
                        enum_name: "Gun",
                    })?;
                Ok(Chan1Client::Spawn {
                    life_sequence,
                    gun_select,
                })
            }
            N_SUICIDE => Ok(Chan1Client::Suicide),
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
                    hits.push(HitInfo::decode(r)?);
                }

                Ok(Chan1Client::Shoot {
                    id,
                    gun,
                    from,
                    to,
                    hits,
                })
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
                    hits.push(HitInfo::decode(r)?);
                }

                Ok(Chan1Client::Explode {
                    millis,
                    gun,
                    id,
                    hits,
                })
            }
            N_ITEMPICKUP => {
                let item_index = r.get_int()?;
                Ok(Chan1Client::ItemPickup { item_index })
            }
            N_TEXT => {
                let text = r.get_string()?;
                Ok(Chan1Client::Text { text })
            }
            N_SAYTEAM => {
                let text = r.get_string()?;
                Ok(Chan1Client::SayTeam { text })
            }
            N_SWITCHNAME => {
                let name = r.get_string()?;
                Ok(Chan1Client::SwitchName { name })
            }
            N_SWITCHMODEL => {
                let model = r.get_int()?;
                Ok(Chan1Client::SwitchModel { model })
            }
            N_SWITCHTEAM => {
                let team = r.get_string()?;
                Ok(Chan1Client::SwitchTeam { team })
            }
            N_MAPVOTE => {
                let map_name = r.get_string()?;
                let mode = r.get_int()?;
                Ok(Chan1Client::MapVote { map_name, mode })
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
                Ok(Chan1Client::ItemList { items })
            }
            N_EDITENT => {
                let ent_id = r.get_int()?;
                let x = r.get_int()?;
                let y = r.get_int()?;
                let z = r.get_int()?;
                let type_ = r.get_int()?;
                let attr1 = r.get_int()?;
                let attr2 = r.get_int()?;
                let attr3 = r.get_int()?;
                let attr4 = r.get_int()?;
                let attr5 = r.get_int()?;
                Ok(Chan1Client::EditEnt {
                    ent_id,
                    x,
                    y,
                    z,
                    type_,
                    attr1,
                    attr2,
                    attr3,
                    attr4,
                    attr5,
                })
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
                Ok(Chan1Client::EditVar {
                    var_type,
                    name,
                    val,
                })
            }
            N_CLIENTPING => {
                let ping = r.get_int()?;
                Ok(Chan1Client::ClientPing { ping })
            }
            N_MASTERMODE => {
                let mode_val = r.get_int()?;
                let mode =
                    MasterMode::try_from(mode_val).map_err(|_| ParseError::InvalidEnumValue {
                        value: mode_val,
                        enum_name: "MasterMode",
                    })?;
                Ok(Chan1Client::MasterMode { mode })
            }
            N_CLEARBANS => Ok(Chan1Client::ClearBans),
            N_KICK => {
                let victim_cn = r.get_int()?;
                let reason = r.get_string()?;
                Ok(Chan1Client::Kick { victim_cn, reason })
            }
            N_SPECTATOR => {
                let spectator_cn = r.get_int()?;
                let val = r.get_int()?;
                Ok(Chan1Client::Spectator { spectator_cn, val })
            }
            N_SETTEAM => {
                let client_num = r.get_int()?;
                let team = r.get_string()?;
                Ok(Chan1Client::SetTeam { client_num, team })
            }
            N_FORCEINTERMISSION => Ok(Chan1Client::ForceIntermission),
            N_RECORDDEMO => {
                let val = r.get_int()?;
                Ok(Chan1Client::RecordDemo { val })
            }
            N_STOPDEMO => Ok(Chan1Client::StopDemo),
            N_CLEARDEMOS => {
                let demo_id = r.get_int()?;
                Ok(Chan1Client::ClearDemos { demo_id })
            }
            N_LISTDEMOS => Ok(Chan1Client::ListDemos),
            N_GETDEMO => {
                let demo_id = r.get_int()?;
                let tag = r.get_int()?;
                Ok(Chan1Client::GetDemo { demo_id, tag })
            }
            N_GETMAP => Ok(Chan1Client::GetMap),
            N_NEWMAP => {
                let size = r.get_int()?;
                Ok(Chan1Client::NewMap { size })
            }
            N_SETMASTER => {
                let client_num = r.get_int()?;
                let val = r.get_int()?;
                let hash = r.get_string()?;
                Ok(Chan1Client::SetMaster {
                    client_num,
                    val,
                    hash,
                })
            }
            N_ADDBOT => {
                let skill = r.get_int()?;
                Ok(Chan1Client::AddBot { skill })
            }
            N_DELBOT => Ok(Chan1Client::DelBot),
            N_BOTLIMIT => {
                let limit = r.get_int()?;
                Ok(Chan1Client::BotLimit { limit })
            }
            N_BOTBALANCE => {
                let balance = r.get_int()?;
                Ok(Chan1Client::BotBalance { balance })
            }
            N_AUTHTRY => {
                let desc = r.get_string()?;
                let name = r.get_string()?;
                Ok(Chan1Client::AuthTry { desc, name })
            }
            N_AUTHKICK => {
                let desc = r.get_string()?;
                let name = r.get_string()?;
                let victim_cn = r.get_int()?;
                let reason = r.get_string()?;
                Ok(Chan1Client::AuthKick {
                    desc,
                    name,
                    victim_cn,
                    reason,
                })
            }
            N_PAUSEGAME => {
                let val = r.get_int()?;
                Ok(Chan1Client::PauseGame { val })
            }
            N_GAMESPEED => {
                let speed = r.get_int()?;
                Ok(Chan1Client::GameSpeed { speed })
            }
            N_COPY => Ok(Chan1Client::Copy),
            N_PASTE => Ok(Chan1Client::Paste),
            N_CLIPBOARD => {
                let unpack_len = r.get_int()?;
                let pack_len = r.get_int()?;
                let data = r.get_bytes(pack_len.max(0) as usize)?.to_vec();
                Ok(Chan1Client::Clipboard {
                    unpack_len,
                    pack_len,
                    data,
                })
            }
            N_UNDO => {
                let unpack_len = r.get_int()?;
                let pack_len = r.get_int()?;
                let data = r.get_bytes(pack_len.max(0) as usize)?.to_vec();
                Ok(Chan1Client::Undo {
                    unpack_len,
                    pack_len,
                    data,
                })
            }
            N_REDO => {
                let unpack_len = r.get_int()?;
                let pack_len = r.get_int()?;
                let data = r.get_bytes(pack_len.max(0) as usize)?.to_vec();
                Ok(Chan1Client::Redo {
                    unpack_len,
                    pack_len,
                    data,
                })
            }
            N_EDITF => {
                let sel = SelInfo::decode(r)?;
                let dir = r.get_int()?;
                let mode = r.get_int()?;
                Ok(Chan1Client::EditFace { sel, dir, mode })
            }
            N_EDITT => {
                let sel = SelInfo::decode(r)?;
                let tex = r.get_int()?;
                let all_faces = r.get_int()?;

                let len_lower = r.get_byte()? as u16;
                let len_upper = r.get_byte()? as u16;
                let extra_len = len_lower | (len_upper << 8);
                let extra_data = r.get_bytes(extra_len as usize)?.to_vec();

                Ok(Chan1Client::EditTex {
                    sel,
                    tex,
                    all_faces,
                    extra_data,
                })
            }
            N_EDITM => {
                let sel = SelInfo::decode(r)?;
                let mat = r.get_int()?;
                let filter = r.get_int()?;
                Ok(Chan1Client::EditMat { sel, mat, filter })
            }
            N_FLIP => {
                let sel = SelInfo::decode(r)?;
                Ok(Chan1Client::Flip { sel })
            }
            N_ROTATE => {
                let sel = SelInfo::decode(r)?;
                let dir = r.get_int()?;
                Ok(Chan1Client::Rotate { sel, dir })
            }
            N_REPLACE => {
                let sel = SelInfo::decode(r)?;
                let old_tex = r.get_int()?;
                let new_tex = r.get_int()?;
                let in_sel = r.get_int()?;

                let len_lower = r.get_byte()? as u16;
                let len_upper = r.get_byte()? as u16;
                let extra_len = len_lower | (len_upper << 8);
                let extra_data = r.get_bytes(extra_len as usize)?.to_vec();

                Ok(Chan1Client::Replace {
                    sel,
                    old_tex,
                    new_tex,
                    in_sel,
                    extra_data,
                })
            }
            N_DELCUBE => {
                let sel = SelInfo::decode(r)?;
                Ok(Chan1Client::DelCube { sel })
            }
            N_EDITVSLOT => {
                let sel = SelInfo::decode(r)?;
                let delta = r.get_int()?;
                let all_faces = r.get_int()?;

                let len_lower = r.get_byte()? as u16;
                let len_upper = r.get_byte()? as u16;
                let extra_len = len_lower | (len_upper << 8);
                let extra_data = r.get_bytes(extra_len as usize)?.to_vec();

                Ok(Chan1Client::EditVSlot {
                    sel,
                    delta,
                    all_faces,
                    extra_data,
                })
            }
            N_REMIP => Ok(Chan1Client::Remip),
            N_SERVCMD => {
                let cmd = r.get_string()?;
                Ok(Chan1Client::ServCmd { cmd })
            }
            N_TRYDROPFLAG => Ok(Chan1Client::TryDropFlag),
            N_TAKEFLAG => Ok(Chan1Client::TakeFlag {
                flag: r.get_int()?,
                version: r.get_int()?,
            }),
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

                Ok(Chan1Client::InitFlags { flags })
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
                    bases.push(BaseSpawn {
                        ammotype: r.get_int()?,
                        pos: Vector3 {
                            x: r.get_int()? as f32 / DMF,
                            y: r.get_int()? as f32 / DMF,
                            z: r.get_int()? as f32 / DMF,
                        },
                    });
                }
                Ok(Chan1Client::Bases { bases })
            }
            N_REPAMMO => Ok(Chan1Client::RepAmmo),
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
                Ok(Chan1Client::InitTokens { bases })
            }
            N_TAKETOKEN => Ok(Chan1Client::TakeToken { id: r.get_int()? }),
            N_DEPOSITTOKENS => Ok(Chan1Client::DepositTokens { base: r.get_int()? }),
            _ => Err(ParseError::UnknownMessageType {
                channel: 1,
                id: msg_type,
            }),
        }
    }
}
