use crate::{
    Chan1Client,
    data::EditVarValue,
    internal::{Encode, WriterCore, constants::*},
};

impl Encode for Chan1Client {
    fn encode(&self, w: &mut WriterCore) {
        match self {
            Chan1Client::Connect(v) => {
                w.put_int(N_CONNECT);
                w.put_string(&v.name);
                w.put_int(v.player_model as i32);
                w.put_string(&v.password_hash);
                w.put_string(&v.auth_desc);
                w.put_string(&v.auth_name);
            }
            Chan1Client::AuthAnswer(v) => {
                w.put_int(N_AUTHANS);
                w.put_string(&v.desc);
                w.put_int(v.id as i32);
                w.put_string(&v.answer);
            }
            Chan1Client::Ping(v) => {
                w.put_int(N_PING);
                w.put_int(v.time);
            }
            Chan1Client::FromAi(v) => {
                w.put_int(N_FROMAI);
                w.put_int(v.qcn);
            }
            Chan1Client::EditMode(v) => {
                w.put_int(N_EDITMODE);
                w.put_bool(v.val);
            }
            Chan1Client::MapCrc(v) => {
                w.put_int(N_MAPCRC);
                w.put_string(&v.map_name);
                w.put_int(v.crc);
            }
            Chan1Client::CheckMaps(_) => {
                w.put_int(N_CHECKMAPS);
            }
            Chan1Client::TrySpawn(_) => {
                w.put_int(N_TRYSPAWN);
            }
            Chan1Client::GunSelect(v) => {
                w.put_int(N_GUNSELECT);
                w.put_int(v.gun as i32);
            }
            Chan1Client::Spawn(v) => {
                w.put_int(N_SPAWN);
                w.put_int(v.life_sequence);
                w.put_int(v.gun_select as i32);
            }
            Chan1Client::Suicide(_) => {
                w.put_int(N_SUICIDE);
            }
            Chan1Client::Shoot(v) => {
                w.put_int(N_SHOOT);
                w.put_int(v.id);
                w.put_int(v.gun as i32);

                // Pack coordinates (x, y, z) scaled by DMF
                w.put_int((&v.from.x * DMF) as i32);
                w.put_int((&v.from.y * DMF) as i32);
                w.put_int((&v.from.z * DMF) as i32);
                w.put_int((&v.to.x * DMF) as i32);
                w.put_int((&v.to.y * DMF) as i32);
                w.put_int((&v.to.z * DMF) as i32);

                w.put_int(v.hits.len() as i32);
                for hit in &v.hits {
                    hit.encode(w);
                }
            }
            Chan1Client::Explode(v) => {
                w.put_int(N_EXPLODE);
                w.put_int(v.millis);
                w.put_int(v.gun as i32);
                w.put_int(v.id);

                w.put_int(v.hits.len() as i32);
                for hit in &v.hits {
                    hit.encode(w);
                }
            }
            Chan1Client::ItemPickup(v) => {
                w.put_int(N_ITEMPICKUP);
                w.put_int(v.item_index);
            }
            Chan1Client::Text(v) => {
                w.put_int(N_TEXT);
                w.put_string(&v.text);
            }
            Chan1Client::SayTeam(v) => {
                w.put_int(N_SAYTEAM);
                w.put_string(&v.text);
            }
            Chan1Client::SwitchName(v) => {
                w.put_int(N_SWITCHNAME);
                w.put_string(&v.name);
            }
            Chan1Client::SwitchModel(v) => {
                w.put_int(N_SWITCHMODEL);
                w.put_int(v.model as i32);
            }
            Chan1Client::SwitchTeam(v) => {
                w.put_int(N_SWITCHTEAM);
                w.put_string(&v.team);
            }
            Chan1Client::MapVote(v) => {
                w.put_int(N_MAPVOTE);
                w.put_string(&v.map_name);
                w.put_int(v.mode as i32);
            }
            Chan1Client::ItemList(v) => {
                w.put_int(N_ITEMLIST);
                for item in &v.items {
                    w.put_int(item.item_index);
                    w.put_int(item.item_type as i32);
                }
                w.put_int(-1); // Sent as a terminator
            }
            Chan1Client::EditEnt(v) => {
                w.put_int(N_EDITENT);
                w.put_int(v.ent_id);
                w.put_int(v.x);
                w.put_int(v.y);
                w.put_int(v.z);
                w.put_int(v.type_ as i32);
                w.put_int(v.attr1);
                w.put_int(v.attr2);
                w.put_int(v.attr3);
                w.put_int(v.attr4);
                w.put_int(v.attr5);
            }
            Chan1Client::EditVar(v) => {
                w.put_int(N_EDITVAR);
                match &v.val {
                    EditVarValue::Int(i) => {
                        w.put_int(0);
                        w.put_string(&v.name);
                        w.put_int(*i);
                    }
                    EditVarValue::Float(f) => {
                        w.put_int(1);
                        w.put_string(&v.name);
                        w.put_float(*f);
                    }
                    EditVarValue::String(s) => {
                        w.put_int(2);
                        w.put_string(&v.name);
                        w.put_string(s);
                    }
                }
            }
            Chan1Client::ClientPing(v) => {
                w.put_int(N_CLIENTPING);
                w.put_int(v.ping);
            }
            Chan1Client::MasterMode(v) => {
                w.put_int(N_MASTERMODE);
                w.put_int(v.mode as i32);
            }
            Chan1Client::ClearBans(_) => {
                w.put_int(N_CLEARBANS);
            }
            Chan1Client::Kick(v) => {
                w.put_int(N_KICK);
                w.put_int(v.victim_cn);
                w.put_string(&v.reason);
            }
            Chan1Client::Spectator(v) => {
                w.put_int(N_SPECTATOR);
                w.put_int(v.spectator_cn);
                w.put_bool(v.val);
            }
            Chan1Client::SetTeam(v) => {
                w.put_int(N_SETTEAM);
                w.put_int(v.client_num);
                w.put_string(&v.team);
            }
            Chan1Client::ForceIntermission(_) => {
                w.put_int(N_FORCEINTERMISSION);
            }
            Chan1Client::RecordDemo(v) => {
                w.put_int(N_RECORDDEMO);
                w.put_bool(v.val);
            }
            Chan1Client::StopDemo(_) => {
                w.put_int(N_STOPDEMO);
            }
            Chan1Client::ClearDemos(v) => {
                w.put_int(N_CLEARDEMOS);
                w.put_int(v.demo_id);
            }
            Chan1Client::ListDemos(_) => {
                w.put_int(N_LISTDEMOS);
            }
            Chan1Client::GetDemo(v) => {
                w.put_int(N_GETDEMO);
                w.put_int(v.demo_id);
                w.put_int(v.tag);
            }
            Chan1Client::GetMap(_) => {
                w.put_int(N_GETMAP);
            }
            Chan1Client::NewMap(v) => {
                w.put_int(N_NEWMAP);
                w.put_int(v.size);
            }
            Chan1Client::SetMaster(v) => {
                w.put_int(N_SETMASTER);
                w.put_int(v.client_num);
                w.put_bool(v.val);
                w.put_string(&v.hash);
            }
            Chan1Client::AddBot(v) => {
                w.put_int(N_ADDBOT);
                w.put_int(v.skill);
            }
            Chan1Client::DelBot(_) => {
                w.put_int(N_DELBOT);
            }
            Chan1Client::BotLimit(v) => {
                w.put_int(N_BOTLIMIT);
                w.put_int(v.limit);
            }
            Chan1Client::BotBalance(v) => {
                w.put_int(N_BOTBALANCE);
                w.put_bool(v.balance);
            }
            Chan1Client::AuthTry(v) => {
                w.put_int(N_AUTHTRY);
                w.put_string(&v.desc);
                w.put_string(&v.name);
            }
            Chan1Client::AuthKick(v) => {
                w.put_int(N_AUTHKICK);
                w.put_string(&v.desc);
                w.put_string(&v.name);
                w.put_int(v.victim_cn);
                w.put_string(&v.reason);
            }
            Chan1Client::PauseGame(v) => {
                w.put_int(N_PAUSEGAME);
                w.put_bool(v.val);
            }
            Chan1Client::GameSpeed(v) => {
                w.put_int(N_GAMESPEED);
                w.put_int(v.speed);
            }
            Chan1Client::Copy(_) => {
                w.put_int(N_COPY);
            }
            Chan1Client::Paste(_) => {
                w.put_int(N_PASTE);
            }
            Chan1Client::Clipboard(v) => {
                w.put_int(N_CLIPBOARD);
                w.put_int(v.unpack_len);
                w.put_int(v.pack_len);
                w.put_bytes(&v.data);
            }
            Chan1Client::Undo(v) => {
                w.put_int(N_UNDO);
                w.put_int(v.unpack_len);
                w.put_int(v.pack_len);
                w.put_bytes(&v.data);
            }
            Chan1Client::Redo(v) => {
                w.put_int(N_REDO);
                w.put_int(v.unpack_len);
                w.put_int(v.pack_len);
                w.put_bytes(&v.data);
            }
            Chan1Client::EditFace(v) => {
                w.put_int(N_EDITF);
                v.sel.encode(w);
                w.put_int(v.dir);
                w.put_int(v.mode as i32);
            }
            Chan1Client::EditTex(v) => {
                w.put_int(N_EDITT);
                v.sel.encode(w);
                w.put_int(v.tex);
                w.put_bool(v.all_faces);

                let extra_len = v.extra_data.len() as u16;
                w.put_byte((extra_len & 0xFF) as u8);
                w.put_byte((extra_len >> 8) as u8);
                w.put_bytes(&v.extra_data);
            }
            Chan1Client::EditMat(v) => {
                w.put_int(N_EDITM);
                v.sel.encode(w);
                w.put_int(v.mat);
                w.put_int(v.filter);
            }
            Chan1Client::Flip(v) => {
                w.put_int(N_FLIP);
                v.sel.encode(w);
            }
            Chan1Client::Rotate(v) => {
                w.put_int(N_ROTATE);
                v.sel.encode(w);
                w.put_int(v.dir);
            }
            Chan1Client::Replace(v) => {
                w.put_int(N_REPLACE);
                v.sel.encode(w);
                w.put_int(v.old_tex);
                w.put_int(v.new_tex);
                w.put_int(v.in_sel);

                let extra_len = v.extra_data.len() as u16;
                w.put_byte((extra_len & 0xFF) as u8);
                w.put_byte((extra_len >> 8) as u8);
                w.put_bytes(&v.extra_data);
            }
            Chan1Client::DelCube(v) => {
                w.put_int(N_DELCUBE);
                v.sel.encode(w);
            }
            Chan1Client::EditVSlot(v) => {
                w.put_int(N_EDITVSLOT);
                v.sel.encode(w);
                w.put_int(v.delta);
                w.put_bool(v.all_faces);

                let extra_len = v.extra_data.len() as u16;
                w.put_byte((extra_len & 0xFF) as u8);
                w.put_byte((extra_len >> 8) as u8);
                w.put_bytes(&v.extra_data);
            }
            Chan1Client::Remip(_) => {
                w.put_int(N_REMIP);
            }
            Chan1Client::ServCmd(v) => {
                w.put_int(N_SERVCMD);
                w.put_string(&v.cmd);
            }
            Chan1Client::TryDropFlag(_) => {
                w.put_int(N_TRYDROPFLAG);
            }
            Chan1Client::TakeFlag(v) => {
                w.put_int(N_TAKEFLAG);
                w.put_int(v.flag);
                w.put_int(v.version);
            }
            Chan1Client::InitFlags(v) => {
                w.put_int(N_INITFLAGS);

                w.put_int(v.flags.len() as i32);
                for f in &v.flags {
                    w.put_int(f.team);

                    w.put_int((f.pos.x * DMF) as i32);
                    w.put_int((f.pos.y * DMF) as i32);
                    w.put_int((f.pos.z * DMF) as i32);
                }
            }
            Chan1Client::Bases(v) => {
                w.put_int(N_BASES);
                w.put_int(v.bases.len() as i32);
                for b in &v.bases {
                    w.put_int(b.ammotype.map_or(0, |t| t as i32));
                    w.put_int((b.pos.x * DMF) as i32);
                    w.put_int((b.pos.y * DMF) as i32);
                    w.put_int((b.pos.z * DMF) as i32);
                }
            }
            Chan1Client::RepAmmo(_) => {
                w.put_int(N_REPAMMO);
            }
            Chan1Client::InitTokens(v) => {
                w.put_int(N_INITTOKENS);
                w.put_int(v.bases.len() as i32);
                for b in &v.bases {
                    w.put_int(b.team);
                    w.put_int((b.pos.x * DMF) as i32);
                    w.put_int((b.pos.y * DMF) as i32);
                    w.put_int((b.pos.z * DMF) as i32);
                }
            }
            Chan1Client::TakeToken(v) => {
                w.put_int(N_TAKETOKEN);
                w.put_int(v.id);
            }
            Chan1Client::DepositTokens(v) => {
                w.put_int(N_DEPOSITTOKENS);
                w.put_int(v.base);
            }
            Chan1Client::Sound(v) => {
                w.put_int(N_SOUND);
                w.put_int(v.sound_id);
            }
        }
    }
}
