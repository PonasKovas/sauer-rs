/// Divisor used when converting fixed-point position coordinates to floating-point.
pub const DMF: f32 = 16.0;

/// `N_CONNECT` — Client sends its name, model, password hash, and auth credentials to join.
pub const N_CONNECT: i32 = 0;
/// `N_SERVINFO` — Server sends initial connection information (client number, protocol, session ID, etc.).
pub const N_SERVINFO: i32 = 1;
/// `N_WELCOME` — Server signals that the client is fully welcomed into the game.
pub const N_WELCOME: i32 = 2;
/// `N_INITCLIENT` — Server announces a newly connected client to all peers.
pub const N_INITCLIENT: i32 = 3;
/// `N_POS` — Constant physics/position update (channel 0).
pub const N_POS: i32 = 4;
/// `N_TEXT` — Global chat message.
pub const N_TEXT: i32 = 5;
/// `N_SOUND` — Plays a sound at the sending client's location.
pub const N_SOUND: i32 = 6;
/// `N_CDIS` — Client disconnected notification.
pub const N_CDIS: i32 = 7;
/// `N_SHOOT` — Client fired a weapon.
pub const N_SHOOT: i32 = 8;
/// `N_EXPLODE` — Projectile explosion triggered by the client.
pub const N_EXPLODE: i32 = 9;
/// `N_SUICIDE` — Client forcefully kills themselves.
pub const N_SUICIDE: i32 = 10;
/// `N_DIED` — A player was killed.
pub const N_DIED: i32 = 11;
/// `N_DAMAGE` — Damage applied to a target client.
pub const N_DAMAGE: i32 = 12;
/// `N_HITPUSH` — Physical knockback from a weapon hit.
pub const N_HITPUSH: i32 = 13;
/// `N_SHOTFX` — Weapon firing effect replication.
pub const N_SHOTFX: i32 = 14;
/// `N_EXPLODEFX` — Projectile explosion effect replication.
pub const N_EXPLODEFX: i32 = 15;
/// `N_TRYSPAWN` — Client requests to spawn.
pub const N_TRYSPAWN: i32 = 16;
/// `N_SPAWNSTATE` — Server confirms spawn state (health, ammo, etc.) after a spawn.
pub const N_SPAWNSTATE: i32 = 17;
/// `N_SPAWN` — Instructs a client to spawn in the game.
pub const N_SPAWN: i32 = 18;
/// `N_FORCEDEATH` — Forces a client to die instantly.
pub const N_FORCEDEATH: i32 = 19;
/// `N_GUNSELECT` — Player changed their weapon selection.
pub const N_GUNSELECT: i32 = 20;
/// `N_TAUNT` — Player is taunting.
pub const N_TAUNT: i32 = 21;
/// `N_MAPCHANGE` — Server instructs all clients to load a new map and game mode.
pub const N_MAPCHANGE: i32 = 22;
/// `N_MAPVOTE` — Client votes for a specific map and mode.
pub const N_MAPVOTE: i32 = 23;
/// `N_TEAMINFO` — Transmits team frag scores.
pub const N_TEAMINFO: i32 = 24;
/// `N_ITEMSPAWN` — Notifies that an item has spawned on the map.
pub const N_ITEMSPAWN: i32 = 25;
/// `N_ITEMPICKUP` — Client requests to pick up an item.
pub const N_ITEMPICKUP: i32 = 26;
/// `N_ITEMACC` — Server acknowledges an item pickup.
pub const N_ITEMACC: i32 = 27;
/// `N_TELEPORT` — Player teleported.
pub const N_TELEPORT: i32 = 28;
/// `N_JUMPPAD` — Player hit a jump pad.
pub const N_JUMPPAD: i32 = 29;
/// `N_PING` — Round-trip time measurement request from the client.
pub const N_PING: i32 = 30;
/// `N_PONG` — Round-trip time measurement reply from the server.
pub const N_PONG: i32 = 31;
/// `N_CLIENTPING` — Scoreboard ping update for a specific client.
pub const N_CLIENTPING: i32 = 32;
/// `N_TIMEUP` — Current match time remaining has been updated.
pub const N_TIMEUP: i32 = 33;
/// `N_FORCEINTERMISSION` — Requests or triggers the end of the current match.
pub const N_FORCEINTERMISSION: i32 = 34;
/// `N_SERVMSG` — Direct server/system message displayed in the client console.
pub const N_SERVMSG: i32 = 35;
/// `N_ITEMLIST` — Bulk item spawn list (locations and types) for the current map.
pub const N_ITEMLIST: i32 = 36;
/// `N_RESUME` — Sends the full state of all current players to a late-joining client.
pub const N_RESUME: i32 = 37;
/// `N_EDITMODE` — Player entered or left cooperative edit mode.
pub const N_EDITMODE: i32 = 38;
/// `N_EDITENT` — Coop Edit: map entity modification.
pub const N_EDITENT: i32 = 39;
/// `N_EDITF` — Coop Edit: face modification.
pub const N_EDITF: i32 = 40;
/// `N_EDITT` — Coop Edit: texture modification.
pub const N_EDITT: i32 = 41;
/// `N_EDITM` — Coop Edit: material modification.
pub const N_EDITM: i32 = 42;
/// `N_FLIP` — Coop Edit: flip geometry.
pub const N_FLIP: i32 = 43;
/// `N_COPY` — Coop Edit: copy geometry selection to clipboard.
pub const N_COPY: i32 = 44;
/// `N_PASTE` — Coop Edit: paste geometry from clipboard.
pub const N_PASTE: i32 = 45;
/// `N_ROTATE` — Coop Edit: rotate geometry.
pub const N_ROTATE: i32 = 46;
/// `N_REPLACE` — Coop Edit: replace texture.
pub const N_REPLACE: i32 = 47;
/// `N_DELCUBE` — Coop Edit: delete a cube.
pub const N_DELCUBE: i32 = 48;
/// `N_REMIP` — Coop Edit: recalculate lighting and mip maps.
pub const N_REMIP: i32 = 49;
/// `N_EDITVSLOT` — Coop Edit: modify texture V-Slots.
pub const N_EDITVSLOT: i32 = 50;
/// `N_UNDO` — Coop Edit: geometry undo data.
pub const N_UNDO: i32 = 51;
/// `N_REDO` — Coop Edit: geometry redo data.
pub const N_REDO: i32 = 52;
/// `N_NEWMAP` — Instructs clients to generate a new empty map of the specified size.
pub const N_NEWMAP: i32 = 53;
/// `N_GETMAP` — Client requests to download the current map from the server.
pub const N_GETMAP: i32 = 54;
/// `N_SENDMAP` — Server streams a cooperative edit map file to the client (channel 2).
pub const N_SENDMAP: i32 = 55;
/// `N_CLIPBOARD` — Pushes raw map clipboard geometry data.
pub const N_CLIPBOARD: i32 = 56;
/// `N_EDITVAR` — Coop Edit: a map variable was overridden.
pub const N_EDITVAR: i32 = 57;
/// `N_MASTERMODE` — Gets or sets the global master mode.
pub const N_MASTERMODE: i32 = 58;
/// `N_KICK` — Requests to kick a player from the server.
pub const N_KICK: i32 = 59;
/// `N_CLEARBANS` — Requests to clear all active bans.
pub const N_CLEARBANS: i32 = 60;
/// `N_CURRENTMASTER` — Notifies clients of the current master mode and privileged clients.
pub const N_CURRENTMASTER: i32 = 61;
/// `N_SPECTATOR` — Forces a player into or out of spectator mode.
pub const N_SPECTATOR: i32 = 62;
/// `N_SETMASTER` — Requests to set master privileges using a password hash.
pub const N_SETMASTER: i32 = 63;
/// `N_SETTEAM` — Moves a player to a specific team.
pub const N_SETTEAM: i32 = 64;
/// `N_BASES` — Sends the list of capture bases for the current map (Capture / Regen Capture).
pub const N_BASES: i32 = 65;
/// `N_BASEINFO` — Updates the state of a single capture base.
pub const N_BASEINFO: i32 = 66;
/// `N_BASESCORE` — Updates the score for a team on a capture base.
pub const N_BASESCORE: i32 = 67;
/// `N_REPAMMO` — Replenishes a player's ammo (Regen Capture).
pub const N_REPAMMO: i32 = 68;
/// `N_BASEREGEN` — Regenerates a player's health and armour at a capture base.
pub const N_BASEREGEN: i32 = 69;
/// `N_ANNOUNCE` — Server announces an important item spawn (e.g. Quad Damage).
pub const N_ANNOUNCE: i32 = 70;
/// `N_LISTDEMOS` — Client requests the list of available demos on the server.
pub const N_LISTDEMOS: i32 = 71;
/// `N_SENDDEMOLIST` — Server replies with the list of available demos.
pub const N_SENDDEMOLIST: i32 = 72;
/// `N_GETDEMO` — Client requests to download a specific demo.
pub const N_GETDEMO: i32 = 73;
/// `N_SENDDEMO` — Server streams a recorded demo file to the client (channel 2).
pub const N_SENDDEMO: i32 = 74;
/// `N_DEMOPLAYBACK` — Notifies the client that the server started or stopped demo playback.
pub const N_DEMOPLAYBACK: i32 = 75;
/// `N_RECORDDEMO` — Toggles automatic demo recording for the next match.
pub const N_RECORDDEMO: i32 = 76;
/// `N_STOPDEMO` — Stops the current demo recording.
pub const N_STOPDEMO: i32 = 77;
/// `N_CLEARDEMOS` — Clears a specific demo or all demos from server memory.
pub const N_CLEARDEMOS: i32 = 78;
/// `N_TAKEFLAG` — A player picked up a flag (CTF / Protect / Hold).
pub const N_TAKEFLAG: i32 = 79;
/// `N_RETURNFLAG` — A player returned a flag to its base.
pub const N_RETURNFLAG: i32 = 80;
/// `N_RESETFLAG` — A flag was reset to its spawn location.
pub const N_RESETFLAG: i32 = 81;
/// `N_INVISFLAG` — Marks a flag as invisible or visible.
pub const N_INVISFLAG: i32 = 82;
/// `N_TRYDROPFLAG` — Client attempts to drop the flag they are carrying.
pub const N_TRYDROPFLAG: i32 = 83;
/// `N_DROPFLAG` — A player dropped a flag.
pub const N_DROPFLAG: i32 = 84;
/// `N_SCOREFLAG` — A player scored with a flag.
pub const N_SCOREFLAG: i32 = 85;
/// `N_INITFLAGS` — Sends initial flag states to a joining client (CTF modes).
pub const N_INITFLAGS: i32 = 86;
/// `N_SAYTEAM` — Team-only chat message.
pub const N_SAYTEAM: i32 = 87;
/// `N_CLIENT` — Envelope wrapper that prefixes messages with a client number and length.
pub const N_CLIENT: i32 = 88;
/// `N_AUTHTRY` — Client initiates a cryptographic authentication attempt.
pub const N_AUTHTRY: i32 = 89;
/// `N_AUTHKICK` — Client requests a kick via cryptographic auth verification.
pub const N_AUTHKICK: i32 = 90;
/// `N_AUTHCHAL` — Server sends a cryptographic challenge to the client.
pub const N_AUTHCHAL: i32 = 91;
/// `N_AUTHANS` — Client answers the server's cryptographic challenge.
pub const N_AUTHANS: i32 = 92;
/// `N_REQAUTH` — Server initiates an authentication request.
pub const N_REQAUTH: i32 = 93;
/// `N_PAUSEGAME` — Sets or clears the paused state of the match.
pub const N_PAUSEGAME: i32 = 94;
/// `N_GAMESPEED` — Modifies the global game speed multiplier.
pub const N_GAMESPEED: i32 = 95;
/// `N_ADDBOT` — Requests to spawn an AI bot with a given skill level.
pub const N_ADDBOT: i32 = 96;
/// `N_DELBOT` — Requests to remove an AI bot.
pub const N_DELBOT: i32 = 97;
/// `N_INITAI` — Server spawns an AI bot into the client's game world.
pub const N_INITAI: i32 = 98;
/// `N_FROMAI` — Redirects subsequent messages to act on behalf of an AI bot.
pub const N_FROMAI: i32 = 99;
/// `N_BOTLIMIT` — Sets the maximum number of bots allowed on the server.
pub const N_BOTLIMIT: i32 = 100;
/// `N_BOTBALANCE` — Sets the bot auto-balance flag.
pub const N_BOTBALANCE: i32 = 101;
/// `N_MAPCRC` — Client verifies the CRC checksum of the map they have loaded.
pub const N_MAPCRC: i32 = 102;
/// `N_CHECKMAPS` — Forces a manual CRC check of all clients' loaded maps.
pub const N_CHECKMAPS: i32 = 103;
/// `N_SWITCHNAME` — A player changed their display name.
pub const N_SWITCHNAME: i32 = 104;
/// `N_SWITCHMODEL` — A player changed their player model.
pub const N_SWITCHMODEL: i32 = 105;
/// `N_SWITCHTEAM` — Client requests to switch to a different team.
pub const N_SWITCHTEAM: i32 = 106;
/// `N_INITTOKENS` — Sends initial token states to a joining client (Collect mode).
pub const N_INITTOKENS: i32 = 107;
/// `N_TAKETOKEN` — A player collected a token.
pub const N_TAKETOKEN: i32 = 108;
/// `N_EXPIRETOKENS` — Notifies that specific tokens have expired.
pub const N_EXPIRETOKENS: i32 = 109;
/// `N_DROPTOKENS` — A player dropped their held tokens.
pub const N_DROPTOKENS: i32 = 110;
/// `N_DEPOSITTOKENS` — A player deposited tokens at a base.
pub const N_DEPOSITTOKENS: i32 = 111;
/// `N_STEALTOKENS` — A player stole tokens from the enemy team.
pub const N_STEALTOKENS: i32 = 112;
/// `N_SERVCMD` — Remote server command payload.
pub const N_SERVCMD: i32 = 113;
/// `N_DEMOPACKET` — Demo playback marker injected from a recorded `.dmo` file.
pub const N_DEMOPACKET: i32 = 114;
