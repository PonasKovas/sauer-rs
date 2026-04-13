use crate::{
    Chan1Server,
    data::{EditVarValue, FlagStateHoldTeam, FlagStateOwner},
    internal::{Encode, WriterCore, constants::*},
};

impl Encode for Chan1Server {
    fn requires_client_wrapper(&self) -> Option<i32> {
        match self {
            Chan1Server::Sound { client_num, .. }
            | Chan1Server::Text { client_num, .. }
            | Chan1Server::SwitchName { client_num, .. }
            | Chan1Server::SwitchModel { client_num, .. }
            | Chan1Server::Spawn { client_num, .. }
            | Chan1Server::GunSelect { client_num, .. }
            | Chan1Server::Taunt { client_num, .. }
            | Chan1Server::Remip { client_num, .. }
            | Chan1Server::EditEnt { client_num, .. }
            | Chan1Server::EditVar { client_num, .. }
            | Chan1Server::ClientPing { client_num, .. }
            | Chan1Server::EditMode { client_num, .. }
            | Chan1Server::EditFace { client_num, .. }
            | Chan1Server::EditTex { client_num, .. }
            | Chan1Server::EditMat { client_num, .. }
            | Chan1Server::Flip { client_num, .. }
            | Chan1Server::Copy { client_num, .. }
            | Chan1Server::Paste { client_num, .. }
            | Chan1Server::Rotate { client_num, .. }
            | Chan1Server::Replace { client_num, .. }
            | Chan1Server::DelCube { client_num, .. }
            | Chan1Server::EditVSlot { client_num, .. }
            | Chan1Server::NewMap {
                client_num: Some(client_num),
                ..
            } => Some(*client_num),
            _ => None,
        }
    }

