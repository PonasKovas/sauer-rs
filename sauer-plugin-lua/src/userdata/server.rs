use crate::extra::LuaExtraRegistry;
use crate::userdata::task_handle::TaskHandle;
use mlua::{
    FromLua, IntoLua, Lua, RegistryKey, Table, UserData, UserDataFields, UserDataMethods, Value,
};
use sauer_proto::{channel1, extinfo::{UdpResponse, ExtInfoResponse, ScoreData}};
use std::net::SocketAddr;
use cube_string::CubeString;
use sauer_server::Server;
use std::{sync::Arc, time::Duration};
use tokio::time::{Instant, sleep_until};

pub struct LuaServer {
    pub inner: Arc<Server>,
    extra_key: RegistryKey,
}

impl LuaServer {
    /// Creates a new `LuaServer` and builds the `server.extra` table.
    ///
    /// The extra table is a plain Lua table with a metatable whose `__index`
    /// falls through to `LuaExtraRegistry` on miss, and whose `__newindex`
    /// uses `rawset` so Lua writes land directly in the table without looping
    /// back through the metamethod.
    pub fn new(lua: &Lua, server: &Arc<Server>) -> mlua::Result<Self> {
        let extra_table = lua.create_table()?;

        let mt = lua.create_table()?;

        // __index: on miss, look up the Rust factory registry
        let server_clone = Arc::clone(server);
        mt.set(
            "__index",
            lua.create_function(move |lua, (_t, key): (Value, String)| {
                let factory = server_clone
                    .extra
                    .get::<LuaExtraRegistry>()
                    .and_then(|r| r.entries.get(&key))
                    .map(Arc::clone);

                match factory {
                    Some(f) => f(lua),
                    None => Ok(Value::Nil),
                }
            })?,
        )?;

        // __newindex: rawset so subsequent reads hit the table directly
        mt.set(
            "__newindex",
            lua.create_function(|_lua, (t, k, v): (Table, Value, Value)| t.raw_set(k, v))?,
        )?;

        extra_table.set_metatable(Some(mt))?;

        let extra_key = lua.create_registry_value(extra_table)?;

        Ok(Self {
            inner: Arc::clone(server),
            extra_key,
        })
    }
}

