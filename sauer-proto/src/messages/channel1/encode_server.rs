use super::*;
use crate::{
    Chan1Server,
    data::{EditVarValue, FlagStateHoldTeam, FlagStateOwner},
    internal::{Encode, WriterCore, constants::*},
};

impl Encode for Chan1Server {
    fn requires_client_wrapper(&self) -> Option<i32> {
        match self {
            Chan1Server::Client(Client { client_num, .. }) => Some(*client_num),
            _ => None,
        }
    }

    fn encode(&self, w: &mut WriterCore) {
        match self {
            Chan1Server::DemoPacket(_) => w.put_int(N_DEMOPACKET),

            Chan1Server::ServInfo(v) => {
                w.put_int(N_SERVINFO);
                w.put_int(v.client_num);
                w.put_int(v.protocol);
                w.put_int(v.session_id);
                w.put_bool(v.has_password);
                w.put_string(&v.server_desc);
                w.put_string(&v.server_auth);
            }

            Chan1Server::Welcome(_) => w.put_int(N_WELCOME),

            Chan1Server::PauseGame(v) => {
                w.put_int(N_PAUSEGAME);
                w.put_bool(v.is_paused);
                w.put_int(v.client_num);
            }

            Chan1Server::GameSpeed(v) => {
                w.put_int(N_GAMESPEED);
                w.put_int(v.speed);
                w.put_int(v.client_num);
            }

            Chan1Server::SayTeam(v) => {
                w.put_int(N_SAYTEAM);
                w.put_int(v.client_num);
                w.put_string(&v.text);
            }

            Chan1Server::MapChange(v) => {
                w.put_int(N_MAPCHANGE);
                w.put_string(&v.map_name);
                w.put_int(v.mode as i32);
                w.put_bool(v.has_items);
            }

            Chan1Server::ForceDeath(v) => {
                w.put_int(N_FORCEDEATH);
                w.put_int(v.client_num);
            }

            Chan1Server::ItemList(v) => {
                w.put_int(N_ITEMLIST);
                for item in &v.items {
                    w.put_int(item.item_index);
                    w.put_int(item.item_type as i32);
                }
                w.put_int(-1); // Terminator
            }

            Chan1Server::InitClient(v) => {
                w.put_int(N_INITCLIENT);
                w.put_int(v.client_num);
                w.put_string(&v.name);
                w.put_string(&v.team);
                w.put_int(v.player_model as i32);
            }

            Chan1Server::ClientDisconnected(v) => {
                w.put_int(N_CDIS);
                w.put_int(v.client_num);
            }

            Chan1Server::SpawnState(v) => {
                w.put_int(N_SPAWNSTATE);
                w.put_int(v.client_num);
                v.state.encode(w);
            }

            Chan1Server::ShotFx(v) => {
                w.put_int(N_SHOTFX);
                w.put_int(v.client_num);
                w.put_int(v.gun as i32);
                w.put_int(v.id);
                w.put_int((&v.from.x * DMF) as i32);
                w.put_int((&v.from.y * DMF) as i32);
                w.put_int((&v.from.z * DMF) as i32);
                w.put_int((&v.to.x * DMF) as i32);
                w.put_int((&v.to.y * DMF) as i32);
                w.put_int((&v.to.z * DMF) as i32);
            }

            Chan1Server::ExplodeFx(v) => {
                w.put_int(N_EXPLODEFX);
                w.put_int(v.client_num);
                w.put_int(v.gun as i32);
                w.put_int(v.id);
            }

            Chan1Server::Damage(v) => {
                w.put_int(N_DAMAGE);
                w.put_int(v.target_cn);
                w.put_int(v.actor_cn);
                w.put_int(v.damage);
                w.put_int(v.armour);
                w.put_int(v.health);
            }

            Chan1Server::HitPush(v) => {
                w.put_int(N_HITPUSH);
                w.put_int(v.target_cn);
                w.put_int(v.gun as i32);
                w.put_int(v.damage);
                w.put_int((&v.dir.x * DMF) as i32);
                w.put_int((&v.dir.y * DMF) as i32);
                w.put_int((&v.dir.z * DMF) as i32);
            }

            Chan1Server::Died(v) => {
                w.put_int(N_DIED);
                w.put_int(v.victim_cn);
                w.put_int(v.actor_cn);
                w.put_int(v.actor_frags);
                w.put_int(v.team_frags);
            }

            Chan1Server::TeamInfo(v) => {
                w.put_int(N_TEAMINFO);
                for team in &v.teams {
                    w.put_string(&team.team);
                    w.put_int(team.frags);
                }
                w.put_byte(0); // empty string terminator
            }

            Chan1Server::Resume(v) => {
                w.put_int(N_RESUME);
                for c in &v.clients {
                    w.put_int(c.client_num);
                    w.put_int(c.state as i32);
                    w.put_int(c.frags);
                    w.put_int(c.flags);
                    w.put_int(c.deaths);
                    w.put_int(c.quad_millis);
                    c.player_state.encode(w);
                }
                w.put_int(-1); // Terminator
            }

            Chan1Server::ItemSpawn(v) => {
                w.put_int(N_ITEMSPAWN);
                w.put_int(v.item_index);
            }

            Chan1Server::ItemAck(v) => {
                w.put_int(N_ITEMACC);
                w.put_int(v.item_index);
                w.put_int(v.client_num);
            }

            Chan1Server::Clipboard(v) => {
                w.put_int(N_CLIPBOARD);
                w.put_int(v.client_num);
                w.put_int(v.unpack_len);
                w.put_int(v.pack_len);
                w.put_bytes(&v.data);
            }

            Chan1Server::Undo(v) => {
                w.put_int(N_UNDO);
                w.put_int(v.client_num);
                w.put_int(v.unpack_len);
                w.put_int(v.pack_len);
                w.put_bytes(&v.data);
            }

            Chan1Server::Redo(v) => {
                w.put_int(N_REDO);
                w.put_int(v.client_num);
                w.put_int(v.unpack_len);
                w.put_int(v.pack_len);
                w.put_bytes(&v.data);
            }

            Chan1Server::Pong(v) => {
                w.put_int(N_PONG);
                w.put_int(v.ping);
            }

            Chan1Server::TimeUp(v) => {
                w.put_int(N_TIMEUP);
                w.put_int(v.seconds_remaining);
            }

            Chan1Server::ServMsg(v) => {
                w.put_int(N_SERVMSG);
                w.put_string(&v.text);
            }

            Chan1Server::SendDemoList(v) => {
                w.put_int(N_SENDDEMOLIST);
                w.put_int(v.demos.len() as i32);
                for demo in &v.demos {
                    w.put_string(demo);
                }
            }

            Chan1Server::DemoPlayback(v) => {
                w.put_int(N_DEMOPLAYBACK);
                w.put_bool(v.is_on);
                w.put_int(v.client_num);
            }

            Chan1Server::CurrentMaster(v) => {
                w.put_int(N_CURRENTMASTER);
                w.put_int(v.master_mode as i32);
                for c in &v.clients {
                    w.put_int(c.client_num);
                    w.put_int(c.privilege as i32);
                }
                w.put_int(-1);
            }

            Chan1Server::MasterMode(v) => {
                w.put_int(N_MASTERMODE);
                w.put_int(v.master_mode as i32);
            }

            Chan1Server::Spectator(v) => {
                w.put_int(N_SPECTATOR);
                w.put_int(v.client_num);
                w.put_bool(v.is_spectator);
            }

            Chan1Server::SetTeam(v) => {
                w.put_int(N_SETTEAM);
                w.put_int(v.client_num);
                w.put_string(&v.team);
                w.put_int(v.reason);
            }

            Chan1Server::Announce(v) => {
                w.put_int(N_ANNOUNCE);
                w.put_int(v.item_type as i32);
            }

            Chan1Server::NewMap(v) => {
                w.put_int(N_NEWMAP);
                w.put_int(v.size);
            }

            Chan1Server::ReqAuth(v) => {
                w.put_int(N_REQAUTH);
                w.put_string(&v.auth_domain);
            }

            Chan1Server::AuthChal(v) => {
                w.put_int(N_AUTHCHAL);
                w.put_string(&v.desc);
                w.put_int(v.id as i32);
                w.put_string(&v.challenge);
            }

            Chan1Server::InitAi(v) => {
                w.put_int(N_INITAI);
                w.put_int(v.bot_cn);
                w.put_int(v.owner_cn);
                w.put_int(v.ai_type as i32);
                w.put_int(v.skill);
                w.put_int(v.player_model as i32);
                w.put_string(&v.name);
                w.put_string(&v.team);
            }

            Chan1Server::ServCmd(v) => {
                w.put_int(N_SERVCMD);
                w.put_string(&v.cmd);
            }

            Chan1Server::InitFlags(v) => {
                w.put_int(N_INITFLAGS);
                w.put_int(v.scores[0]);
                w.put_int(v.scores[1]);

                w.put_int(v.flags.len() as i32);
                for f in &v.flags {
                    w.put_int(f.version);
                    w.put_int(f.spawn_index);
                    if let FlagStateOwner::Owned { owner } = &f.owner {
                        w.put_int(*owner);
                    } else {
                        w.put_int(-1);
                    }
                    w.put_int(if f.invis { 1 } else { 0 });

                    if let Some(hold) = &f.hold {
                        match hold {
                            FlagStateHoldTeam::None => {
                                w.put_int(-1);
                            }
                            FlagStateHoldTeam::Team { team, hold_time } => {
                                w.put_int(*team);
                                w.put_int(*hold_time);
                            }
                        }
                    }
                }
            }

            Chan1Server::DropFlag(v) => {
                w.put_int(N_DROPFLAG);
                w.put_int(v.client_num);
                w.put_int(v.flag);
                w.put_int(v.version);
                w.put_int((&v.drop_loc.x * DMF) as i32);
                w.put_int((&v.drop_loc.y * DMF) as i32);
                w.put_int((&v.drop_loc.z * DMF) as i32);
            }

            Chan1Server::ScoreFlag(v) => {
                w.put_int(N_SCOREFLAG);
                w.put_int(v.client_num);
                w.put_int(v.relay_flag);
                w.put_int(v.relay_version);
                w.put_int(v.goal_flag);
                w.put_int(v.goal_version);
                w.put_int(v.goal_spawn);
                w.put_int(v.team);
                w.put_int(v.score);
                w.put_int(v.oflags);
            }

            Chan1Server::ReturnFlag(v) => {
                w.put_int(N_RETURNFLAG);
                w.put_int(v.client_num);
                w.put_int(v.flag);
                w.put_int(v.version);
            }

            Chan1Server::TakeFlag(v) => {
                w.put_int(N_TAKEFLAG);
                w.put_int(v.client_num);
                w.put_int(v.flag);
                w.put_int(v.version);
            }

            Chan1Server::ResetFlag(v) => {
                w.put_int(N_RESETFLAG);
                w.put_int(v.flag);
                w.put_int(v.version);
                w.put_int(v.spawn_index);
                w.put_int(v.team);
                w.put_int(v.score);
            }

            Chan1Server::InvisFlag(v) => {
                w.put_int(N_INVISFLAG);
                w.put_int(v.flag);
                w.put_bool(v.invis);
            }

            Chan1Server::Bases(v) => {
                w.put_int(N_BASES);
                w.put_int(v.bases.len() as i32);
                for b in &v.bases {
                    w.put_int(b.ammotype.map_or(0, |t| t as i32));
                    w.put_string(&b.owner);
                    w.put_string(&b.enemy);
                    w.put_int(if b.converted { 1 } else { 0 });
                    w.put_int(b.ammo);
                }
            }
            Chan1Server::BaseInfo(v) => {
                w.put_int(N_BASEINFO);
                w.put_int(v.base);
                w.put_string(&v.owner);
                w.put_string(&v.enemy);
                w.put_bool(v.converted);
                w.put_int(v.ammo);
            }
            Chan1Server::BaseScore(v) => {
                w.put_int(N_BASESCORE);
                w.put_int(v.base);
                w.put_string(&v.team);
                w.put_int(v.total);
            }
            Chan1Server::RepAmmo(v) => {
                w.put_int(N_REPAMMO);
                w.put_int(v.client_num);
                w.put_int(v.ammotype as i32);
            }
            Chan1Server::BaseRegen(v) => {
                w.put_int(N_BASEREGEN);
                w.put_int(v.client_num);
                w.put_int(v.health);
                w.put_int(v.armour);
                w.put_int(v.ammotype as i32);
                w.put_int(v.ammo);
            }
            Chan1Server::InitTokens(v) => {
                w.put_int(N_INITTOKENS);
                w.put_int(v.scores[0]);
                w.put_int(v.scores[1]);
                w.put_int(v.tokens.len() as i32);
                for t in &v.tokens {
                    w.put_int(t.id);
                    w.put_int(t.team);
                    w.put_int(t.yaw);
                    w.put_int((t.pos.x * DMF) as i32);
                    w.put_int((t.pos.y * DMF) as i32);
                    w.put_int((t.pos.z * DMF) as i32);
                }
                for ct in &v.client_tokens {
                    w.put_int(ct.client_num);
                    w.put_int(ct.tokens);
                }
                w.put_int(-1);
            }
            Chan1Server::TakeToken(v) => {
                w.put_int(N_TAKETOKEN);
                w.put_int(v.client_num);
                w.put_int(v.id);
                w.put_int(v.total);
            }
            Chan1Server::ExpireTokens(v) => {
                w.put_int(N_EXPIRETOKENS);
                for &id in &v.tokens {
                    w.put_int(id);
                }
                w.put_int(-1);
            }
            Chan1Server::DropTokens(v) => {
                w.put_int(N_DROPTOKENS);
                w.put_int(v.client_num);
                w.put_int((&v.drop_loc.x * DMF) as i32);
                w.put_int((&v.drop_loc.y * DMF) as i32);
                w.put_int((&v.drop_loc.z * DMF) as i32);
                for t in &v.tokens {
                    w.put_int(t.id);
                    w.put_int(t.team);
                    w.put_int(t.yaw);
                }
                w.put_int(-1);
            }
            Chan1Server::StealTokens(v) => {
                w.put_int(N_STEALTOKENS);
                w.put_int(v.client_num);
                w.put_int(v.team);
                w.put_int(v.basenum);
                w.put_int(v.enemy_team);
                w.put_int(v.score);
                w.put_int((&v.drop_loc.x * DMF) as i32);
                w.put_int((&v.drop_loc.y * DMF) as i32);
                w.put_int((&v.drop_loc.z * DMF) as i32);
                for t in &v.tokens {
                    w.put_int(t.id);
                    w.put_int(t.yaw);
                }
                w.put_int(-1);
            }
            Chan1Server::DepositTokens(v) => {
                w.put_int(N_DEPOSITTOKENS);
                w.put_int(v.client_num);
                w.put_int(v.base);
                w.put_int(v.deposited);
                w.put_int(v.team);
                w.put_int(v.score);
                w.put_int(v.flags);
            }
            Chan1Server::Client(Client {
                client_num: _,
                packet,
            }) => match packet {
                ClientPacket::Sound(v) => {
                    w.put_int(N_SOUND);
                    w.put_int(v.sound_id);
                }

                ClientPacket::Text(v) => {
                    w.put_int(N_TEXT);
                    w.put_string(&v.text);
                }
                ClientPacket::SwitchName(v) => {
                    w.put_int(N_SWITCHNAME);
                    w.put_string(&v.name);
                }

                ClientPacket::SwitchModel(v) => {
                    w.put_int(N_SWITCHMODEL);
                    w.put_int(v.model as i32);
                }
                ClientPacket::Spawn(v) => {
                    w.put_int(N_SPAWN);
                    v.state.encode(w);
                }
                ClientPacket::GunSelect(v) => {
                    w.put_int(N_GUNSELECT);
                    w.put_int(v.gun as i32);
                }

                ClientPacket::Taunt(_) => {
                    w.put_int(N_TAUNT);
                }
                ClientPacket::EditFace(v) => {
                    w.put_int(N_EDITF);
                    v.sel.encode(w);
                    w.put_int(v.dir);
                    w.put_int(v.mode as i32);
                }

                ClientPacket::EditTex(v) => {
                    w.put_int(N_EDITT);
                    v.sel.encode(w);
                    w.put_int(v.tex);
                    w.put_bool(v.all_faces);
                    let extra = v.extra_data.len() as u16;
                    w.put_byte((extra & 0xFF) as u8);
                    w.put_byte((extra >> 8) as u8);
                    w.put_bytes(&v.extra_data);
                }

                ClientPacket::EditMat(v) => {
                    w.put_int(N_EDITM);
                    v.sel.encode(w);
                    w.put_int(v.mat);
                    w.put_int(v.filter);
                }

                ClientPacket::Flip(v) => {
                    w.put_int(N_FLIP);
                    v.sel.encode(w);
                }

                ClientPacket::Copy(v) => {
                    w.put_int(N_COPY);
                    v.sel.encode(w);
                }

                ClientPacket::Paste(v) => {
                    w.put_int(N_PASTE);
                    v.sel.encode(w);
                }

                ClientPacket::Rotate(v) => {
                    w.put_int(N_ROTATE);
                    v.sel.encode(w);
                    w.put_int(v.dir);
                }

                ClientPacket::Replace(v) => {
                    w.put_int(N_REPLACE);
                    v.sel.encode(w);
                    w.put_int(v.old_tex);
                    w.put_int(v.new_tex);
                    w.put_int(v.in_sel);
                    let extra = v.extra_data.len() as u16;
                    w.put_byte((extra & 0xFF) as u8);
                    w.put_byte((extra >> 8) as u8);
                    w.put_bytes(&v.extra_data);
                }

                ClientPacket::DelCube(v) => {
                    w.put_int(N_DELCUBE);
                    v.sel.encode(w);
                }

                ClientPacket::EditVSlot(v) => {
                    w.put_int(N_EDITVSLOT);
                    v.sel.encode(w);
                    w.put_int(v.delta);
                    w.put_bool(v.all_faces);
                    let extra = v.extra_data.len() as u16;
                    w.put_byte((extra & 0xFF) as u8);
                    w.put_byte((extra >> 8) as u8);
                    w.put_bytes(&v.extra_data);
                }

                ClientPacket::Remip(_) => {
                    w.put_int(N_REMIP);
                }

                ClientPacket::EditEnt(v) => {
                    w.put_int(N_EDITENT);
                    w.put_int(v.ent_id);
                    w.put_int((&v.pos.x * DMF) as i32);
                    w.put_int((&v.pos.y * DMF) as i32);
                    w.put_int((&v.pos.z * DMF) as i32);
                    w.put_int(v.type_ as i32);
                    w.put_int(v.attr1);
                    w.put_int(v.attr2);
                    w.put_int(v.attr3);
                    w.put_int(v.attr4);
                    w.put_int(v.attr5);
                }

                ClientPacket::EditVar(v) => {
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
                ClientPacket::ClientPing(v) => {
                    w.put_int(N_CLIENTPING);
                    w.put_int(v.ping);
                }
                ClientPacket::EditMode(v) => {
                    w.put_int(N_EDITMODE);
                    w.put_bool(v.is_editing);
                }
                ClientPacket::NewMap(v) => {
                    w.put_int(N_NEWMAP);
                    w.put_int(v.size);
                }
            },
        }
    }
}