    fn encode(&self, w: &mut WriterCore) {
        match self {
            Chan1Server::DemoPacket => w.put_int(N_DEMOPACKET),

            Chan1Server::ServInfo {
                client_num,
                protocol,
                session_id,
                has_password,
                server_desc,
                server_auth,
            } => {
                w.put_int(N_SERVINFO);
                w.put_int(*client_num);
                w.put_int(*protocol);
                w.put_int(*session_id);
                w.put_int(if *has_password { 1 } else { 0 });
                w.put_string(server_desc);
                w.put_string(server_auth);
            }

            Chan1Server::Welcome => w.put_int(N_WELCOME),

            Chan1Server::PauseGame {
                is_paused,
                client_num,
            } => {
                w.put_int(N_PAUSEGAME);
                w.put_int(if *is_paused { 1 } else { 0 });
                w.put_int(*client_num);
            }

            Chan1Server::GameSpeed { speed, client_num } => {
                w.put_int(N_GAMESPEED);
                w.put_int(*speed);
                w.put_int(*client_num);
            }

            Chan1Server::Sound {
                client_num: _,
                sound_id,
            } => {
                w.put_int(N_SOUND);
                w.put_int(*sound_id);
            }

            Chan1Server::Text {
                client_num: _,
                text,
            } => {
                w.put_int(N_TEXT);
                w.put_string(text);
            }

            Chan1Server::SayTeam { client_num, text } => {
                w.put_int(N_SAYTEAM);
                w.put_int(*client_num);
                w.put_string(text);
            }

            Chan1Server::MapChange {
                map_name,
                mode,
                has_items,
            } => {
                w.put_int(N_MAPCHANGE);
                w.put_string(map_name);
                w.put_int(*mode as i32);
                w.put_int(if *has_items { 1 } else { 0 });
            }

            Chan1Server::ForceDeath { client_num } => {
                w.put_int(N_FORCEDEATH);
                w.put_int(*client_num);
            }

            Chan1Server::ItemList { items } => {
                w.put_int(N_ITEMLIST);
                for item in items {
                    w.put_int(item.item_index);
                    w.put_int(item.item_type as i32);
                }
                w.put_int(-1); // Terminator
            }

            Chan1Server::InitClient {
                client_num,
                name,
                team,
                player_model,
            } => {
                w.put_int(N_INITCLIENT);
                w.put_int(*client_num);
                w.put_string(name);
                w.put_string(team);
                w.put_int(*player_model as i32);
            }

            Chan1Server::SwitchName {
                client_num: _,
                name,
            } => {
                w.put_int(N_SWITCHNAME);
                w.put_string(name);
            }

            Chan1Server::SwitchModel {
                client_num: _,
                model,
            } => {
                w.put_int(N_SWITCHMODEL);
                w.put_int(*model as i32);
            }

            Chan1Server::ClientDisconnected { client_num } => {
                w.put_int(N_CDIS);
                w.put_int(*client_num);
            }

            Chan1Server::Spawn {
                client_num: _,
                state,
            } => {
                w.put_int(N_SPAWN);
                state.encode(w);
            }

            Chan1Server::SpawnState { client_num, state } => {
                w.put_int(N_SPAWNSTATE);
                w.put_int(*client_num);
                state.encode(w);
            }

            Chan1Server::ShotFx {
                client_num,
                gun,
                id,
                from,
                to,
            } => {
                w.put_int(N_SHOTFX);
                w.put_int(*client_num);
                w.put_int(*gun as i32);
                w.put_int(*id);
                w.put_int((from.x * DMF) as i32);
                w.put_int((from.y * DMF) as i32);
                w.put_int((from.z * DMF) as i32);
                w.put_int((to.x * DMF) as i32);
                w.put_int((to.y * DMF) as i32);
                w.put_int((to.z * DMF) as i32);
            }

            Chan1Server::ExplodeFx {
                client_num,
                gun,
                id,
            } => {
                w.put_int(N_EXPLODEFX);
                w.put_int(*client_num);
                w.put_int(*gun as i32);
                w.put_int(*id);
            }

            Chan1Server::Damage {
                target_cn,
                actor_cn,
                damage,
                armour,
                health,
            } => {
                w.put_int(N_DAMAGE);
                w.put_int(*target_cn);
                w.put_int(*actor_cn);
                w.put_int(*damage);
                w.put_int(*armour);
                w.put_int(*health);
            }

            Chan1Server::HitPush {
                target_cn,
                gun,
                damage,
                dir,
            } => {
                w.put_int(N_HITPUSH);
                w.put_int(*target_cn);
                w.put_int(*gun as i32);
                w.put_int(*damage);
                w.put_int((dir.x * DMF) as i32);
                w.put_int((dir.y * DMF) as i32);
                w.put_int((dir.z * DMF) as i32);
            }

            Chan1Server::Died {
                victim_cn,
                actor_cn,
                actor_frags,
                team_frags,
            } => {
                w.put_int(N_DIED);
                w.put_int(*victim_cn);
                w.put_int(*actor_cn);
                w.put_int(*actor_frags);
                w.put_int(*team_frags);
            }

            Chan1Server::TeamInfo { teams } => {
                w.put_int(N_TEAMINFO);
                for team in teams {
                    w.put_string(&team.team);
                    w.put_int(team.frags);
                }
                w.put_byte(0); // empty string terminator
            }

            Chan1Server::GunSelect { client_num: _, gun } => {
                w.put_int(N_GUNSELECT);
                w.put_int(*gun as i32);
            }

            Chan1Server::Taunt { client_num: _ } => {
                w.put_int(N_TAUNT);
            }

            Chan1Server::Resume { clients } => {
                w.put_int(N_RESUME);
                for c in clients {
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

            Chan1Server::ItemSpawn { item_index } => {
                w.put_int(N_ITEMSPAWN);
                w.put_int(*item_index);
            }

            Chan1Server::ItemAck {
                item_index,
                client_num,
            } => {
                w.put_int(N_ITEMACC);
                w.put_int(*item_index);
                w.put_int(*client_num);
            }

            Chan1Server::Clipboard {
                client_num,
                unpack_len,
                pack_len,
                data,
            } => {
                w.put_int(N_CLIPBOARD);
                w.put_int(*client_num);
                w.put_int(*unpack_len);
                w.put_int(*pack_len);
                w.put_bytes(data);
            }

            Chan1Server::Undo {
                client_num,
                unpack_len,
                pack_len,
                data,
            } => {
                w.put_int(N_UNDO);
                w.put_int(*client_num);
                w.put_int(*unpack_len);
                w.put_int(*pack_len);
                w.put_bytes(data);
            }

            Chan1Server::Redo {
                client_num,
                unpack_len,
                pack_len,
                data,
            } => {
                w.put_int(N_REDO);
                w.put_int(*client_num);
                w.put_int(*unpack_len);
                w.put_int(*pack_len);
                w.put_bytes(data);
            }

            Chan1Server::EditFace {
                client_num: _,
                sel,
                dir,
                mode,
            } => {
                w.put_int(N_EDITF);
                sel.encode(w);
                w.put_int(*dir);
                w.put_int(*mode as i32);
            }

            Chan1Server::EditTex {
                client_num: _,
                sel,
                tex,
                all_faces,
                extra_data,
            } => {
                w.put_int(N_EDITT);
                sel.encode(w);
                w.put_int(*tex);
                w.put_int(if *all_faces { 1 } else { 0 });
                let extra = extra_data.len() as u16;
                w.put_byte((extra & 0xFF) as u8);
                w.put_byte((extra >> 8) as u8);
                w.put_bytes(extra_data);
            }

            Chan1Server::EditMat {
                client_num: _,
                sel,
                mat,
                filter,
            } => {
                w.put_int(N_EDITM);
                sel.encode(w);
                w.put_int(*mat);
                w.put_int(*filter);
            }

            Chan1Server::Flip { client_num: _, sel } => {
                w.put_int(N_FLIP);
                sel.encode(w);
            }

            Chan1Server::Copy { client_num: _, sel } => {
                w.put_int(N_COPY);
                sel.encode(w);
            }

            Chan1Server::Paste { client_num: _, sel } => {
                w.put_int(N_PASTE);
                sel.encode(w);
            }

            Chan1Server::Rotate {
                client_num: _,
                sel,
                dir,
            } => {
                w.put_int(N_ROTATE);
                sel.encode(w);
                w.put_int(*dir);
            }

            Chan1Server::Replace {
                client_num: _,
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
                let extra = extra_data.len() as u16;
                w.put_byte((extra & 0xFF) as u8);
                w.put_byte((extra >> 8) as u8);
                w.put_bytes(extra_data);
            }

            Chan1Server::DelCube { client_num: _, sel } => {
                w.put_int(N_DELCUBE);
                sel.encode(w);
            }

            Chan1Server::EditVSlot {
                client_num: _,
                sel,
                delta,
                all_faces,
                extra_data,
            } => {
                w.put_int(N_EDITVSLOT);
                sel.encode(w);
                w.put_int(*delta);
                w.put_int(if *all_faces { 1 } else { 0 });
                let extra = extra_data.len() as u16;
                w.put_byte((extra & 0xFF) as u8);
                w.put_byte((extra >> 8) as u8);
                w.put_bytes(extra_data);
            }

            Chan1Server::Remip { client_num: _ } => {
                w.put_int(N_REMIP);
            }

            Chan1Server::EditEnt {
                client_num: _,
                ent_id,
                pos,
                type_,
                attr1,
                attr2,
                attr3,
                attr4,
                attr5,
            } => {
                w.put_int(N_EDITENT);
                w.put_int(*ent_id);
                w.put_int((pos.x * DMF) as i32);
                w.put_int((pos.y * DMF) as i32);
                w.put_int((pos.z * DMF) as i32);
                w.put_int(*type_ as i32);
                w.put_int(*attr1);
                w.put_int(*attr2);
                w.put_int(*attr3);
                w.put_int(*attr4);
                w.put_int(*attr5);
            }

            Chan1Server::EditVar {
                client_num: _,
                name,
                val,
            } => {
                w.put_int(N_EDITVAR);
                match val {
                    EditVarValue::Int(i) => {
                        w.put_int(0);
                        w.put_string(name);
                        w.put_int(*i);
                    }
                    EditVarValue::Float(f) => {
                        w.put_int(1);
                        w.put_string(name);
                        w.put_float(*f);
                    }
                    EditVarValue::String(s) => {
                        w.put_int(2);
                        w.put_string(name);
                        w.put_string(s);
                    }
                }
            }

            Chan1Server::Pong { ping } => {
                w.put_int(N_PONG);
                w.put_int(*ping);
            }

            Chan1Server::ClientPing {
                client_num: _,
                ping,
            } => {
                w.put_int(N_CLIENTPING);
                w.put_int(*ping);
            }

            Chan1Server::TimeUp { seconds_remaining } => {
                w.put_int(N_TIMEUP);
                w.put_int(*seconds_remaining);
            }

            Chan1Server::ServMsg { text } => {
                w.put_int(N_SERVMSG);
                w.put_string(text);
            }

            Chan1Server::SendDemoList { demos } => {
                w.put_int(N_SENDDEMOLIST);
                w.put_int(demos.len() as i32);
                for demo in demos {
                    w.put_string(demo);
                }
            }

            Chan1Server::DemoPlayback { is_on, client_num } => {
                w.put_int(N_DEMOPLAYBACK);
                w.put_int(if *is_on { 1 } else { 0 });
                w.put_int(*client_num);
            }

            Chan1Server::CurrentMaster {
                master_mode,
                clients,
            } => {
                w.put_int(N_CURRENTMASTER);
                w.put_int(*master_mode as i32);
                for c in clients {
                    w.put_int(c.client_num);
                    w.put_int(c.privilege as i32);
                }
                w.put_int(-1);
            }

            Chan1Server::MasterMode { master_mode } => {
                w.put_int(N_MASTERMODE);
                w.put_int(*master_mode as i32);
            }

            Chan1Server::EditMode {
                client_num: _,
                is_editing,
            } => {
                w.put_int(N_EDITMODE);
                w.put_int(if *is_editing { 1 } else { 0 });
            }

            Chan1Server::Spectator {
                client_num,
                is_spectator,
            } => {
                w.put_int(N_SPECTATOR);
                w.put_int(*client_num);
                w.put_int(if *is_spectator { 1 } else { 0 });
            }

            Chan1Server::SetTeam {
                client_num,
                team,
                reason,
            } => {
                w.put_int(N_SETTEAM);
                w.put_int(*client_num);
                w.put_string(team);
                w.put_int(*reason);
            }

            Chan1Server::Announce { item_type } => {
                w.put_int(N_ANNOUNCE);
                w.put_int(*item_type as i32);
            }

            Chan1Server::NewMap {
                client_num: _,
                size,
            } => {
                w.put_int(N_NEWMAP);
                w.put_int(*size);
            }

            Chan1Server::ReqAuth { auth_domain } => {
                w.put_int(N_REQAUTH);
                w.put_string(auth_domain);
            }

            Chan1Server::AuthChal {
                desc,
                id,
                challenge,
            } => {
                w.put_int(N_AUTHCHAL);
                w.put_string(desc);
                w.put_int(*id as i32);
                w.put_string(challenge);
            }

            Chan1Server::InitAi {
                bot_cn,
                owner_cn,
                ai_type,
                skill,
                player_model,
                name,
                team,
            } => {
                w.put_int(N_INITAI);
                w.put_int(*bot_cn);
                w.put_int(*owner_cn);
                w.put_int(*ai_type as i32);
                w.put_int(*skill);
                w.put_int(*player_model as i32);
                w.put_string(name);
                w.put_string(team);
            }

            Chan1Server::ServCmd { cmd } => {
                w.put_int(N_SERVCMD);
                w.put_string(cmd);
            }

            Chan1Server::InitFlags { scores, flags } => {
                w.put_int(N_INITFLAGS);
                w.put_int(scores[0]);
                w.put_int(scores[1]);

                w.put_int(flags.len() as i32);
                for f in flags {
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

            Chan1Server::DropFlag {
                client_num,
                flag,
                version,
                drop_loc,
            } => {
                w.put_int(N_DROPFLAG);
                w.put_int(*client_num);
                w.put_int(*flag);
                w.put_int(*version);
                w.put_int((drop_loc.x * DMF) as i32);
                w.put_int((drop_loc.y * DMF) as i32);
                w.put_int((drop_loc.z * DMF) as i32);
            }

            Chan1Server::ScoreFlag {
                client_num,
                relay_flag,
                relay_version,
                goal_flag,
                goal_version,
                goal_spawn,
                team,
                score,
                oflags,
            } => {
                w.put_int(N_SCOREFLAG);
                w.put_int(*client_num);
                w.put_int(*relay_flag);
                w.put_int(*relay_version);
                w.put_int(*goal_flag);
                w.put_int(*goal_version);
                w.put_int(*goal_spawn);
                w.put_int(*team);
                w.put_int(*score);
                w.put_int(*oflags);
            }

            Chan1Server::ReturnFlag {
                client_num,
                flag,
                version,
            } => {
                w.put_int(N_RETURNFLAG);
                w.put_int(*client_num);
                w.put_int(*flag);
                w.put_int(*version);
            }

            Chan1Server::TakeFlag {
                client_num,
                flag,
                version,
            } => {
                w.put_int(N_TAKEFLAG);
                w.put_int(*client_num);
                w.put_int(*flag);
                w.put_int(*version);
            }

            Chan1Server::ResetFlag {
                flag,
                version,
                spawn_index,
                team,
                score,
            } => {
                w.put_int(N_RESETFLAG);
                w.put_int(*flag);
                w.put_int(*version);
                w.put_int(*spawn_index);
                w.put_int(*team);
                w.put_int(*score);
            }

            Chan1Server::InvisFlag { flag, invis } => {
                w.put_int(N_INVISFLAG);
                w.put_int(*flag);
                w.put_int(if *invis { 1 } else { 0 });
            }

            Chan1Server::Bases { bases } => {
                w.put_int(N_BASES);
                w.put_int(bases.len() as i32);
                for b in bases {
                    w.put_int(b.ammotype.map_or(0, |t| t as i32));
                    w.put_string(&b.owner);
                    w.put_string(&b.enemy);
                    w.put_int(if b.converted { 1 } else { 0 });
                    w.put_int(b.ammo);
                }
            }
            Chan1Server::BaseInfo {
                base,
                owner,
                enemy,
                converted,
                ammo,
            } => {
                w.put_int(N_BASEINFO);
                w.put_int(*base);
                w.put_string(owner);
                w.put_string(enemy);
                w.put_int(if *converted { 1 } else { 0 });
                w.put_int(*ammo);
            }
            Chan1Server::BaseScore { base, team, total } => {
                w.put_int(N_BASESCORE);
                w.put_int(*base);
                w.put_string(team);
                w.put_int(*total);
            }
            Chan1Server::RepAmmo {
                client_num,
                ammotype,
            } => {
                w.put_int(N_REPAMMO);
                w.put_int(*client_num);
                w.put_int(*ammotype as i32);
            }
            Chan1Server::BaseRegen {
                client_num,
                health,
                armour,
                ammotype,
                ammo,
            } => {
                w.put_int(N_BASEREGEN);
                w.put_int(*client_num);
                w.put_int(*health);
                w.put_int(*armour);
                w.put_int(*ammotype as i32);
                w.put_int(*ammo);
            }
            Chan1Server::InitTokens {
                scores,
                tokens,
                client_tokens,
            } => {
                w.put_int(N_INITTOKENS);
                w.put_int(scores[0]);
                w.put_int(scores[1]);
                w.put_int(tokens.len() as i32);
                for t in tokens {
                    w.put_int(t.id);
                    w.put_int(t.team);
                    w.put_int(t.yaw);
                    w.put_int((t.pos.x * DMF) as i32);
                    w.put_int((t.pos.y * DMF) as i32);
                    w.put_int((t.pos.z * DMF) as i32);
                }
                for ct in client_tokens {
                    w.put_int(ct.client_num);
                    w.put_int(ct.tokens);
                }
                w.put_int(-1);
            }
            Chan1Server::TakeToken {
                client_num,
                id,
                total,
            } => {
                w.put_int(N_TAKETOKEN);
                w.put_int(*client_num);
                w.put_int(*id);
                w.put_int(*total);
            }
            Chan1Server::ExpireTokens { tokens } => {
                w.put_int(N_EXPIRETOKENS);
                for &id in tokens {
                    w.put_int(id);
                }
                w.put_int(-1);
            }
            Chan1Server::DropTokens {
                client_num,
                drop_loc,
                tokens,
            } => {
                w.put_int(N_DROPTOKENS);
                w.put_int(*client_num);
                w.put_int((drop_loc.x * DMF) as i32);
                w.put_int((drop_loc.y * DMF) as i32);
                w.put_int((drop_loc.z * DMF) as i32);
                for t in tokens {
                    w.put_int(t.id);
                    w.put_int(t.team);
                    w.put_int(t.yaw);
                }
                w.put_int(-1);
            }
            Chan1Server::StealTokens {
                client_num,
                team,
                basenum,
                enemy_team,
                score,
                drop_loc,
                tokens,
            } => {
                w.put_int(N_STEALTOKENS);
                w.put_int(*client_num);
                w.put_int(*team);
                w.put_int(*basenum);
                w.put_int(*enemy_team);
                w.put_int(*score);
                w.put_int((drop_loc.x * DMF) as i32);
                w.put_int((drop_loc.y * DMF) as i32);
                w.put_int((drop_loc.z * DMF) as i32);
                for t in tokens {
                    w.put_int(t.id);
                    w.put_int(t.yaw);
                }
                w.put_int(-1);
            }
            Chan1Server::DepositTokens {
                client_num,
                base,
                deposited,
                team,
                score,
                flags,
            } => {
                w.put_int(N_DEPOSITTOKENS);
                w.put_int(*client_num);
                w.put_int(*base);
                w.put_int(*deposited);
                w.put_int(*team);
                w.put_int(*score);
                w.put_int(*flags);
            }
        }
    }
}
