use super::*;
use crate::{
    data::EditVarValue,
    internal::{constants::*, EncodeCore, WriterCore},
    route::{Chan1, C2S},
    Encode,
};

// =====================================================================
// Individual struct Encode impls for C2S<Chan1>
// =====================================================================

impl Encode<C2S<Chan1>> for Connect {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_CONNECT);
        w.put_string(&self.name);
        w.put_int(self.player_model as i32);
        w.put_string(&self.password_hash);
        w.put_string(&self.auth_desc);
        w.put_string(&self.auth_name);
    }
}

impl Encode<C2S<Chan1>> for AuthAnswer {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_AUTHANS);
        w.put_string(&self.desc);
        w.put_int(self.id as i32);
        w.put_string(&self.answer);
    }
}

impl Encode<C2S<Chan1>> for Ping {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_PING);
        w.put_int(self.time);
    }
}

impl Encode<C2S<Chan1>> for FromAi {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_FROMAI);
        w.put_int(self.qcn);
    }
}

impl Encode<C2S<Chan1>> for EditModeClient {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_EDITMODE);
        w.put_bool(self.val);
    }
}

impl Encode<C2S<Chan1>> for MapCrc {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_MAPCRC);
        w.put_string(&self.map_name);
        w.put_int(self.crc);
    }
}

impl Encode<C2S<Chan1>> for CheckMaps {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_CHECKMAPS);
    }
}

impl Encode<C2S<Chan1>> for TrySpawn {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_TRYSPAWN);
    }
}

impl Encode<C2S<Chan1>> for GunSelect {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_GUNSELECT);
        w.put_int(self.gun as i32);
    }
}

impl Encode<C2S<Chan1>> for SpawnClient {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_SPAWN);
        w.put_int(self.life_sequence);
        w.put_int(self.gun_select as i32);
    }
}

impl Encode<C2S<Chan1>> for Suicide {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_SUICIDE);
    }
}

impl Encode<C2S<Chan1>> for Shoot {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_SHOOT);
        w.put_int(self.id);
        w.put_int(self.gun as i32);
        w.put_int((&self.from.x * DMF) as i32);
        w.put_int((&self.from.y * DMF) as i32);
        w.put_int((&self.from.z * DMF) as i32);
        w.put_int((&self.to.x * DMF) as i32);
        w.put_int((&self.to.y * DMF) as i32);
        w.put_int((&self.to.z * DMF) as i32);
        w.put_int(self.hits.len() as i32);
        for hit in &self.hits {
            EncodeCore::encode(hit, w);
        }
    }
}

impl Encode<C2S<Chan1>> for Explode {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_EXPLODE);
        w.put_int(self.millis);
        w.put_int(self.gun as i32);
        w.put_int(self.id);
        w.put_int(self.hits.len() as i32);
        for hit in &self.hits {
            EncodeCore::encode(hit, w);
        }
    }
}

impl Encode<C2S<Chan1>> for ItemPickup {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_ITEMPICKUP);
        w.put_int(self.item_index);
    }
}

impl Encode<C2S<Chan1>> for Text {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_TEXT);
        w.put_string(&self.text);
    }
}

impl Encode<C2S<Chan1>> for SayTeamClient {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_SAYTEAM);
        w.put_string(&self.text);
    }
}

impl Encode<C2S<Chan1>> for SwitchName {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_SWITCHNAME);
        w.put_string(&self.name);
    }
}

impl Encode<C2S<Chan1>> for SwitchModel {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_SWITCHMODEL);
        w.put_int(self.model as i32);
    }
}

impl Encode<C2S<Chan1>> for SwitchTeam {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_SWITCHTEAM);
        w.put_string(&self.team);
    }
}

impl Encode<C2S<Chan1>> for MapVote {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_MAPVOTE);
        w.put_string(&self.map_name);
        w.put_int(self.mode as i32);
    }
}

impl Encode<C2S<Chan1>> for ItemList {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_ITEMLIST);
        for item in &self.items {
            w.put_int(item.item_index);
            w.put_int(item.item_type as i32);
        }
        w.put_int(-1);
    }
}

impl Encode<C2S<Chan1>> for EditEntClient {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_EDITENT);
        w.put_int(self.ent_id);
        w.put_int(self.x);
        w.put_int(self.y);
        w.put_int(self.z);
        w.put_int(self.type_ as i32);
        w.put_int(self.attr1);
        w.put_int(self.attr2);
        w.put_int(self.attr3);
        w.put_int(self.attr4);
        w.put_int(self.attr5);
    }
}

impl Encode<C2S<Chan1>> for EditVar {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_EDITVAR);
        match &self.val {
            EditVarValue::Int(i) => {
                w.put_int(0);
                w.put_string(&self.name);
                w.put_int(*i);
            }
            EditVarValue::Float(f) => {
                w.put_int(1);
                w.put_string(&self.name);
                w.put_float(*f);
            }
            EditVarValue::String(s) => {
                w.put_int(2);
                w.put_string(&self.name);
                w.put_string(s);
            }
        }
    }
}

