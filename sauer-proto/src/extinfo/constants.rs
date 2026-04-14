/// ExtInfo acknowledgement sentinel value.
pub const EXT_ACK: i32 = -1;
/// Current ExtInfo protocol version.
pub const EXT_VERSION: i32 = 105;
/// ExtInfo response code indicating success.
pub const EXT_NO_ERROR: i32 = 0;
/// ExtInfo response code indicating an error.
pub const EXT_ERROR: i32 = 1;
/// ExtInfo sub-response type carrying a list of connected client numbers.
pub const EXT_PLAYERSTATS_RESP_IDS: i32 = -10;
/// ExtInfo sub-response type carrying the full stats for a single player.
pub const EXT_PLAYERSTATS_RESP_STATS: i32 = -11;
/// ExtInfo query type: request the server uptime in seconds.
pub const EXT_UPTIME: i32 = 0;
/// ExtInfo query type: request player stats for all or a specific client.
pub const EXT_PLAYERSTATS: i32 = 1;
/// ExtInfo query type: request team scores.
pub const EXT_TEAMSCORE: i32 = 2;
