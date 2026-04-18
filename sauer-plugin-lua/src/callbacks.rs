use crate::{Instance, LuaPlugin, LuaPluginOptions};
use mlua::Lua;

#[derive(Default)]
pub(crate) struct Callbacks {
    pub init: Vec<LuaCallback>,
    pub start: Vec<LuaCallback>,
    pub connected: Vec<LuaCallback>,
    pub disconnected: Vec<LuaCallback>,
    pub map: Vec<LuaCallback>,

    // Channel 0
    pub pos: Vec<LuaCallback>,
    pub teleport: Vec<LuaCallback>,
    pub jumppad: Vec<LuaCallback>,

    // Channel 1
    pub connect: Vec<LuaCallback>,
    pub auth_answer: Vec<LuaCallback>,
    pub ping: Vec<LuaCallback>,
    pub from_ai: Vec<LuaCallback>,
    pub sound: Vec<LuaCallback>,
    pub edit_mode: Vec<LuaCallback>,
    pub map_crc: Vec<LuaCallback>,
    pub check_maps: Vec<LuaCallback>,
    pub try_spawn: Vec<LuaCallback>,
    pub gun_select: Vec<LuaCallback>,
    pub spawn_client: Vec<LuaCallback>,
    pub suicide: Vec<LuaCallback>,
    pub shoot: Vec<LuaCallback>,
    pub explode: Vec<LuaCallback>,
    pub item_pickup: Vec<LuaCallback>,
    pub text: Vec<LuaCallback>,
    pub say_team: Vec<LuaCallback>,
    pub switch_name: Vec<LuaCallback>,
    pub switch_model: Vec<LuaCallback>,
    pub switch_team: Vec<LuaCallback>,
    pub map_vote: Vec<LuaCallback>,
    pub item_list: Vec<LuaCallback>,
    pub edit_ent: Vec<LuaCallback>,
    pub edit_var: Vec<LuaCallback>,
    pub client_ping: Vec<LuaCallback>,
    pub master_mode: Vec<LuaCallback>,
    pub clear_bans: Vec<LuaCallback>,
    pub kick: Vec<LuaCallback>,
    pub spectator: Vec<LuaCallback>,
    pub set_team: Vec<LuaCallback>,
    pub force_intermission: Vec<LuaCallback>,
    pub record_demo: Vec<LuaCallback>,
    pub stop_demo: Vec<LuaCallback>,
    pub clear_demos: Vec<LuaCallback>,
    pub list_demos: Vec<LuaCallback>,
    pub get_demo: Vec<LuaCallback>,
    pub get_map: Vec<LuaCallback>,
    pub new_map: Vec<LuaCallback>,
    pub set_master: Vec<LuaCallback>,
    pub add_bot: Vec<LuaCallback>,
    pub del_bot: Vec<LuaCallback>,
    pub bot_limit: Vec<LuaCallback>,
    pub bot_balance: Vec<LuaCallback>,
    pub auth_try: Vec<LuaCallback>,
    pub auth_kick: Vec<LuaCallback>,
    pub pause_game: Vec<LuaCallback>,
    pub game_speed: Vec<LuaCallback>,
    pub copy: Vec<LuaCallback>,
    pub paste: Vec<LuaCallback>,
    pub clipboard: Vec<LuaCallback>,
    pub undo: Vec<LuaCallback>,
    pub redo: Vec<LuaCallback>,
    pub edit_face: Vec<LuaCallback>,
    pub edit_tex: Vec<LuaCallback>,
    pub edit_mat: Vec<LuaCallback>,
    pub flip: Vec<LuaCallback>,
    pub rotate: Vec<LuaCallback>,
    pub replace: Vec<LuaCallback>,
    pub del_cube: Vec<LuaCallback>,
    pub edit_v_slot: Vec<LuaCallback>,
    pub remip: Vec<LuaCallback>,
    pub serv_cmd: Vec<LuaCallback>,
    pub bases: Vec<LuaCallback>,
    pub rep_ammo: Vec<LuaCallback>,
    pub try_drop_flag: Vec<LuaCallback>,
    pub take_flag: Vec<LuaCallback>,
    pub init_flags: Vec<LuaCallback>,
    pub init_tokens: Vec<LuaCallback>,
    pub take_token: Vec<LuaCallback>,
    pub deposit_tokens: Vec<LuaCallback>,
    pub taunt: Vec<LuaCallback>,
}

