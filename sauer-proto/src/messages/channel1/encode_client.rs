use crate::{
    Chan1Client,
    data::EditVarValue,
    internal::{Encode, WriterCore, constants::*},
};

impl Encode for Chan1Client {
    fn encode(&self, w: &mut WriterCore) {
        match self {
            Chan1Client::Connect {
                name,
                player_model,
                password_hash,
                auth_desc,
                auth_name,
            } => {
                w.put_int(N_CONNECT);
                w.put_string(name);
                w.put_int(*player_model);
                w.put_string(password_hash);
                w.put_string(auth_desc);
                w.put_string(auth_name);
            }
            Chan1Client::AuthAnswer { desc, id, answer } => {
                w.put_int(N_AUTHANS);
                w.put_string(desc);
                w.put_int(*id as i32);
                w.put_string(answer);
            }
            Chan1Client::Ping { time } => {
                w.put_int(N_PING);
                w.put_int(*time);
            }
            Chan1Client::FromAi { qcn } => {
                w.put_int(N_FROMAI);
                w.put_int(*qcn);
            }
            Chan1Client::EditMode { val } => {
                w.put_int(N_EDITMODE);
                w.put_int(*val);
            }
            Chan1Client::MapCrc { map_name, crc } => {
                w.put_int(N_MAPCRC);
                w.put_string(map_name);
                w.put_int(*crc);
            }
            Chan1Client::CheckMaps => {
                w.put_int(N_CHECKMAPS);
            }
            Chan1Client::TrySpawn => {
                w.put_int(N_TRYSPAWN);
            }
            Chan1Client::GunSelect { gun } => {
                w.put_int(N_GUNSELECT);
                w.put_int(*gun as i32);
            }
            Chan1Client::Spawn {
                life_sequence,
                gun_select,
            } => {
                w.put_int(N_SPAWN);
                w.put_int(*life_sequence);
                w.put_int(*gun_select as i32);
            }
            Chan1Client::Suicide => {
                w.put_int(N_SUICIDE);
            }
            Chan1Client::Shoot {
                id,
                gun,
                from,
                to,
                hits,
            } => {
                w.put_int(N_SHOOT);
                w.put_int(*id);
                w.put_int(*gun as i32);

                // Pack coordinates (x, y, z) scaled by DMF
                w.put_int((from.x * DMF) as i32);
                w.put_int((from.y * DMF) as i32);
                w.put_int((from.z * DMF) as i32);
                w.put_int((to.x * DMF) as i32);
                w.put_int((to.y * DMF) as i32);
                w.put_int((to.z * DMF) as i32);

                w.put_int(hits.len() as i32);
                for hit in hits {
                    hit.encode(w);
                }
            }
            Chan1Client::Explode {
                millis,
                gun,
                id,
                hits,
            } => {
                w.put_int(N_EXPLODE);
                w.put_int(*millis);
                w.put_int(*gun as i32);
                w.put_int(*id);

                w.put_int(hits.len() as i32);
                for hit in hits {
                    hit.encode(w);
                }
            }
            Chan1Client::ItemPickup { item_index } => {
                w.put_int(N_ITEMPICKUP);
                w.put_int(*item_index);
            }
            Chan1Client::Text { text } => {
                w.put_int(N_TEXT);
                w.put_string(text);
            }
            Chan1Client::SayTeam { text } => {
                w.put_int(N_SAYTEAM);
                w.put_string(text);
            }
            Chan1Client::SwitchName { name } => {
                w.put_int(N_SWITCHNAME);
                w.put_string(name);
            }
            Chan1Client::SwitchModel { model } => {
                w.put_int(N_SWITCHMODEL);
                w.put_int(*model);
            }
            Chan1Client::SwitchTeam { team } => {
                w.put_int(N_SWITCHTEAM);
                w.put_string(team);
            }
            Chan1Client::MapVote { map_name, mode } => {
                w.put_int(N_MAPVOTE);
                w.put_string(map_name);
                w.put_int(*mode);
            }
            Chan1Client::ItemList { items } => {
                w.put_int(N_ITEMLIST);
                for item in items {
                    w.put_int(item.item_index);
                    w.put_int(item.item_type as i32);
                }
                w.put_int(-1); // Sent as a terminator
            }
            Chan1Client::EditEnt {
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
            } => {
                w.put_int(N_EDITENT);
                w.put_int(*ent_id);
                w.put_int(*x);
                w.put_int(*y);
                w.put_int(*z);
                w.put_int(*type_);
                w.put_int(*attr1);
                w.put_int(*attr2);
                w.put_int(*attr3);
                w.put_int(*attr4);
                w.put_int(*attr5);
            }
            Chan1Client::EditVar {
                var_type,
                name,
                val,
            } => {
                w.put_int(N_EDITVAR);
                w.put_int(*var_type);
                w.put_string(name);
                match val {
                    EditVarValue::Int(i) => w.put_int(*i),
                    EditVarValue::Float(f) => w.put_float(*f),
                    EditVarValue::String(s) => w.put_string(s),
                }
            }
            Chan1Client::ClientPing { ping } => {
                w.put_int(N_CLIENTPING);
                w.put_int(*ping);
            }
            Chan1Client::MasterMode { mode } => {
                w.put_int(N_MASTERMODE);
                w.put_int(*mode as i32);
            }
            Chan1Client::ClearBans => {
                w.put_int(N_CLEARBANS);
            }
            Chan1Client::Kick { victim_cn, reason } => {
                w.put_int(N_KICK);
                w.put_int(*victim_cn);
                w.put_string(reason);
            }
            Chan1Client::Spectator { spectator_cn, val } => {
                w.put_int(N_SPECTATOR);
                w.put_int(*spectator_cn);
                w.put_int(*val);
            }
            Chan1Client::SetTeam { client_num, team } => {
                w.put_int(N_SETTEAM);
                w.put_int(*client_num);
                w.put_string(team);
            }
            Chan1Client::ForceIntermission => {
                w.put_int(N_FORCEINTERMISSION);
            }
            Chan1Client::RecordDemo { val } => {
                w.put_int(N_RECORDDEMO);
                w.put_int(*val);
            }
            Chan1Client::StopDemo => {
                w.put_int(N_STOPDEMO);
            }
            Chan1Client::ClearDemos { demo_id } => {
                w.put_int(N_CLEARDEMOS);
                w.put_int(*demo_id);
            }
            Chan1Client::ListDemos => {
                w.put_int(N_LISTDEMOS);
            }
            Chan1Client::GetDemo { demo_id, tag } => {
                w.put_int(N_GETDEMO);
                w.put_int(*demo_id);
                w.put_int(*tag);
            }
            Chan1Client::GetMap => {
                w.put_int(N_GETMAP);
            }
            Chan1Client::NewMap { size } => {
                w.put_int(N_NEWMAP);
                w.put_int(*size);
            }
            Chan1Client::SetMaster {
                client_num,
                val,
                hash,
            } => {
                w.put_int(N_SETMASTER);
                w.put_int(*client_num);
                w.put_int(*val);
                w.put_string(hash);
            }
            Chan1Client::AddBot { skill } => {
                w.put_int(N_ADDBOT);
                w.put_int(*skill);
            }
            Chan1Client::DelBot => {
                w.put_int(N_DELBOT);
            }
            Chan1Client::BotLimit { limit } => {
                w.put_int(N_BOTLIMIT);
                w.put_int(*limit);
            }
            Chan1Client::BotBalance { balance } => {
                w.put_int(N_BOTBALANCE);
                w.put_int(*balance);
            }
            Chan1Client::AuthTry { desc, name } => {
                w.put_int(N_AUTHTRY);
                w.put_string(desc);
                w.put_string(name);
            }
            Chan1Client::AuthKick {
                desc,
                name,
                victim_cn,
                reason,
            } => {
                w.put_int(N_AUTHKICK);
                w.put_string(desc);
                w.put_string(name);
                w.put_int(*victim_cn);
                w.put_string(reason);
            }
            Chan1Client::PauseGame { val } => {
                w.put_int(N_PAUSEGAME);
                w.put_int(*val);
            }
            Chan1Client::GameSpeed { speed } => {
                w.put_int(N_GAMESPEED);
                w.put_int(*speed);
            }
            Chan1Client::Copy => {
                w.put_int(N_COPY);
            }
            Chan1Client::Paste => {
                w.put_int(N_PASTE);
            }
            Chan1Client::Clipboard {
                unpack_len,
                pack_len,
                data,
            } => {
                w.put_int(N_CLIPBOARD);
                w.put_int(*unpack_len);
                w.put_int(*pack_len);
                w.put_bytes(data);
            }
            Chan1Client::Undo {
                unpack_len,
                pack_len,
                data,
            } => {
                w.put_int(N_UNDO);
                w.put_int(*unpack_len);
                w.put_int(*pack_len);
                w.put_bytes(data);
            }
            Chan1Client::Redo {
                unpack_len,
                pack_len,
                data,
            } => {
                w.put_int(N_REDO);
                w.put_int(*unpack_len);
                w.put_int(*pack_len);
                w.put_bytes(data);
            }
            Chan1Client::EditFace { sel, dir, mode } => {
                w.put_int(N_EDITF);
                sel.encode(w);
                w.put_int(*dir);
                w.put_int(*mode);
            }
            Chan1Client::EditTex {
                sel,
                tex,
                all_faces,
                extra_data,
            } => {
                w.put_int(N_EDITT);
                sel.encode(w);
                w.put_int(*tex);
                w.put_int(*all_faces);

                let extra_len = extra_data.len() as u16;
                w.put_byte((extra_len & 0xFF) as u8);
                w.put_byte((extra_len >> 8) as u8);
                w.put_bytes(extra_data);
            }
            Chan1Client::EditMat { sel, mat, filter } => {
                w.put_int(N_EDITM);
                sel.encode(w);
                w.put_int(*mat);
                w.put_int(*filter);
            }
            Chan1Client::Flip { sel } => {
                w.put_int(N_FLIP);
                sel.encode(w);
            }
            Chan1Client::Rotate { sel, dir } => {
                w.put_int(N_ROTATE);
                sel.encode(w);
                w.put_int(*dir);
            }
            Chan1Client::Replace {
                sel,
                old_tex,
                new_tex,
                in_sel,
                extra_data,
            } => {
                w.put_int(N_REPLACE);
                sel.encode(w);
                w.put_int(*old_tex);
                w.put_int(*new_tex);
                w.put_int(*in_sel);

                let extra_len = extra_data.len() as u16;
                w.put_byte((extra_len & 0xFF) as u8);
                w.put_byte((extra_len >> 8) as u8);
                w.put_bytes(extra_data);
            }
            Chan1Client::DelCube { sel } => {
                w.put_int(N_DELCUBE);
                sel.encode(w);
            }
            Chan1Client::EditVSlot {
                sel,
                delta,
                all_faces,
                extra_data,
            } => {
                w.put_int(N_EDITVSLOT);
                sel.encode(w);
                w.put_int(*delta);
                w.put_int(*all_faces);

                let extra_len = extra_data.len() as u16;
                w.put_byte((extra_len & 0xFF) as u8);
                w.put_byte((extra_len >> 8) as u8);
                w.put_bytes(extra_data);
            }
            Chan1Client::Remip => {
                w.put_int(N_REMIP);
            }
            Chan1Client::ServCmd { cmd } => {
                w.put_int(N_SERVCMD);
                w.put_string(cmd);
            }
            Chan1Client::TryDropFlag => {
                w.put_int(N_TRYDROPFLAG);
            }
            Chan1Client::TakeFlag { flag, version } => {
                w.put_int(N_TAKEFLAG);
                w.put_int(*flag);
                w.put_int(*version);
            }
            Chan1Client::InitFlags { flags } => {
                w.put_int(N_INITFLAGS);

                w.put_int(flags.len() as i32);
                for f in flags {
                    w.put_int(f.team);

                    w.put_int((f.pos.x * DMF) as i32);
                    w.put_int((f.pos.y * DMF) as i32);
                    w.put_int((f.pos.z * DMF) as i32);
                }
            }
            Chan1Client::Bases { bases } => {
                w.put_int(N_BASES);
                w.put_int(bases.len() as i32);
                for b in bases {
                    w.put_int(b.ammotype);
                    w.put_int((b.pos.x * DMF) as i32);
                    w.put_int((b.pos.y * DMF) as i32);
                    w.put_int((b.pos.z * DMF) as i32);
                }
            }
            Chan1Client::RepAmmo => {
                w.put_int(N_REPAMMO);
            }
            Chan1Client::InitTokens { bases } => {
                w.put_int(N_INITTOKENS);
                w.put_int(bases.len() as i32);
                for b in bases {
                    w.put_int(b.team);
                    w.put_int((b.pos.x * DMF) as i32);
                    w.put_int((b.pos.y * DMF) as i32);
                    w.put_int((b.pos.z * DMF) as i32);
                }
            }
            Chan1Client::TakeToken { id } => {
                w.put_int(N_TAKETOKEN);
                w.put_int(*id);
            }
            Chan1Client::DepositTokens { base } => {
                w.put_int(N_DEPOSITTOKENS);
                w.put_int(*base);
            }
            Chan1Client::Sound { sound_id } => {
                w.put_int(N_SOUND);
                w.put_int(*sound_id);
            }
        }
    }
}