impl Encode<C2S<Chan1>> for ClientPing {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_CLIENTPING);
        w.put_int(self.ping);
    }
}

impl Encode<C2S<Chan1>> for MasterModeClient {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_MASTERMODE);
        w.put_int(self.mode as i32);
    }
}

impl Encode<C2S<Chan1>> for ClearBans {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_CLEARBANS);
    }
}

impl Encode<C2S<Chan1>> for Kick {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_KICK);
        w.put_int(self.victim_cn);
        w.put_string(&self.reason);
    }
}

impl Encode<C2S<Chan1>> for SpectatorClient {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_SPECTATOR);
        w.put_int(self.spectator_cn);
        w.put_bool(self.val);
    }
}

impl Encode<C2S<Chan1>> for SetTeamClient {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_SETTEAM);
        w.put_int(self.client_num);
        w.put_string(&self.team);
    }
}

impl Encode<C2S<Chan1>> for ForceIntermission {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_FORCEINTERMISSION);
    }
}

impl Encode<C2S<Chan1>> for RecordDemo {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_RECORDDEMO);
        w.put_bool(self.val);
    }
}

impl Encode<C2S<Chan1>> for StopDemo {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_STOPDEMO);
    }
}

impl Encode<C2S<Chan1>> for ClearDemos {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_CLEARDEMOS);
        w.put_int(self.demo_id);
    }
}

impl Encode<C2S<Chan1>> for ListDemos {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_LISTDEMOS);
    }
}

impl Encode<C2S<Chan1>> for GetDemo {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_GETDEMO);
        w.put_int(self.demo_id);
        w.put_int(self.tag);
    }
}

impl Encode<C2S<Chan1>> for GetMap {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_GETMAP);
    }
}

impl Encode<C2S<Chan1>> for NewMap {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_NEWMAP);
        w.put_int(self.size);
    }
}

impl Encode<C2S<Chan1>> for SetMaster {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_SETMASTER);
        w.put_int(self.client_num);
        w.put_bool(self.val);
        w.put_string(&self.hash);
    }
}

impl Encode<C2S<Chan1>> for AddBot {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_ADDBOT);
        w.put_int(self.skill);
    }
}

impl Encode<C2S<Chan1>> for DelBot {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_DELBOT);
    }
}

impl Encode<C2S<Chan1>> for BotLimit {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_BOTLIMIT);
        w.put_int(self.limit);
    }
}

impl Encode<C2S<Chan1>> for BotBalance {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_BOTBALANCE);
        w.put_bool(self.balance);
    }
}

impl Encode<C2S<Chan1>> for AuthTry {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_AUTHTRY);
        w.put_string(&self.desc);
        w.put_string(&self.name);
    }
}

impl Encode<C2S<Chan1>> for AuthKick {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_AUTHKICK);
        w.put_string(&self.desc);
        w.put_string(&self.name);
        w.put_int(self.victim_cn);
        w.put_string(&self.reason);
    }
}

impl Encode<C2S<Chan1>> for PauseGameClient {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_PAUSEGAME);
        w.put_bool(self.val);
    }
}

impl Encode<C2S<Chan1>> for GameSpeedClient {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_GAMESPEED);
        w.put_int(self.speed);
    }
}

impl Encode<C2S<Chan1>> for CopyClient {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_COPY);
    }
}

impl Encode<C2S<Chan1>> for PasteClient {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_PASTE);
    }
}

impl Encode<C2S<Chan1>> for ClipboardClient {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_CLIPBOARD);
        w.put_int(self.unpack_len);
        w.put_int(self.pack_len);
        w.put_bytes(&self.data);
    }
}

impl Encode<C2S<Chan1>> for UndoClient {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_UNDO);
        w.put_int(self.unpack_len);
        w.put_int(self.pack_len);
        w.put_bytes(&self.data);
    }
}

impl Encode<C2S<Chan1>> for RedoClient {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_REDO);
        w.put_int(self.unpack_len);
        w.put_int(self.pack_len);
        w.put_bytes(&self.data);
    }
}

impl Encode<C2S<Chan1>> for EditFace {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_EDITF);
        EncodeCore::encode(&self.sel, w);
        w.put_int(self.dir);
        w.put_int(self.mode as i32);
    }
}

impl Encode<C2S<Chan1>> for EditTex {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_EDITT);
        EncodeCore::encode(&self.sel, w);
        w.put_int(self.tex);
        w.put_bool(self.all_faces);
        let extra_len = self.extra_data.len() as u16;
        w.put_byte((extra_len & 0xFF) as u8);
        w.put_byte((extra_len >> 8) as u8);
        w.put_bytes(&self.extra_data);
    }
}

impl Encode<C2S<Chan1>> for EditMat {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_EDITM);
        EncodeCore::encode(&self.sel, w);
        w.put_int(self.mat);
        w.put_int(self.filter);
    }
}

impl Encode<C2S<Chan1>> for Flip {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_FLIP);
        EncodeCore::encode(&self.sel, w);
    }
}

impl Encode<C2S<Chan1>> for Rotate {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_ROTATE);
        EncodeCore::encode(&self.sel, w);
        w.put_int(self.dir);
    }
}