pub(crate) struct LuaCallback {
    pub instance_idx: usize,
    pub callback: mlua::Function,
}

pub(crate) fn add_callbacks(
    _options: &LuaPluginOptions,
    luaplugin: &mut LuaPlugin,
    name: &str,
    lua: Lua,
) -> Result<(), mlua::Error> {
    let globals = lua.globals();

    let instance_idx = luaplugin.lua_instances.len();

    macro_rules! add_callback {
        ($name:ident) => {
            if globals.contains_key(stringify!($name))? {
                luaplugin.callbacks.$name.push(LuaCallback {
                    instance_idx,
                    callback: globals.get(stringify!($name))?,
                });
            }
        };
    }

    add_callback!(init);
    add_callback!(start);
    add_callback!(connected);
    add_callback!(disconnected);
    add_callback!(map);

    add_callback!(pos);
    add_callback!(teleport);
    add_callback!(jumppad);

    add_callback!(connect);
    add_callback!(auth_answer);
    add_callback!(ping);
    add_callback!(from_ai);
    add_callback!(sound);
    add_callback!(edit_mode);
    add_callback!(map_crc);
    add_callback!(check_maps);
    add_callback!(try_spawn);
    add_callback!(gun_select);
    add_callback!(spawn_client);
    add_callback!(suicide);
    add_callback!(shoot);
    add_callback!(explode);
    add_callback!(item_pickup);
    add_callback!(text);
    add_callback!(say_team);
    add_callback!(switch_name);
    add_callback!(switch_model);
    add_callback!(switch_team);
    add_callback!(map_vote);
    add_callback!(item_list);
    add_callback!(edit_ent);
    add_callback!(edit_var);
    add_callback!(client_ping);
    add_callback!(master_mode);
    add_callback!(clear_bans);
    add_callback!(kick);
    add_callback!(spectator);
    add_callback!(set_team);
    add_callback!(force_intermission);
    add_callback!(record_demo);
    add_callback!(stop_demo);
    add_callback!(clear_demos);
    add_callback!(list_demos);
    add_callback!(get_demo);
    add_callback!(get_map);
    add_callback!(new_map);
    add_callback!(set_master);
    add_callback!(add_bot);
    add_callback!(del_bot);
    add_callback!(bot_limit);
    add_callback!(bot_balance);
    add_callback!(auth_try);
    add_callback!(auth_kick);
    add_callback!(pause_game);
    add_callback!(game_speed);
    add_callback!(copy);
    add_callback!(paste);
    add_callback!(clipboard);
    add_callback!(undo);
    add_callback!(redo);
    add_callback!(edit_face);
    add_callback!(edit_tex);
    add_callback!(edit_mat);
    add_callback!(flip);
    add_callback!(rotate);
    add_callback!(replace);
    add_callback!(del_cube);
    add_callback!(edit_v_slot);
    add_callback!(remip);
    add_callback!(serv_cmd);
    add_callback!(bases);
    add_callback!(rep_ammo);
    add_callback!(try_drop_flag);
    add_callback!(take_flag);
    add_callback!(init_flags);
    add_callback!(init_tokens);
    add_callback!(take_token);
    add_callback!(deposit_tokens);
    add_callback!(taunt);

    luaplugin.lua_instances.push(Instance {
        lua,
        plugin_name: name.to_owned(),
    });

    Ok(())
}
