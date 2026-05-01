//! Server Query protocol (Server Info & ExtInfo)
//!
//! This is separate from the rest of the protocol and operates simply by sending messages to the UDP socket which is usually
//! on port `NORMAL_PORT+1`.

use crate::{
    ParseError,
    data::{ClientState, Gun, Privilege},
    internal::{SauerRead, SauerWrite},
};
use constants::*;
use cube_string::CubeString;

pub mod constants;

#[derive(Debug, Clone, PartialEq)]
pub enum ServerQueryRequest {
    ServerInfo { ping_id: i32 },
    ExtInfo(ExtInfoRequest),
}

impl ServerQueryRequest {
    pub fn parse(buf: &[u8]) -> Result<Self, ParseError> {
        let mut r = buf;
        if r.is_empty() {
            return Err(ParseError::UnexpectedEof { expected: 1 });
        }
        let ping_id = i32::sauer_read(&mut r)?;
        if ping_id == 0 && !r.is_empty() {
            Ok(Self::ExtInfo(ExtInfoRequest::from_reader(&mut r)?))
        } else {
            Ok(Self::ServerInfo { ping_id })
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ServerQueryResponse {
    ServerInfo {
        ping_id: i32,
        num_clients: i32,
        protocol_version: i32,
        game_mode: i32,
        remaining_time: i32,
        max_clients: i32,
        master_mode: i32,
        is_paused: bool,
        game_speed: i32,
        map_name: CubeString,
        server_desc: CubeString,
    },
    ExtInfo(ExtInfoResponse),
}

impl ServerQueryResponse {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Vec::new();
        match self {
            Self::ServerInfo {
                ping_id,
                num_clients,
                protocol_version,
                game_mode,
                remaining_time,
                max_clients,
                master_mode,
                is_paused,
                game_speed,
                map_name,
                server_desc,
            } => {
                ping_id.sauer_write(&mut w);
                num_clients.sauer_write(&mut w);

                let attrs = if *is_paused || *game_speed != 100 {
                    7
                } else {
                    5
                };
                attrs.sauer_write(&mut w);

                protocol_version.sauer_write(&mut w);
                game_mode.sauer_write(&mut w);
                remaining_time.sauer_write(&mut w);
                max_clients.sauer_write(&mut w);
                master_mode.sauer_write(&mut w);

                if attrs == 7 {
                    (if *is_paused { 1i32 } else { 0i32 }).sauer_write(&mut w);
                    game_speed.sauer_write(&mut w);
                }

                map_name.sauer_write(&mut w);
                server_desc.sauer_write(&mut w);
            }
            Self::ExtInfo(ext_resp) => {
                0i32.sauer_write(&mut w);
                ext_resp.write_to(&mut w);
            }
        }
        w
    }
}

/// ExtInfo requests that the client may send to a server
#[derive(Debug, Clone, PartialEq)]
pub enum ExtInfoRequest {
    /// Requests the server's uptime in seconds
    Uptime,
    /// Requests the stats for all or a specific player
    PlayerStats { client_number: Option<i32> },
    /// Requests the team scores
    TeamScore,
}

impl ExtInfoRequest {
    pub(crate) fn from_reader(r: &mut &[u8]) -> Result<Self, ParseError> {
        let msg_type = i32::sauer_read(r)?;

        Ok(match msg_type {
            EXT_UPTIME => Self::Uptime,
            EXT_PLAYERSTATS => {
                let cn = i32::sauer_read(r)?;
                Self::PlayerStats {
                    client_number: if cn < 0 { None } else { Some(cn) },
                }
            }
            EXT_TEAMSCORE => Self::TeamScore,
            other => return Err(ParseError::InvalidExtInfoType(other)),
        })
    }
}

/// ExtInfo responses from the server
#[derive(Debug, Clone, PartialEq)]
pub enum ExtInfoResponse {
    Uptime {
        seconds: i32,
    },
    PlayerStats {
        client_number: Option<i32>,
        ok: bool,
    },
    PlayerStatsIds {
        ids: Vec<i32>,
    },
    PlayerStatsStats {
        client_num: i32,
        ping: i32,
        name: CubeString,
        team: CubeString,
        frags: i32,
        flags: i32,
        deaths: i32,
        teamkills: i32,
        damage: i32,
        health: i32,
        armour: i32,
        gunselect: Gun,
        privilege: Privilege,
        state: ClientState,
        ip: Option<[u8; 3]>,
    },
    TeamScore {
        game_mode: i32,
        remaining_time: i32,
        scores: Option<Vec<ScoreData>>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScoreData {
    pub team_name: CubeString,
    pub score: i32,
}

impl ExtInfoResponse {
    pub(crate) fn write_to(&self, w: &mut Vec<u8>) {
        match self {
            Self::Uptime { seconds } => {
                EXT_UPTIME.sauer_write(w);
                EXT_ACK.sauer_write(w);
                EXT_VERSION.sauer_write(w);
                seconds.sauer_write(w);
            }
            Self::PlayerStats { client_number, ok } => {
                EXT_PLAYERSTATS.sauer_write(w);
                client_number.unwrap_or(-1).sauer_write(w);
                EXT_ACK.sauer_write(w);
                EXT_VERSION.sauer_write(w);
                if *ok { EXT_NO_ERROR } else { EXT_ERROR }.sauer_write(w);
            }
            Self::PlayerStatsIds { ids } => {
                EXT_PLAYERSTATS_RESP_IDS.sauer_write(w);
                for id in ids {
                    id.sauer_write(w);
                }
            }
            Self::PlayerStatsStats {
                client_num,
                ping,
                name,
                team,
                frags,
                flags,
                deaths,
                teamkills,
                damage,
                health,
                armour,
                gunselect,
                privilege,
                state,
                ip,
            } => {
                EXT_PLAYERSTATS_RESP_STATS.sauer_write(w);
                client_num.sauer_write(w);
                ping.sauer_write(w);
                name.sauer_write(w);
                team.sauer_write(w);
                frags.sauer_write(w);
                flags.sauer_write(w);
                deaths.sauer_write(w);
                teamkills.sauer_write(w);
                damage.sauer_write(w);
                health.sauer_write(w);
                armour.sauer_write(w);
                (*gunselect as i32).sauer_write(w);
                (*privilege as i32).sauer_write(w);
                (*state as i32).sauer_write(w);
                if let Some(ip) = ip {
                    w.extend_from_slice(ip);
                }
            }
            Self::TeamScore {
                game_mode,
                remaining_time,
                scores,
            } => {
                EXT_TEAMSCORE.sauer_write(w);
                EXT_ACK.sauer_write(w);
                EXT_VERSION.sauer_write(w);
                if scores.is_some() {
                    EXT_NO_ERROR
                } else {
                    EXT_ERROR
                }
                .sauer_write(w);
                game_mode.sauer_write(w);
                remaining_time.sauer_write(w);

                if let Some(scores) = scores {
                    for score in scores {
                        score.team_name.sauer_write(w);
                        score.score.sauer_write(w);
                    }
                    (-1).sauer_write(w);
                }
            }
        }
    }

    pub fn from_bytes(mut buf: &[u8]) -> Result<Self, ParseError> {
        let r = &mut buf;

        if i32::sauer_read(r)? != 0 {
            return Err(ParseError::InvalidExtInfo);
        }

        let msg_type = i32::sauer_read(r)?;

        Ok(match msg_type {
            EXT_UPTIME => {
                if i32::sauer_read(r)? != EXT_ACK {
                    return Err(ParseError::InvalidExtInfo);
                }
                let version = i32::sauer_read(r)?;
                if version != EXT_VERSION {
                    return Err(ParseError::ExtInfoVersionMismatch(version));
                }

                let seconds = i32::sauer_read(r)?;
                Self::Uptime { seconds }
            }
            EXT_PLAYERSTATS => {
                let cn = i32::sauer_read(r)?;

                if i32::sauer_read(r)? != EXT_ACK {
                    return Err(ParseError::InvalidExtInfo);
                }
                let version = i32::sauer_read(r)?;
                if version != EXT_VERSION {
                    return Err(ParseError::ExtInfoVersionMismatch(version));
                }

                let ok = i32::sauer_read(r)? == EXT_NO_ERROR;

                Self::PlayerStats {
                    client_number: if cn < 0 { None } else { Some(cn) },
                    ok,
                }
            }
            EXT_PLAYERSTATS_RESP_IDS => {
                let mut ids = Vec::new();
                while r.len() > 0 {
                    ids.push(i32::sauer_read(r)?);
                }

                Self::PlayerStatsIds { ids }
            }
            EXT_PLAYERSTATS_RESP_STATS => {
                let client_num = i32::sauer_read(r)?;
                let ping = i32::sauer_read(r)?;
                let name = CubeString::sauer_read(r)?;
                let team = CubeString::sauer_read(r)?;
                let frags = i32::sauer_read(r)?;
                let flags = i32::sauer_read(r)?;
                let deaths = i32::sauer_read(r)?;
                let teamkills = i32::sauer_read(r)?;
                let damage = i32::sauer_read(r)?;
                let health = i32::sauer_read(r)?;
                let armour = i32::sauer_read(r)?;
                let gunselect_val = i32::sauer_read(r)?;
                let privilege_val = i32::sauer_read(r)?;
                let state_val = i32::sauer_read(r)?;

                let ip = if r.len() > 0 {
                    Some([u8::sauer_read(r)?, u8::sauer_read(r)?, u8::sauer_read(r)?])
                } else {
                    None
                };

                Self::PlayerStatsStats {
                    client_num,
                    ping,
                    name,
                    team,
                    frags,
                    flags,
                    deaths,
                    teamkills,
                    damage,
                    health,
                    armour,
                    gunselect: Gun::try_from(gunselect_val).map_err(|_| {
                        ParseError::InvalidEnumValue {
                            value: gunselect_val,
                            enum_name: "Gun",
                        }
                    })?,
                    privilege: Privilege::try_from(privilege_val).map_err(|_| {
                        ParseError::InvalidEnumValue {
                            value: privilege_val,
                            enum_name: "Privilege",
                        }
                    })?,
                    state: ClientState::try_from(state_val).map_err(|_| {
                        ParseError::InvalidEnumValue {
                            value: state_val,
                            enum_name: "ClientState",
                        }
                    })?,
                    ip,
                }
            }
            EXT_TEAMSCORE => {
                if i32::sauer_read(r)? != EXT_ACK {
                    return Err(ParseError::InvalidExtInfo);
                }
                let version = i32::sauer_read(r)?;
                if version != EXT_VERSION {
                    return Err(ParseError::ExtInfoVersionMismatch(version));
                }

                let error = i32::sauer_read(r)? != EXT_NO_ERROR;
                let game_mode = i32::sauer_read(r)?;
                let remaining_time = i32::sauer_read(r)?;

                let scores = if error {
                    None
                } else {
                    let mut scores = Vec::new();

                    while r.get(1).copied().unwrap_or(0xFF) != 0xFF {
                        let team_name = CubeString::sauer_read(r)?;
                        let score = i32::sauer_read(r)?;

                        scores.push(ScoreData { team_name, score });
                    }

                    Some(scores)
                };

                Self::TeamScore {
                    game_mode,
                    remaining_time,
                    scores,
                }
            }
            other => return Err(ParseError::InvalidExtInfoType(other)),
        })
    }
}