impl Encode<C2S<Chan1>> for Replace {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_REPLACE);
        EncodeCore::encode(&self.sel, w);
        w.put_int(self.old_tex);
        w.put_int(self.new_tex);
        w.put_int(self.in_sel);
        let extra_len = self.extra_data.len() as u16;
        w.put_byte((extra_len & 0xFF) as u8);
        w.put_byte((extra_len >> 8) as u8);
        w.put_bytes(&self.extra_data);
    }
}

impl Encode<C2S<Chan1>> for DelCube {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_DELCUBE);
        EncodeCore::encode(&self.sel, w);
    }
}

impl Encode<C2S<Chan1>> for EditVSlot {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_EDITVSLOT);
        EncodeCore::encode(&self.sel, w);
        w.put_int(self.delta);
        w.put_bool(self.all_faces);
        let extra_len = self.extra_data.len() as u16;
        w.put_byte((extra_len & 0xFF) as u8);
        w.put_byte((extra_len >> 8) as u8);
        w.put_bytes(&self.extra_data);
    }
}

impl Encode<C2S<Chan1>> for Remip {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_REMIP);
    }
}

impl Encode<C2S<Chan1>> for ServCmd {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_SERVCMD);
        w.put_string(&self.cmd);
    }
}

impl Encode<C2S<Chan1>> for TryDropFlag {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_TRYDROPFLAG);
    }
}

impl Encode<C2S<Chan1>> for TakeFlagClient {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_TAKEFLAG);
        w.put_int(self.flag);
        w.put_int(self.version);
    }
}

impl Encode<C2S<Chan1>> for InitFlagsClient {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_INITFLAGS);
        w.put_int(self.flags.len() as i32);
        for f in &self.flags {
            w.put_int(f.team);
            w.put_int((f.pos.x * DMF) as i32);
            w.put_int((f.pos.y * DMF) as i32);
            w.put_int((f.pos.z * DMF) as i32);
        }
    }
}

impl Encode<C2S<Chan1>> for BasesClient {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_BASES);
        w.put_int(self.bases.len() as i32);
        for b in &self.bases {
            w.put_int(b.ammotype.map_or(0, |t| t as i32));
            w.put_int((b.pos.x * DMF) as i32);
            w.put_int((b.pos.y * DMF) as i32);
            w.put_int((b.pos.z * DMF) as i32);
        }
    }
}

impl Encode<C2S<Chan1>> for RepAmmoClient {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_REPAMMO);
    }
}

impl Encode<C2S<Chan1>> for InitTokensClient {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_INITTOKENS);
        w.put_int(self.bases.len() as i32);
        for b in &self.bases {
            w.put_int(b.team);
            w.put_int((b.pos.x * DMF) as i32);
            w.put_int((b.pos.y * DMF) as i32);
            w.put_int((b.pos.z * DMF) as i32);
        }
    }
}

impl Encode<C2S<Chan1>> for TakeTokenClient {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_TAKETOKEN);
        w.put_int(self.id);
    }
}

impl Encode<C2S<Chan1>> for DepositTokensClient {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_DEPOSITTOKENS);
        w.put_int(self.base);
    }
}

impl Encode<C2S<Chan1>> for Sound {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_SOUND);
        w.put_int(self.sound_id);
    }
}

// =====================================================================
// Chan1Client enum Encode (delegates to individual impls)
// =====================================================================

impl Encode<C2S<Chan1>> for Chan1Client {
    fn encode(&self, w: &mut WriterCore) {
        match self {
            Chan1Client::Connect(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::AuthAnswer(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::Ping(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::FromAi(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::Sound(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::EditMode(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::MapCrc(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::CheckMaps(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::TrySpawn(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::GunSelect(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::Spawn(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::Suicide(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::Shoot(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::Explode(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::ItemPickup(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::Text(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::SayTeam(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::SwitchName(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::SwitchModel(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::SwitchTeam(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::MapVote(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::ItemList(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::EditEnt(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::EditVar(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::ClientPing(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::MasterMode(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::ClearBans(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::Kick(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::Spectator(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::SetTeam(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::ForceIntermission(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::RecordDemo(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::StopDemo(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::ClearDemos(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::ListDemos(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::GetDemo(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::GetMap(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::NewMap(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::SetMaster(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::AddBot(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::DelBot(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::BotLimit(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::BotBalance(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::AuthTry(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::AuthKick(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::PauseGame(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::GameSpeed(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::Copy(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::Paste(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::Clipboard(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::Undo(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::Redo(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::EditFace(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::EditTex(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::EditMat(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::Flip(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::Rotate(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::Replace(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::DelCube(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::EditVSlot(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::Remip(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::ServCmd(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::Bases(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::RepAmmo(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::TryDropFlag(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::TakeFlag(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::InitFlags(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::InitTokens(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::TakeToken(v) => Encode::<C2S<Chan1>>::encode(v, w),
            Chan1Client::DepositTokens(v) => Encode::<C2S<Chan1>>::encode(v, w),
        }
    }
}
