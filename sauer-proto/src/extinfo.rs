//! ExtInfo protocol
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

mod constants;

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
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Vec::new();
        w.push(0);

        match self {
            Self::Uptime => {
                EXT_UPTIME.sauer_write(&mut w);
            }
            Self::PlayerStats { client_number } => {
                EXT_PLAYERSTATS.sauer_write(&mut w);
                client_number.unwrap_or(-1).sauer_write(&mut w);
            }
            Self::TeamScore => {
                EXT_TEAMSCORE.sauer_write(&mut w);
            }
        }

        w
    }
    pub fn from_bytes(mut buf: &[u8]) -> Result<Self, ParseError> {
        let r = &mut buf;

        if i32::sauer_read(r)? != 0 {
            return Err(ParseError::InvalidExtInfo);
        }

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
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Vec::new();
        w.push(0);

        match self {
            Self::Uptime { seconds } => {
                EXT_UPTIME.sauer_write(&mut w);
                EXT_ACK.sauer_write(&mut w);
                EXT_VERSION.sauer_write(&mut w);
                seconds.sauer_write(&mut w);
            }
            Self::PlayerStats { client_number, ok } => {
                EXT_PLAYERSTATS.sauer_write(&mut w);
                client_number.unwrap_or(-1).sauer_write(&mut w);
                EXT_ACK.sauer_write(&mut w);
                EXT_VERSION.sauer_write(&mut w);
                if *ok { EXT_NO_ERROR } else { EXT_ERROR }.sauer_write(&mut w);
            }
            Self::PlayerStatsIds { ids } => {
                EXT_PLAYERSTATS_RESP_IDS.sauer_write(&mut w);
                for id in ids {
                    id.sauer_write(&mut w);
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
                EXT_PLAYERSTATS_RESP_STATS.sauer_write(&mut w);
                client_num.sauer_write(&mut w);
                ping.sauer_write(&mut w);
                name.sauer_write(&mut w);
                team.sauer_write(&mut w);
                frags.sauer_write(&mut w);
                flags.sauer_write(&mut w);
                deaths.sauer_write(&mut w);
                teamkills.sauer_write(&mut w);
                damage.sauer_write(&mut w);
                health.sauer_write(&mut w);
                armour.sauer_write(&mut w);
                (*gunselect as i32).sauer_write(&mut w);
                (*privilege as i32).sauer_write(&mut w);
                (*state as i32).sauer_write(&mut w);
                if let Some(ip) = ip {
                    w.extend_from_slice(ip);
                }
            }
            Self::TeamScore {
                game_mode,
                remaining_time,
                scores,
            } => {
                EXT_TEAMSCORE.sauer_write(&mut w);
                EXT_ACK.sauer_write(&mut w);
                EXT_VERSION.sauer_write(&mut w);
                if scores.is_some() {
                    EXT_NO_ERROR
                } else {
                    EXT_ERROR
                }
                .sauer_write(&mut w);
                game_mode.sauer_write(&mut w);
                remaining_time.sauer_write(&mut w);

                if let Some(scores) = scores {
                    for score in scores {
                        score.team_name.sauer_write(&mut w);
                        score.score.sauer_write(&mut w);
                    }
                    (-1).sauer_write(&mut w);
                }
            }
        }

        w
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
