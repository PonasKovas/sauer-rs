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

/// ExtInfo responses from the server (engine `fpsgame/extinfo.h`).
#[derive(Debug, Clone, PartialEq)]
pub enum ExtInfoResponse {
    Uptime {
        seconds: i32,
    },
    /// One packet of a player stats reply. Every packet repeats the header
    /// (the requested cn, or `None` for all players), and a reply is the ids
    /// packet followed by one stats packet per player, or a lone error.
    PlayerStats {
        client_number: Option<i32>,
        body: PlayerStatsBody,
    },
    TeamScore {
        game_mode: i32,
        remaining_time: i32,
        /// `None` when the mode has no teams.
        scores: Option<Vec<ScoreData>>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlayerStatsBody {
    /// The requested cn isn't on the server.
    Error,
    Ids(Vec<i32>),
    Stats(PlayerStats),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerStats {
    pub client_num: i32,
    pub ping: i32,
    pub name: CubeString,
    pub team: CubeString,
    pub frags: i32,
    pub flags: i32,
    pub deaths: i32,
    pub teamkills: i32,
    /// Accuracy in percent (damage dealt per damage shot).
    pub accuracy: i32,
    pub health: i32,
    pub armour: i32,
    pub gunselect: Gun,
    pub privilege: Privilege,
    pub state: ClientState,
    /// The first three bytes of the player's IPv4 address, or zeros.
    pub ip: [u8; 3],
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScoreData {
    pub team_name: CubeString,
    pub score: i32,
    /// Bases the team holds in capture modes, `None` in other modes.
    pub bases: Option<Vec<i32>>,
}

/// The ACK and version every ExtInfo reply has after the echoed request.
fn read_ack(r: &mut &[u8]) -> Result<(), ParseError> {
    if i32::sauer_read(r)? != EXT_ACK {
        return Err(ParseError::InvalidExtInfo);
    }
    match i32::sauer_read(r)? {
        EXT_VERSION => Ok(()),
        version => Err(ParseError::ExtInfoVersionMismatch(version)),
    }
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
            Self::PlayerStats {
                client_number,
                body,
            } => {
                EXT_PLAYERSTATS.sauer_write(w);
                client_number.unwrap_or(-1).sauer_write(w);
                EXT_ACK.sauer_write(w);
                EXT_VERSION.sauer_write(w);
                match body {
                    PlayerStatsBody::Error => EXT_ERROR.sauer_write(w),
                    PlayerStatsBody::Ids(ids) => {
                        EXT_NO_ERROR.sauer_write(w);
                        EXT_PLAYERSTATS_RESP_IDS.sauer_write(w);
                        for id in ids {
                            id.sauer_write(w);
                        }
                    }
                    PlayerStatsBody::Stats(p) => {
                        EXT_NO_ERROR.sauer_write(w);
                        EXT_PLAYERSTATS_RESP_STATS.sauer_write(w);
                        p.client_num.sauer_write(w);
                        p.ping.sauer_write(w);
                        p.name.sauer_write(w);
                        p.team.sauer_write(w);
                        p.frags.sauer_write(w);
                        p.flags.sauer_write(w);
                        p.deaths.sauer_write(w);
                        p.teamkills.sauer_write(w);
                        p.accuracy.sauer_write(w);
                        p.health.sauer_write(w);
                        p.armour.sauer_write(w);
                        p.gunselect.sauer_write(w);
                        p.privilege.sauer_write(w);
                        p.state.sauer_write(w);
                        w.extend_from_slice(&p.ip);
                    }
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
                for score in scores.iter().flatten() {
                    score.team_name.sauer_write(w);
                    score.score.sauer_write(w);
                    match &score.bases {
                        Some(bases) => {
                            (bases.len() as i32).sauer_write(w);
                            for b in bases {
                                b.sauer_write(w);
                            }
                        }
                        None => (-1).sauer_write(w),
                    }
                }
            }
        }
    }

    pub fn from_bytes(mut buf: &[u8]) -> Result<Self, ParseError> {
        let r = &mut buf;

        if i32::sauer_read(r)? != 0 {
            return Err(ParseError::InvalidExtInfo);
        }

        Ok(match i32::sauer_read(r)? {
            EXT_UPTIME => {
                read_ack(r)?;
                Self::Uptime {
                    seconds: i32::sauer_read(r)?,
                }
            }
            EXT_PLAYERSTATS => {
                let cn = i32::sauer_read(r)?;
                read_ack(r)?;
                let body = if i32::sauer_read(r)? != EXT_NO_ERROR {
                    PlayerStatsBody::Error
                } else {
                    match i32::sauer_read(r)? {
                        EXT_PLAYERSTATS_RESP_IDS => {
                            let mut ids = Vec::new();
                            while !r.is_empty() {
                                ids.push(i32::sauer_read(r)?);
                            }
                            PlayerStatsBody::Ids(ids)
                        }
                        EXT_PLAYERSTATS_RESP_STATS => PlayerStatsBody::Stats(PlayerStats {
                            client_num: i32::sauer_read(r)?,
                            ping: i32::sauer_read(r)?,
                            name: CubeString::sauer_read(r)?,
                            team: CubeString::sauer_read(r)?,
                            frags: i32::sauer_read(r)?,
                            flags: i32::sauer_read(r)?,
                            deaths: i32::sauer_read(r)?,
                            teamkills: i32::sauer_read(r)?,
                            accuracy: i32::sauer_read(r)?,
                            health: i32::sauer_read(r)?,
                            armour: i32::sauer_read(r)?,
                            gunselect: Gun::sauer_read(r)?,
                            privilege: Privilege::sauer_read(r)?,
                            state: ClientState::sauer_read(r)?,
                            ip: [u8::sauer_read(r)?, u8::sauer_read(r)?, u8::sauer_read(r)?],
                        }),
                        _ => return Err(ParseError::InvalidExtInfo),
                    }
                };
                Self::PlayerStats {
                    client_number: (cn >= 0).then_some(cn),
                    body,
                }
            }
            EXT_TEAMSCORE => {
                read_ack(r)?;
                let error = i32::sauer_read(r)? != EXT_NO_ERROR;
                let game_mode = i32::sauer_read(r)?;
                let remaining_time = i32::sauer_read(r)?;
                let scores = if error {
                    None
                } else {
                    let mut scores = Vec::new();
                    while !r.is_empty() {
                        let team_name = CubeString::sauer_read(r)?;
                        let score = i32::sauer_read(r)?;
                        let bases = match i32::sauer_read(r)? {
                            n if n < 0 => None,
                            n => Some(
                                (0..n)
                                    .map(|_| i32::sauer_read(r))
                                    .collect::<Result<_, _>>()?,
                            ),
                        };
                        scores.push(ScoreData {
                            team_name,
                            score,
                            bases,
                        });
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