impl UserData for LuaServer {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("extra", |lua, this| {
            let table: Table = lua.registry_value(&this.extra_key)?;
            table.into_lua(lua)
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("disconnect", |_lua, this, cn: u32| {
            this.inner.disconnect(cn);
            Ok(())
        });

        methods.add_method("peer_address", |_lua, this, cn: u32| {
            Ok(this
                .inner
                .peer_addr(cn)
                .map(|a| a.to_string())
                .unwrap_or_default())
        });

        methods.add_method("send_udp", |lua, this, (addr_str, table): (String, Table)| {
            let addr: SocketAddr = addr_str.parse().map_err(|e| mlua::Error::RuntimeError(format!("invalid socket addr: {e}")))?;
            let t: String = table.get("type")?;
            
            let response = match t.as_str() {
                "ServerInfo" => UdpResponse::ServerInfo {
                    ping_id: table.get("ping_id")?,
                    num_clients: table.get("num_clients")?,
                    protocol_version: table.get("protocol_version")?,
                    game_mode: table.get("game_mode")?,
                    remaining_time: table.get("remaining_time")?,
                    max_clients: table.get("max_clients")?,
                    master_mode: table.get("master_mode")?,
                    is_paused: table.get("is_paused")?,
                    game_speed: table.get("game_speed")?,
                    map_name: table.get("map_name")?,
                    server_desc: table.get("server_desc")?,
                },
                "Uptime" => UdpResponse::ExtInfo(ExtInfoResponse::Uptime {
                    seconds: table.get("seconds")?,
                }),
                "PlayerStats" => UdpResponse::ExtInfo(ExtInfoResponse::PlayerStats {
                    client_number: table.get("client_number")?,
                    ok: table.get("ok")?,
                }),
                "PlayerStatsIds" => {
                    let ids: Vec<i32> = table.get("ids")?;
                    UdpResponse::ExtInfo(ExtInfoResponse::PlayerStatsIds { ids })
                },
                "PlayerStatsStats" => {
                    let ip_table: Option<Vec<u8>> = table.get("ip")?;
                    let ip = ip_table.and_then(|v| {
                        if v.len() == 3 {
                            Some([v[0], v[1], v[2]])
                        } else {
                            None
                        }
                    });

                    UdpResponse::ExtInfo(ExtInfoResponse::PlayerStatsStats {
                        client_num: table.get("client_num")?,
                        ping: table.get("ping")?,
                        name: table.get("name")?,
                        team: table.get("team")?,
                        frags: table.get("frags")?,
                        flags: table.get("flags")?,
                        deaths: table.get("deaths")?,
                        teamkills: table.get("teamkills")?,
                        damage: table.get("damage")?,
                        health: table.get("health")?,
                        armour: table.get("armour")?,
                        gunselect: table.get("gunselect")?,
                        privilege: table.get("privilege")?,
                        state: table.get("state")?,
                        ip,
                    })
                },
                "TeamScore" => {
                    let scores_table: Option<Vec<Table>> = table.get("scores")?;
                    let scores = scores_table.map(|v| {
                        v.into_iter().filter_map(|t| {
                            let team_name = t.get("team_name").ok()?;
                            let score = t.get("score").ok()?;
                            Some(ScoreData { team_name, score })
                        }).collect()
                    });

                    UdpResponse::ExtInfo(ExtInfoResponse::TeamScore {
                        game_mode: table.get("game_mode")?,
                        remaining_time: table.get("remaining_time")?,
                        scores,
                    })
                },
                _ => return Err(mlua::Error::RuntimeError(format!("unknown UDP response type: {t}"))),
            };

            this.inner.send_udp(addr, &response);
            Ok(())
        });

        // Spawn a background tokio task that calls `func()` every
        // `interval_ms` milliseconds.  Returns a `TaskHandle` with a
        // `cancel()` method.
        methods.add_method(
            "set_interval",
            |_lua, _this, (interval_ms, func): (u64, mlua::Function)| {
                let handle = tokio::spawn(async move {
                    let mut interval = tokio::time::interval(Duration::from_millis(interval_ms));

                    loop {
                        interval.tick().await;
                        if let Err(e) = func.call::<()>(()) {
                            tracing::error!(target: "lua", "set_interval: {e}");
                            break;
                        }
                    }
                });
                Ok(TaskHandle(handle))
            },
        );

        // Spawn a background tokio task that calls `func()` after
        // `interval_ms` milliseconds.  Returns a `TaskHandle` with a
        // `cancel()` method.
        methods.add_method(
            "set_timeout",
            |_lua, _this, (timeout_ms, func): (u64, mlua::Function)| {
                let handle = tokio::spawn(async move {
                    let timeout = Instant::now() + Duration::from_millis(timeout_ms);

                    sleep_until(timeout).await;

                    if let Err(e) = func.call::<()>(()) {
                        tracing::error!(target: "lua", "set_timeout: {e}");
                    }
                });
                Ok(TaskHandle(handle))
            },
        );

        // ── Local macros to cut boilerplate ─────────────────────────────────

        /// Send a Chan1 packet to one client by CN.
        macro_rules! add_send {
            ($name:literal, $ty:ty) => {
                methods.add_method($name, |lua, this, (cn, table): (u32, Table)| {
                    let packet = <$ty>::from_lua(Value::Table(table), lua)?;
                    this.inner.send_chan1(cn, &packet);
                    Ok(())
                });
            };
        }

        /// Broadcast a Chan1 packet to all connected clients.
        macro_rules! add_broadcast {
            ($name:literal, $ty:ty) => {
                methods.add_method($name, |lua, this, table: Table| {
                    let packet = <$ty>::from_lua(Value::Table(table), lua)?;
                    this.inner.broadcast_chan1(&packet);
                    Ok(())
                });
            };
        }

        // ── Non-client-wrapped S2C packets ───────────────────────────────────

        add_send!("send_demo_packet", channel1::DemoPacket);
        add_send!("send_serv_info", channel1::ServInfo);
        add_send!("send_welcome", channel1::Welcome);
        add_send!("send_pause_game", channel1::PauseGameServer);
        add_send!("send_game_speed", channel1::GameSpeedServer);
        add_send!("send_say_team", channel1::SayTeamServer);
        add_send!("send_map_change", channel1::MapChange);
        add_send!("send_force_death", channel1::ForceDeath);
        add_send!("send_item_list", channel1::ItemList);
        add_send!("send_init_client", channel1::InitClient);
        add_send!("send_client_disconnected", channel1::ClientDisconnected);
        add_send!("send_spawn_state", channel1::SpawnState);
        add_send!("send_shot_fx", channel1::ShotFx);
        add_send!("send_explode_fx", channel1::ExplodeFx);
        add_send!("send_damage", channel1::Damage);
        add_send!("send_hit_push", channel1::HitPush);
        add_send!("send_died", channel1::Died);
        add_send!("send_team_info", channel1::TeamInfo);
        add_send!("send_resume", channel1::Resume);
        add_send!("send_item_spawn", channel1::ItemSpawn);
        add_send!("send_item_ack", channel1::ItemAck);
        add_send!("send_clipboard", channel1::ClipboardServer);
        add_send!("send_undo", channel1::UndoServer);
        add_send!("send_redo", channel1::RedoServer);
        add_send!("send_pong", channel1::Pong);
        add_send!("send_time_up", channel1::TimeUp);
        add_send!("send_serv_msg", channel1::ServMsg);
        add_send!("send_demo_list", channel1::SendDemoList);
        add_send!("send_demo_playback", channel1::DemoPlayback);
        add_send!("send_current_master", channel1::CurrentMaster);
        add_send!("send_master_mode", channel1::MasterModeServer);
        add_send!("send_spectator", channel1::SpectatorServer);
        add_send!("send_set_team", channel1::SetTeamServer);
        add_send!("send_announce", channel1::Announce);
        add_send!("send_new_map", channel1::NewMap);
        add_send!("send_req_auth", channel1::ReqAuth);
        add_send!("send_auth_chal", channel1::AuthChal);
        add_send!("send_init_ai", channel1::InitAi);
        add_send!("send_serv_cmd", channel1::ServCmd);
        add_send!("send_bases", channel1::BasesServer);
        add_send!("send_base_info", channel1::BaseInfo);
        add_send!("send_base_score", channel1::BaseScore);
        add_send!("send_rep_ammo", channel1::RepAmmoServer);
        add_send!("send_base_regen", channel1::BaseRegen);
        add_send!("send_init_flags", channel1::InitFlagsServer);
        add_send!("send_drop_flag", channel1::DropFlag);
        add_send!("send_score_flag", channel1::ScoreFlag);
        add_send!("send_return_flag", channel1::ReturnFlag);
        add_send!("send_take_flag", channel1::TakeFlagServer);
        add_send!("send_reset_flag", channel1::ResetFlag);
        add_send!("send_invis_flag", channel1::InvisFlag);
        add_send!("send_init_tokens", channel1::InitTokensServer);
        add_send!("send_take_token", channel1::TakeTokenServer);
        add_send!("send_expire_tokens", channel1::ExpireTokens);
        add_send!("send_drop_tokens", channel1::DropTokens);
        add_send!("send_steal_tokens", channel1::StealTokens);
        add_send!("send_deposit_tokens", channel1::DepositTokensServer);

        // ── Client-wrapped S2C packets ───────────────────────────────────────
        // Table must include `client_num` alongside the packet fields.

        add_send!("send_sound", channel1::Client::<channel1::Sound>);
        add_send!("send_text", channel1::Client::<channel1::Text>);
        add_send!("send_switch_name", channel1::Client::<channel1::SwitchName>);
        add_send!(
            "send_switch_model",
            channel1::Client::<channel1::SwitchModel>
        );
        add_send!("send_spawn", channel1::Client::<channel1::SpawnServer>);
        add_send!("send_gun_select", channel1::Client::<channel1::GunSelect>);
        add_send!("send_taunt", channel1::Client::<channel1::Taunt>);
        add_send!("send_remip", channel1::Client::<channel1::Remip>);
        add_send!("send_edit_ent", channel1::Client::<channel1::EditEnt>);
        add_send!("send_edit_var", channel1::Client::<channel1::EditVar>);
        add_send!("send_client_ping", channel1::Client::<channel1::ClientPing>);
        add_send!("send_edit_mode", channel1::Client::<channel1::EditMode>);
        add_send!("send_edit_face", channel1::Client::<channel1::EditFace>);
        add_send!("send_edit_tex", channel1::Client::<channel1::EditTex>);
        add_send!("send_edit_mat", channel1::Client::<channel1::EditMat>);
        add_send!("send_flip", channel1::Client::<channel1::Flip>);
        add_send!("send_copy", channel1::Client::<channel1::CopyServer>);
        add_send!("send_paste", channel1::Client::<channel1::PasteServer>);
        add_send!("send_rotate", channel1::Client::<channel1::Rotate>);
        add_send!("send_replace", channel1::Client::<channel1::Replace>);
        add_send!("send_del_cube", channel1::Client::<channel1::DelCube>);
        add_send!("send_edit_vslot", channel1::Client::<channel1::EditVSlot>);
        add_send!("send_new_map_client", channel1::Client::<channel1::NewMap>);

        // ── Broadcast variants ───────────────────────────────────────────────

        add_broadcast!("broadcast_demo_packet", channel1::DemoPacket);
        add_broadcast!("broadcast_serv_info", channel1::ServInfo);
        add_broadcast!("broadcast_welcome", channel1::Welcome);
        add_broadcast!("broadcast_pause_game", channel1::PauseGameServer);
        add_broadcast!("broadcast_game_speed", channel1::GameSpeedServer);
        add_broadcast!("broadcast_say_team", channel1::SayTeamServer);
        add_broadcast!("broadcast_map_change", channel1::MapChange);
        add_broadcast!("broadcast_force_death", channel1::ForceDeath);
        add_broadcast!("broadcast_item_list", channel1::ItemList);
        add_broadcast!("broadcast_init_client", channel1::InitClient);
        add_broadcast!(
            "broadcast_client_disconnected",
            channel1::ClientDisconnected
        );
        add_broadcast!("broadcast_spawn_state", channel1::SpawnState);
        add_broadcast!("broadcast_shot_fx", channel1::ShotFx);
        add_broadcast!("broadcast_explode_fx", channel1::ExplodeFx);
        add_broadcast!("broadcast_damage", channel1::Damage);
        add_broadcast!("broadcast_hit_push", channel1::HitPush);
        add_broadcast!("broadcast_died", channel1::Died);
        add_broadcast!("broadcast_team_info", channel1::TeamInfo);
        add_broadcast!("broadcast_resume", channel1::Resume);
        add_broadcast!("broadcast_item_spawn", channel1::ItemSpawn);
        add_broadcast!("broadcast_item_ack", channel1::ItemAck);
        add_broadcast!("broadcast_clipboard", channel1::ClipboardServer);
        add_broadcast!("broadcast_undo", channel1::UndoServer);
        add_broadcast!("broadcast_redo", channel1::RedoServer);
        add_broadcast!("broadcast_pong", channel1::Pong);
        add_broadcast!("broadcast_time_up", channel1::TimeUp);
        add_broadcast!("broadcast_serv_msg", channel1::ServMsg);
        add_broadcast!("broadcast_demo_list", channel1::SendDemoList);
        add_broadcast!("broadcast_demo_playback", channel1::DemoPlayback);
        add_broadcast!("broadcast_current_master", channel1::CurrentMaster);
        add_broadcast!("broadcast_master_mode", channel1::MasterModeServer);
        add_broadcast!("broadcast_spectator", channel1::SpectatorServer);
        add_broadcast!("broadcast_set_team", channel1::SetTeamServer);
        add_broadcast!("broadcast_announce", channel1::Announce);
        add_broadcast!("broadcast_new_map", channel1::NewMap);
        add_broadcast!("broadcast_req_auth", channel1::ReqAuth);
        add_broadcast!("broadcast_auth_chal", channel1::AuthChal);
        add_broadcast!("broadcast_init_ai", channel1::InitAi);
        add_broadcast!("broadcast_serv_cmd", channel1::ServCmd);
        add_broadcast!("broadcast_bases", channel1::BasesServer);
        add_broadcast!("broadcast_base_info", channel1::BaseInfo);
        add_broadcast!("broadcast_base_score", channel1::BaseScore);
        add_broadcast!("broadcast_rep_ammo", channel1::RepAmmoServer);
        add_broadcast!("broadcast_base_regen", channel1::BaseRegen);
        add_broadcast!("broadcast_init_flags", channel1::InitFlagsServer);
        add_broadcast!("broadcast_drop_flag", channel1::DropFlag);
        add_broadcast!("broadcast_score_flag", channel1::ScoreFlag);
        add_broadcast!("broadcast_return_flag", channel1::ReturnFlag);
        add_broadcast!("broadcast_take_flag", channel1::TakeFlagServer);
        add_broadcast!("broadcast_reset_flag", channel1::ResetFlag);
        add_broadcast!("broadcast_invis_flag", channel1::InvisFlag);
        add_broadcast!("broadcast_init_tokens", channel1::InitTokensServer);
        add_broadcast!("broadcast_take_token", channel1::TakeTokenServer);
        add_broadcast!("broadcast_expire_tokens", channel1::ExpireTokens);
        add_broadcast!("broadcast_drop_tokens", channel1::DropTokens);
        add_broadcast!("broadcast_steal_tokens", channel1::StealTokens);
        add_broadcast!("broadcast_deposit_tokens", channel1::DepositTokensServer);

        add_broadcast!("broadcast_sound", channel1::Client::<channel1::Sound>);
        add_broadcast!("broadcast_text", channel1::Client::<channel1::Text>);
        add_broadcast!(
            "broadcast_switch_name",
            channel1::Client::<channel1::SwitchName>
        );
        add_broadcast!(
            "broadcast_switch_model",
            channel1::Client::<channel1::SwitchModel>
        );
        add_broadcast!("broadcast_spawn", channel1::Client::<channel1::SpawnServer>);
        add_broadcast!(
            "broadcast_gun_select",
            channel1::Client::<channel1::GunSelect>
        );
        add_broadcast!("broadcast_taunt", channel1::Client::<channel1::Taunt>);
        add_broadcast!("broadcast_remip", channel1::Client::<channel1::Remip>);
        add_broadcast!("broadcast_edit_ent", channel1::Client::<channel1::EditEnt>);
        add_broadcast!("broadcast_edit_var", channel1::Client::<channel1::EditVar>);
        add_broadcast!(
            "broadcast_client_ping",
            channel1::Client::<channel1::ClientPing>
        );
        add_broadcast!(
            "broadcast_edit_mode",
            channel1::Client::<channel1::EditMode>
        );
        add_broadcast!(
            "broadcast_edit_face",
            channel1::Client::<channel1::EditFace>
        );
        add_broadcast!("broadcast_edit_tex", channel1::Client::<channel1::EditTex>);
        add_broadcast!("broadcast_edit_mat", channel1::Client::<channel1::EditMat>);
        add_broadcast!("broadcast_flip", channel1::Client::<channel1::Flip>);
        add_broadcast!("broadcast_copy", channel1::Client::<channel1::CopyServer>);
        add_broadcast!("broadcast_paste", channel1::Client::<channel1::PasteServer>);
        add_broadcast!("broadcast_rotate", channel1::Client::<channel1::Rotate>);
        add_broadcast!("broadcast_replace", channel1::Client::<channel1::Replace>);
        add_broadcast!("broadcast_del_cube", channel1::Client::<channel1::DelCube>);
        add_broadcast!(
            "broadcast_edit_vslot",
            channel1::Client::<channel1::EditVSlot>
        );
        add_broadcast!(
            "broadcast_new_map_client",
            channel1::Client::<channel1::NewMap>
        );
    }
}
