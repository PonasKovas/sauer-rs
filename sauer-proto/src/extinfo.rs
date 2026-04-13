//! ExtInfo protocol
//!
//! This is separate from the rest of the protocol and operates simply by sending messages to the UDP socket which is usually
//! on port `NORMAL_PORT+1`.

use crate::{
    ParseError,
    internal::{ReaderCore, WriterCore},
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
        let mut w = WriterCore::new();
        w.put_byte(0);

        match self {
            Self::Uptime => {
                w.put_int(EXT_UPTIME);
            }
            Self::PlayerStats { client_number } => {
                w.put_int(EXT_PLAYERSTATS);
                w.put_int(client_number.unwrap_or(-1));
            }
            Self::TeamScore => {
                w.put_int(EXT_TEAMSCORE);
            }
        }

        w.buffer
    }
    pub fn from_bytes(buf: &[u8]) -> Result<Self, ParseError> {
        let mut r = ReaderCore::new(buf);

        if r.get_int()? != 0 {
            return Err(ParseError::InvalidExtInfo);
        }

        let msg_type = r.get_int()?;

        Ok(match msg_type {
            EXT_UPTIME => Self::Uptime,
            EXT_PLAYERSTATS => {
                let cn = r.get_int()?;
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
        gunselect: i32,
        privilege: i32,
        state: i32,
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
        let mut w = WriterCore::new();
        w.put_byte(0);

        match self {
            Self::Uptime { seconds } => {
                w.put_int(EXT_UPTIME);
                w.put_int(EXT_ACK);
                w.put_int(EXT_VERSION);
                w.put_int(*seconds);
            }
            Self::PlayerStats { client_number, ok } => {
                w.put_int(EXT_PLAYERSTATS);
                w.put_int(client_number.unwrap_or(-1));
                w.put_int(EXT_ACK);
                w.put_int(EXT_VERSION);
                w.put_int(if *ok { EXT_NO_ERROR } else { EXT_ERROR });
            }
            Self::PlayerStatsIds { ids } => {
                w.put_int(EXT_PLAYERSTATS_RESP_IDS);
                for id in ids {
                    w.put_int(*id);
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
                w.put_int(EXT_PLAYERSTATS_RESP_STATS);
                w.put_int(*client_num);
                w.put_int(*ping);
                w.put_string(name);
                w.put_string(team);
                w.put_int(*frags);
                w.put_int(*flags);
                w.put_int(*deaths);
                w.put_int(*teamkills);
                w.put_int(*damage);
                w.put_int(*health);
                w.put_int(*armour);
                w.put_int(*gunselect);
                w.put_int(*privilege);
                w.put_int(*state);
                if let Some(ip) = ip {
                    w.put_bytes(ip);
                }
            }
            Self::TeamScore {
                game_mode,
                remaining_time,
                scores,
            } => {
                w.put_int(EXT_TEAMSCORE);
                w.put_int(EXT_ACK);
                w.put_int(EXT_VERSION);
                w.put_int(if scores.is_some() {
                    EXT_NO_ERROR
                } else {
                    EXT_ERROR
                });
                w.put_int(*game_mode);
                w.put_int(*remaining_time);

                if let Some(scores) = scores {
                    for score in scores {
                        w.put_string(&score.team_name);
                        w.put_int(score.score);
                    }
                    w.put_int(-1);
                }
            }
        }

        w.buffer
    }

    pub fn from_bytes(buf: &[u8]) -> Result<Self, ParseError> {
        let mut r = ReaderCore::new(buf);

        if r.get_int()? != 0 {
            return Err(ParseError::InvalidExtInfo);
        }

        let msg_type = r.get_int()?;

        Ok(match msg_type {
            EXT_UPTIME => {
                if r.get_int()? != EXT_ACK {
                    return Err(ParseError::InvalidExtInfo);
                }
                let version = r.get_int()?;
                if version != EXT_VERSION {
                    return Err(ParseError::ExtInfoVersionMismatch(version));
                }

                let seconds = r.get_int()?;
                Self::Uptime { seconds }
            }
            EXT_PLAYERSTATS => {
                let cn = r.get_int()?;

                if r.get_int()? != EXT_ACK {
                    return Err(ParseError::InvalidExtInfo);
                }
                let version = r.get_int()?;
                if version != EXT_VERSION {
                    return Err(ParseError::ExtInfoVersionMismatch(version));
                }

                let ok = r.get_int()? == EXT_NO_ERROR;

                Self::PlayerStats {
                    client_number: if cn < 0 { None } else { Some(cn) },
                    ok,
                }
            }
            EXT_PLAYERSTATS_RESP_IDS => {
                let mut ids = Vec::new();
                while r.remaining() > 0 {
                    ids.push(r.get_int()?);
                }

                Self::PlayerStatsIds { ids }
            }
            EXT_PLAYERSTATS_RESP_STATS => {
                let client_num = r.get_int()?;
                let ping = r.get_int()?;
                let name = r.get_string()?;
                let team = r.get_string()?;
                let frags = r.get_int()?;
                let flags = r.get_int()?;
                let deaths = r.get_int()?;
                let teamkills = r.get_int()?;
                let damage = r.get_int()?;
                let health = r.get_int()?;
                let armour = r.get_int()?;
                let gunselect = r.get_int()?;
                let privilege = r.get_int()?;
                let state = r.get_int()?;

                let ip = if r.remaining() > 0 {
                    Some([r.get_byte()?, r.get_byte()?, r.get_byte()?])
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
                    gunselect,
                    privilege,
                    state,
                    ip,
                }
            }
            EXT_TEAMSCORE => {
                if r.get_int()? != EXT_ACK {
                    return Err(ParseError::InvalidExtInfo);
                }
                let version = r.get_int()?;
                if version != EXT_VERSION {
                    return Err(ParseError::ExtInfoVersionMismatch(version));
                }

                let error = r.get_int()? != EXT_NO_ERROR;
                let game_mode = r.get_int()?;
                let remaining_time = r.get_int()?;

                let scores = if error {
                    None
                } else {
                    let mut scores = Vec::new();

                    while r.peek(1).unwrap_or(&[0xFF])[0] != 0xFF {
                        let team_name = r.get_string()?;
                        let score = r.get_int()?;

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
