# sauer-proto Protocol Reference

> Auto-generated concise reference. Source: `sauer-proto/src/`.

## Core API

### `Encode<R>` trait
Serialize a packet onto a wire-format buffer for route `R`.  
Do **not** call directly — use `Writer::push` instead.  
Key methods: `encode(&self, out: &mut Vec<u8>)`, `is_reliable(&self) -> bool`,
`requires_client_wrapper(&self) -> Option<i32>` (returns `Some(cn)` if packet must be wrapped in `N_CLIENT`).

### `Decode<R>` trait
Deserialize a packet from wire-format bytes for route `R`.  
Do **not** call directly — use `Reader::next` instead.  
Key method: `decode(reader: &mut &[u8], client_number: Option<i32>, session: &mut Session) -> Result<Self, ParseError>`.

### `Reader<'a, R>`
Stateful deserializer bound to protocol route `R`.  
Transparently handles `N_CLIENT` envelope unwrapping.  
- `Reader::new(slice: &[u8], session: &mut Session) -> Self` — must use a consistent `Session` per connection.
- `next<T: Decode<R>>(&mut self) -> Option<Result<T, ParseError>>` — decode next message; returns `None` at end-of-buffer.
- `is_empty(&self) -> bool`

### `Writer<R>`
Sink-based serializer bound to protocol route `R`.  
Transparently batches `N_CLIENT` envelopes.  
- `Writer::new() -> Self`
- `push<T: Encode<R>>(&mut self, msg: &T)` — serialize and append a packet; no need to wrap in the channel enum.
- `consume(self) -> (Vec<u8>, SendMode)` — finalize, returns raw bytes and reliability mode.

### `Session`
Per-connection parsing state. Keep one `Session` per connection for the entire lifetime.  
- `is_hold_mode: bool` — set automatically on `N_MAPCHANGE`; controls `N_INITFLAGS` parsing (M_HOLD gamemode includes extra hold-team data).

### `SendMode`
Returned by `Writer::consume`. Variants: `Reliable`, `Unreliable` (sequenced).  
If any packet pushed to a writer is reliable, the whole ENet packet is reliable.

### `PacketSequence<R>` trait
Lets slices, arrays, `Vec`, and tuples of packet structs be pushed to a `Writer<R>` via `push_to`.  
Blanket-implemented for homogeneous collections and heterogeneous tuples up to 16 elements.

### `Client<T>`
Wrapper produced when decoding server channel-1 packets that arrive inside an `N_CLIENT` envelope.  
- `client_num: i32` — the originating client.
- `packet: T` — the inner packet.

## Route Markers (`sauer_proto::route`)

The generic parameter on `Reader`/`Writer` and `Encode`/`Decode` encodes direction + channel.

| Route | Channel | Direction |
|-------|---------|-----------|
| `S2C<Chan0>` | 0 — positions | Server → Client |
| `C2S<Chan0>` | 0 — positions | Client → Server |
| `S2C<Chan1>` | 1 — game messages | Server → Client |
| `C2S<Chan1>` | 1 — game messages | Client → Server |
| `S2C<Chan2>` | 2 — file transfer | Server → Client |

`Chan0`, `Chan1`, `Chan2` are zero-sized channel markers. `S2C<C>` and `C2S<C>` are direction markers.

## Data Types (`sauer_proto::data`)

Helper types used as fields in packet structs. Not packets themselves.

### Enums

### `SetTeamReason` (repr `i32`)

- `Switched` = `0`
- `Forced` = `1`

### `Gun` (repr `i32`)

- `Fist` = `0`
- `Sg` = `1`
- `Cg` = `2`
- `Rl` = `3`
- `Rifle` = `4`
- `Gl` = `5`
- `Pistol` = `6`

### `ItemType` (repr `i32`)

- `Shells` = `8`
- `Bullets` = `9`
- `Rockets` = `10`
- `Rounds` = `11`
- `Grenades` = `12`
- `Cartridges` = `13`
- `Health` = `14`
- `Boost` = `15`
- `GreenArmour` = `16`
- `YellowArmour` = `17`
- `Quad` = `18`

### `MasterMode` (repr `i32`)

- `Open` = `0`
- `Veto` = `1`
- `Locked` = `2`
- `Private` = `3`
- `Password` = `4`
- `Auth` = `5`

### `PlayerModel` (repr `i32`)

- `MrFixit` = `0`
- `Snoutx10k` = `1`
- `Ogro2` = `2`
- `Inky` = `3`
- `CaptainCannon` = `4`

### `GameMode` (repr `i32`)
Game mode identifier, including local-only modes (negative values).

- `Sp` = `-3`
- `Dmsp` = `-2`
- `Demo` = `-1`
- `Ffa` = `0`
- `CoopEdit` = `1`
- `TeamPlay` = `2`
- `Instagib` = `3`
- `InstaTeam` = `4`
- `Efficiency` = `5`
- `EfficTeam` = `6`
- `Tactics` = `7`
- `TacTeam` = `8`
- `Capture` = `9`
- `RegenCapture` = `10`
- `Ctf` = `11`
- `InstaCTF` = `12`
- `Protect` = `13`
- `InstaProtect` = `14`
- `Hold` = `15`
- `InstaHold` = `16`
- `EfficCTF` = `17`
- `EfficProtect` = `18`
- `EfficHold` = `19`
- `Collect` = `20`
- `InstaCollect` = `21`
- `EfficCollect` = `22`

### `AiType` (repr `i32`)
Server-side AI bot type.

- `None` = `0`
- `Bot` = `1`

### `ArmourType` (repr `i32`)
Player armour type, determining damage reduction percentage.

- `Blue` = `0` — Blue armour: 20% damage reduction.
- `Green` = `1` — Green armour: 40% damage reduction.
- `Yellow` = `2` — Yellow armour: 60% damage reduction.

### `PhysState` (repr `u8`)
Physical movement/contact state of an entity.

- `Float` = `0`
- `Fall` = `1`
- `Slide` = `2`
- `Slope` = `3`
- `Floor` = `4`
- `StepUp` = `5`
- `StepDown` = `6`
- `Bounce` = `7`

### `Orient` (repr `i32`)
Cube face orientation used in cooperative editing (O_LEFT through O_TOP).

- `Left` = `0`
- `Right` = `1`
- `Back` = `2`
- `Front` = `3`
- `Bottom` = `4`
- `Top` = `5`

### `EntityType` (repr `i32`)
Map entity type.

- `Empty` = `0`
- `Light` = `1`
- `MapModel` = `2`
- `PlayerStart` = `3`
- `EnvMap` = `4`
- `Particles` = `5`
- `MapSound` = `6`
- `SpotLight` = `7`
- `IShells` = `8`
- `IBullets` = `9`
- `IRockets` = `10`
- `IRounds` = `11`
- `IGrenades` = `12`
- `ICartridges` = `13`
- `IHealth` = `14`
- `IBoost` = `15`
- `IGreenArmour` = `16`
- `IYellowArmour` = `17`
- `IQuad` = `18`
- `Teleport` = `19`
- `TeleDest` = `20`
- `Monster` = `21`
- `Carrot` = `22`
- `JumpPad` = `23`
- `Base` = `24`
- `RespawnPoint` = `25`
- `Box` = `26`
- `Barrel` = `27`
- `Platform` = `28`
- `Elevator` = `29`
- `Flag` = `30`

### `EditFaceMode` (repr `i32`)
Edit face operation mode.

- `Default` = `0` — Push or pull cube edges.
- `Fill` = `1` — Fill the selected region.
- `Corner` = `2` — Corner edit mode.

### `ClientState` (repr `i32`)

- `Alive` = `0`
- `Dead` = `1`
- `Spawning` = `2`
- `Lagged` = `3`
- `Editing` = `4`
- `Spectator` = `5`

### `Privilege` (repr `i32`)

- `None` = `0`
- `Auth` = `1`
- `Master` = `2`
- `Admin` = `3`

### `Movement` (repr `i8`)

- `Back` = `-1`
- `None` = `0`
- `Forward` = `1`

### `Strafe` (repr `i8`)

- `Left` = `-1`
- `None` = `0`
- `Right` = `1`

### Structs

### `ClientResumeState`
State for a client resuming a game (e.g., late join).

- `client_num: i32`
- `state: ClientState`
- `frags: i32`
- `flags: i32`
- `deaths: i32`
- `quad_millis: i32`
- `player_state: PlayerState`

### `ClientTokenState`

- `client_num: i32`
- `tokens: i32`

### `DroppedToken`

- `id: i32`
- `team: i32`
- `yaw: i32`

### `ItemSpawn`
Information to represent a spawned item in the world.
  
*Encode+Decode:* `ItemSpawn` on `S2C<Chan1>`

- `item_index: i32`
- `item_type: ItemType`

### `MasterClient`
Represents a master/admin client configuration broadcast to everyone.

- `client_num: i32`
- `privilege: Privilege`

### `PlayerAmmo`
Ammo counts for each weapon slot.

- `shotgun: i32`
- `chaingun: i32`
- `rocket_launcher: i32`
- `rifle: i32`
- `grenade_launcher: i32`
- `pistol: i32`

### `PlayerState`
Represents the game state of a player/client.

- `life_sequence: i32`
- `health: i32`
- `max_health: i32`
- `armour: i32`
- `armour_type: ArmourType`
- `gun_select: Gun`
- `ammo: PlayerAmmo`

### `SelInfo`
Cube selection information for cooperative editing geometry modifications.

- `o: Vec3<i32>`
- `s: Vec3<i32>`
- `grid: i32`
- `orient: Orient`
- `cx: i32`
- `cxs: i32`
- `cy: i32`
- `cys: i32`
- `corner: i32`

### `HitInfo`
Contains ray/hit information from a weapon shot or explosion.

- `target_cn: i32`
- `life_sequence: i32`
- `dist: f32`
- `rays: i32`
- `dir: Vec3<f32>`

### `StolenToken`

- `id: i32`

### `TeamInfo`
Stores the number of frags a particular team has.
  
*Encode+Decode:* `TeamInfo` on `S2C<Chan1>`

- `team: CubeString`
- `frags: i32`

### `TokenBaseSpawn`

- `team: i32`
- `pos: Vec3<f32>`

### `TokenState`

- `id: i32`
- `team: i32`
- `yaw: i32`
- `pos: Vec3<f32>`

### `FlagSpawn`

- `team: i32`
- `pos: Vec3<f32>`

### `data::Vec3<T>`
3D vector. Generic over element type (usually `f32` or `i32`).
- `x: T`, `y: T`, `z: T`
  
Floating-point positions on the wire are quantized: multiplied by `DMF` (1/16 world units) before encoding.

### `data::Direction`
Packed yaw+pitch direction. Wire: 2 bytes, `yaw + biased_pitch * 360`.
- `yaw: u16` — 0–359 degrees
- `pitch: i8` — −90 to +90 degrees

### `data::EditVarValue`
Strict typing for `N_EDITVAR` variable values. Discriminant encoded as `i32` before name+value.
- `Int(i32)` — discriminant 0
- `Float(f32)` — discriminant 1
- `String(CubeString)` — discriminant 2

### `data::BaseInfo`
State of a single capture base (used in `N_BASES` server packet).
- `ammotype: Option<ItemType>` — `None` encodes as 0
- `owner: CubeString`
- `enemy: CubeString`
- `converted: bool`
- `ammo: i32`

### `data::BaseSpawn`
Spawn position of a capture base (used in `N_BASES` client packet).
- `ammotype: Option<ItemType>`
- `pos: Vec3<f32>`

### `data::FlagState`
Full state of a CTF flag, used in `N_INITFLAGS`. Complex conditional encoding.
- `version: i32`
- `spawn_index: i32`
- `owner: FlagStateOwner`
- `invis: bool`
- `hold: Option<FlagStateHoldTeam>` — **must be `Some` only when `Session::is_hold_mode` is true**; stream corrupts otherwise.

### `data::FlagStateOwner`
- `Owned { owner: i32 }` — encoded as `owner_id >= 0`
- `Dropped { dropped: bool, drop_loc: Vec3<f32> }` — encoded as `owner_id = -1`; `drop_loc` only present when `dropped = true`

### `data::FlagStateHoldTeam`
- `None` — encoded as team = -1
- `Team { team: i32, hold_time: i32 }`

---

## Channel 0 Packets (`sauer_proto::channel0`)

High-frequency physics / movement updates. Most packets are *unreliable* (sequenced).  
Enum types: `Chan0Server` for `S2C<Chan0>`, `Chan0Client` for `C2S<Chan0>`.

### `Teleport` (`N_TELEPORT`)
`N_TELEPORT` Sent to immediately teleport a player.
  
*Encode+Decode:* `Teleport` on `S2C<Chan0>` · `Teleport` on `C2S<Chan0>`

- `client_num: i32`
- `tp: i32`
- `td: i32`

### `JumpPad` (`N_JUMPPAD`)
`N_JUMPPAD` Sent to apply a jump-pad acceleration.
  
*Encode+Decode:* `JumpPad` on `S2C<Chan0>` · `JumpPad` on `C2S<Chan0>`

- `client_num: i32`
- `jp: i32`

### `DemoPacket` (`N_DEMOPACKET`)
`N_DEMOPACKET` Demo playback marker (server only).
  
*Encode+Decode:* `DemoPacket` on `S2C<Chan0>` · `DemoPacket` on `S2C<Chan1>` · `DemoPacket` on `S2C<Chan2>`
  
*(no fields)*

### `Position`
`N_POS` Constant physical position updates.
  
*Encode+Decode:* `Position` on `S2C<Chan0>` *(unreliable)* · `Position` on `C2S<Chan0>` *(unreliable)*

- `client_num: u32`
- `phys_state: PhysState`
- `life_seq_parity: bool`
- `move_dir: Movement`
- `strafe: Strafe`
- `touches_gameclip: bool`
- `pos: Vec3<f32>`
- `dir: Direction`
- `roll: i8`
- `vel_speed: u16`
- `vel_dir: Direction`
- `fall_mag: u16`
- `fall_dir: Direction`

---

## Channel 1 Packets (`sauer_proto::channel1`)

Reliable game-state messages, chat, events (default reliable unless noted).  
Enum types: `Chan1Server` for `S2C<Chan1>`, `Chan1Client` for `C2S<Chan1>`.  
Packets with `Client<T>` routing arrive inside an `N_CLIENT` envelope on the server→client direction — the `client_num` field identifies the originating player.

### `Sound` (`N_SOUND`)
`N_SOUND` Plays a sound.
  
*Encode+Decode:* `Client<Sound>` on `S2C<Chan1>` *(unreliable)* · `Sound` on `C2S<Chan1>`

- `sound_id: i32`

### `Text` (`N_TEXT`)
`N_TEXT` Emits a chat message.
  
*Encode+Decode:* `Client<Text>` on `S2C<Chan1>` · `Text` on `C2S<Chan1>`

- `text: CubeString`

### `SwitchName` (`N_SWITCHNAME`)
`N_SWITCHNAME` Requests/Informs name change.
  
*Encode+Decode:* `Client<SwitchName>` on `S2C<Chan1>` · `SwitchName` on `C2S<Chan1>`

- `name: CubeString`

### `SwitchModel` (`N_SWITCHMODEL`)
`N_SWITCHMODEL` Requests/Informs model change.
  
*Encode+Decode:* `Client<SwitchModel>` on `S2C<Chan1>` · `SwitchModel` on `C2S<Chan1>`

- `model: PlayerModel`

### `GunSelect` (`N_GUNSELECT`)
`N_GUNSELECT` Acknowledges or indicates a player changed their current weapon selection.
  
*Encode+Decode:* `Client<GunSelect>` on `S2C<Chan1>` · `GunSelect` on `C2S<Chan1>`

- `gun: Gun`

### `ClientPing` (`N_CLIENTPING`)
`N_CLIENTPING` Reports/Updates the scoreboard ping value.
  
*Encode+Decode:* `Client<ClientPing>` on `S2C<Chan1>` *(unreliable)* · `ClientPing` on `C2S<Chan1>`

- `ping: i32`

### `EditFace` (`N_EDITF`)
`N_EDITF` Coop Edit: Face Modification.
  
*Encode+Decode:* `Client<EditFace>` on `S2C<Chan1>` · `EditFace` on `C2S<Chan1>`

- `sel: SelInfo`
- `dir: i32`
- `mode: EditFaceMode`

### `EditTex` (`N_EDITT`)
`N_EDITT` Coop Edit: Texture Modification.
  
*Encode+Decode:* `Client<EditTex>` on `S2C<Chan1>` · `EditTex` on `C2S<Chan1>`

- `sel: SelInfo`
- `tex: i32`
- `all_faces: bool`
- `extra_data: Vec<u8>`

### `EditMat` (`N_EDITM`)
`N_EDITM` Coop Edit: Material Modification.
  
*Encode+Decode:* `Client<EditMat>` on `S2C<Chan1>` · `EditMat` on `C2S<Chan1>`

- `sel: SelInfo`
- `mat: i32`
- `filter: i32`

### `Flip` (`N_FLIP`)
`N_FLIP` Coop Edit: Flip geometry.
  
*Encode+Decode:* `Client<Flip>` on `S2C<Chan1>` · `Flip` on `C2S<Chan1>`

- `sel: SelInfo`

### `Rotate` (`N_ROTATE`)
`N_ROTATE` Coop Edit: Rotate geometry.
  
*Encode+Decode:* `Client<Rotate>` on `S2C<Chan1>` · `Rotate` on `C2S<Chan1>`

- `sel: SelInfo`
- `dir: i32`

### `Replace` (`N_REPLACE`)
`N_REPLACE` Coop Edit: Replace texture.
  
*Encode+Decode:* `Client<Replace>` on `S2C<Chan1>` · `Replace` on `C2S<Chan1>`

- `sel: SelInfo`
- `old_tex: i32`
- `new_tex: i32`
- `in_sel: i32`
- `extra_data: Vec<u8>`

### `DelCube` (`N_DELCUBE`)
`N_DELCUBE` Coop Edit: Delete a cube.
  
*Encode+Decode:* `Client<DelCube>` on `S2C<Chan1>` · `DelCube` on `C2S<Chan1>`

- `sel: SelInfo`

### `EditVSlot` (`N_EDITVSLOT`)
`N_EDITVSLOT` Coop Edit: Modify texture V-Slots.
  
*Encode+Decode:* `Client<EditVSlot>` on `S2C<Chan1>` · `EditVSlot` on `C2S<Chan1>`

- `sel: SelInfo`
- `delta: i32`
- `all_faces: bool`
- `extra_data: Vec<u8>`

### `Remip` (`N_REMIP`)
`N_REMIP` Coop Edit: Lighting/mips recalculation.
  
*Encode+Decode:* `Client<Remip>` on `S2C<Chan1>` · `Remip` on `C2S<Chan1>`
  
*(no fields)*

### `NewMap` (`N_NEWMAP`)
`N_NEWMAP` Generates a new empty map of the specified size.
  
*Encode+Decode:* `NewMap` on `S2C<Chan1>` · `NewMap` on `C2S<Chan1>`

- `size: i32`

### `ItemList` (`N_ITEMLIST`)
`N_ITEMLIST` Bulk sends the items available on a map (synchronization).
  
*Encode+Decode:* `ItemList` on `S2C<Chan1>` · `ItemList` on `C2S<Chan1>`

- `items: Vec<data::ItemSpawn>`

### `ServCmd` (`N_SERVCMD`)
`N_SERVCMD` Raw server command.
  
*Encode+Decode:* `ServCmd` on `S2C<Chan1>` · `ServCmd` on `C2S<Chan1>`

- `cmd: CubeString`

### `EditEnt` (`N_EDITENT`)
`N_EDITENT` Coop Edit: Modification of map entities.
  
*Encode+Decode:* `Client<EditEnt>` on `S2C<Chan1>` · `EditEnt` on `C2S<Chan1>`

- `ent_id: i32`
- `pos: Vec3<f32>`
- `type_: EntityType`
- `attrs: [i32; 5]`

### `EditMode` (`N_EDITMODE`)
`N_EDITMODE` Notifies that a player entered or left Edit Mode.
  
*Encode+Decode:* `Client<EditMode>` on `S2C<Chan1>` · `EditMode` on `C2S<Chan1>`

- `is_editing: bool`

### `DemoPacket` (`N_DEMOPACKET`)
`N_DEMOPACKET` Demo playback marker.
  
*Encode+Decode:* `DemoPacket` on `S2C<Chan0>` · `DemoPacket` on `S2C<Chan1>` · `DemoPacket` on `S2C<Chan2>`
  
*(no fields)*

### `ServInfo` (`N_SERVINFO`)
`N_SERVINFO` Initial server information sent when a client starts connecting.
  
*Encode+Decode:* `ServInfo` on `S2C<Chan1>`

- `client_num: i32`
- `protocol: i32`
- `session_id: i32`
- `has_password: bool`
- `server_desc: CubeString`
- `server_auth: CubeString`

### `Welcome` (`N_WELCOME`)
`N_WELCOME` The client is fully welcomed and joined into the game.
  
*Encode+Decode:* `Welcome` on `S2C<Chan1>`
  
*(no fields)*

### `PauseGameServer` (`N_PAUSEGAME`)
`N_PAUSEGAME` Sets or removes the paused state of the game.
  
*Encode+Decode:* `PauseGameServer` on `S2C<Chan1>`

- `is_paused: bool`
- `client_num: i32`

### `GameSpeedServer` (`N_GAMESPEED`)
`N_GAMESPEED` Modifies the speed of the game.
  
*Encode+Decode:* `GameSpeedServer` on `S2C<Chan1>`

- `speed: i32`
- `client_num: i32`

### `SayTeamServer` (`N_SAYTEAM`)
`N_SAYTEAM` Emits a team chat message.
  
*Encode+Decode:* `SayTeamServer` on `S2C<Chan1>`

- `client_num: i32`
- `text: CubeString`

### `MapChange` (`N_MAPCHANGE`)
`N_MAPCHANGE` Notifies clients to change the current map and gamemode.
  
*Encode+Decode:* `MapChange` on `S2C<Chan1>`

- `map_name: CubeString`
- `mode: GameMode`
- `has_items: bool`

### `ForceDeath` (`N_FORCEDEATH`)
`N_FORCEDEATH` Forces a client to die instantly.
  
*Encode+Decode:* `ForceDeath` on `S2C<Chan1>`

- `client_num: i32`

### `InitClient` (`N_INITCLIENT`)
`N_INITCLIENT` A new client has connected.
  
*Encode+Decode:* `InitClient` on `S2C<Chan1>`

- `client_num: i32`
- `name: CubeString`
- `team: CubeString`
- `player_model: PlayerModel`

### `ClientDisconnected` (`N_CDIS`)
`N_CDIS` A client has fully disconnected.
  
*Encode+Decode:* `ClientDisconnected` on `S2C<Chan1>`

- `client_num: i32`

### `SpawnState` (`N_SPAWNSTATE`)
`N_SPAWNSTATE` Confirms the specific state (health, ammo, etc.) of a client who just spawned.
  
*Encode+Decode:* `SpawnState` on `S2C<Chan1>`

- `client_num: i32`
- `state: PlayerState`

### `ShotFx` (`N_SHOTFX`)
`N_SHOTFX` Replicates weapon firing effects across clients.
  
*Encode+Decode:* `ShotFx` on `S2C<Chan1>`

- `client_num: i32`
- `gun: Gun`
- `id: i32`
- `from: Vec3<f32>`
- `to: Vec3<f32>`

### `ExplodeFx` (`N_EXPLODEFX`)
`N_EXPLODEFX` Replicates projectile explosion effects across clients.
  
*Encode+Decode:* `ExplodeFx` on `S2C<Chan1>`

- `client_num: i32`
- `gun: Gun`
- `id: i32`

### `Damage` (`N_DAMAGE`)
`N_DAMAGE` Applies damage to a target client.
  
*Encode+Decode:* `Damage` on `S2C<Chan1>`

- `target_cn: i32`
- `actor_cn: i32`
- `damage: i32`
- `armour: i32`
- `health: i32`

### `HitPush` (`N_HITPUSH`)
`N_HITPUSH` Applies physical knockback from weapon hits to a client.
  
*Encode+Decode:* `HitPush` on `S2C<Chan1>`

- `target_cn: i32`
- `gun: Gun`
- `damage: i32`
- `dir: Vec3<f32>`

### `Died` (`N_DIED`)
`N_DIED` Notifies that a player was killed by an actor.
  
*Encode+Decode:* `Died` on `S2C<Chan1>`

- `victim_cn: i32`
- `actor_cn: i32`
- `actor_frags: i32`
- `team_frags: i32`

### `TeamInfo` (`N_TEAMINFO`)
`N_TEAMINFO` Transmits team scores (frags).
  
*Encode+Decode:* `TeamInfo` on `S2C<Chan1>`

- `teams: Vec<data::TeamInfo>`

### `Resume` (`N_RESUME`)
`N_RESUME` Sent during a late join to tell the connecting client about the state of all players.
  
*Encode+Decode:* `Resume` on `S2C<Chan1>`

- `clients: Vec<ClientResumeState>`

### `ItemSpawn` (`N_ITEMSPAWN`)
`N_ITEMSPAWN` Notifies that an item has spawned on the map.
  
*Encode+Decode:* `ItemSpawn` on `S2C<Chan1>`

- `item_index: i32`

### `ItemAck` (`N_ITEMACC`)
`N_ITEMACC` Acknowledges an item pickup.
  
*Encode+Decode:* `ItemAck` on `S2C<Chan1>`

- `item_index: i32`
- `client_num: i32`

### `ClipboardServer` (`N_CLIPBOARD`)
`N_CLIPBOARD` Pushes map clipboard data (for cooperative editing) to the client.
  
*Encode+Decode:* `ClipboardServer` on `S2C<Chan1>`

- `client_num: i32`
- `unpack_len: i32`
- `data: Vec<u8>`

### `UndoServer` (`N_UNDO`)
`N_UNDO` Sends geometry undo data to the client.
  
*Encode+Decode:* `UndoServer` on `S2C<Chan1>`

- `client_num: i32`
- `unpack_len: i32`
- `data: Vec<u8>`

### `RedoServer` (`N_REDO`)
`N_REDO` Sends geometry redo data to the client.
  
*Encode+Decode:* `RedoServer` on `S2C<Chan1>`

- `client_num: i32`
- `unpack_len: i32`
- `data: Vec<u8>`

### `Pong` (`N_PONG`)
`N_PONG` Basic ping packet sent to measure round-trip time.
  
*Encode+Decode:* `Pong` on `S2C<Chan1>` *(unreliable)*

- `ping: i32`

### `TimeUp` (`N_TIMEUP`)
`N_TIMEUP` Indicates that the current match time limit has been updated.
  
*Encode+Decode:* `TimeUp` on `S2C<Chan1>`

- `seconds_remaining: i32`

### `ServMsg` (`N_SERVMSG`)
`N_SERVMSG` Displays a direct system/server message in the client's console.
  
*Encode+Decode:* `ServMsg` on `S2C<Chan1>`

- `text: CubeString`

### `SendDemoList` (`N_SENDDEMOLIST`)
`N_SENDDEMOLIST` Replicates the list of available demos back to the client.
  
*Encode+Decode:* `SendDemoList` on `S2C<Chan1>`

- `demos: Vec<CubeString>`

### `DemoPlayback` (`N_DEMOPLAYBACK`)
`N_DEMOPLAYBACK` Informs the client that the server has started or stopped demo playback.
  
*Encode+Decode:* `DemoPlayback` on `S2C<Chan1>`

- `is_on: bool`
- `client_num: i32`

### `CurrentMaster` (`N_CURRENTMASTER`)
`N_CURRENTMASTER` Notifies clients of the current master mode and list of clients with privileges.
  
*Encode+Decode:* `CurrentMaster` on `S2C<Chan1>`

- `master_mode: MasterMode`
- `clients: Vec<MasterClient>`

### `MasterModeServer` (`N_MASTERMODE`)
`N_MASTERMODE` Updates the global Master Mode.
  
*Encode+Decode:* `MasterModeServer` on `S2C<Chan1>`

- `master_mode: MasterMode`

### `SpectatorServer` (`N_SPECTATOR`)
`N_SPECTATOR` Forces a player into or out of Spectator Mode.
  
*Encode+Decode:* `SpectatorServer` on `S2C<Chan1>`

- `client_num: i32`
- `is_spectator: bool`

### `SetTeamServer` (`N_SETTEAM`)
`N_SETTEAM` Moves a player to a specific team.
  
*Encode+Decode:* `SetTeamServer` on `S2C<Chan1>`

- `client_num: i32`
- `team: CubeString`
- `reason: SetTeamReason`

### `Announce` (`N_ANNOUNCE`)
`N_ANNOUNCE` Server announces important item spawns natively (e.g., Quad Damage).
  
*Encode+Decode:* `Announce` on `S2C<Chan1>`

- `item_type: ItemType`

### `ReqAuth` (`N_REQAUTH`)
`N_REQAUTH` Initiates an authentication request from the server.
  
*Encode+Decode:* `ReqAuth` on `S2C<Chan1>`

- `auth_domain: CubeString`

### `AuthChal` (`N_AUTHCHAL`)
`N_AUTHCHAL` Sends the cryptographic challenge down to the client.
  
*Encode+Decode:* `AuthChal` on `S2C<Chan1>`

- `desc: CubeString`
- `id: u32`
- `challenge: CubeString`

### `InitAi` (`N_INITAI`)
`N_INITAI` Spawns a server-side AI Bot into the client's game world.
  
*Encode+Decode:* `InitAi` on `S2C<Chan1>`

- `bot_cn: i32`
- `owner_cn: i32`
- `ai_type: AiType`
- `skill: i32`
- `player_model: PlayerModel`
- `name: CubeString`
- `team: CubeString`

### `BasesServer` (`N_BASES`)
`N_BASES` Sends the list of capture bases for the current map (Capture / Regen Capture).
  
*Encode+Decode:* `BasesServer` on `S2C<Chan1>`

- `bases: Vec<data::BaseInfo>`

### `BaseInfo` (`N_BASEINFO`)
`N_BASEINFO` Updates the state of a single capture base.
  
*Encode+Decode:* `BaseInfo` on `S2C<Chan1>`

- `base: i32`
- `owner: CubeString`
- `enemy: CubeString`
- `converted: bool`
- `ammo: i32`

### `BaseScore` (`N_BASESCORE`)
`N_BASESCORE` Updates the score for a team on a capture base.
  
*Encode+Decode:* `BaseScore` on `S2C<Chan1>`

- `base: i32`
- `team: CubeString`
- `total: i32`

### `RepAmmoServer` (`N_REPAMMO`)
`N_REPAMMO` Replenishes a player's ammo (Regen Capture).
  
*Encode+Decode:* `RepAmmoServer` on `S2C<Chan1>`

- `client_num: i32`
- `ammotype: ItemType`

### `BaseRegen` (`N_BASEREGEN`)
`N_BASEREGEN` Regenerates a player's health and armour at a capture base.
  
*Encode+Decode:* `BaseRegen` on `S2C<Chan1>`

- `client_num: i32`
- `health: i32`
- `armour: i32`
- `ammotype: ItemType`
- `ammo: i32`

### `DropFlag` (`N_DROPFLAG`)
`N_DROPFLAG` A player dropped a flag.
  
*Encode+Decode:* `DropFlag` on `S2C<Chan1>`

- `client_num: i32`
- `flag: i32`
- `version: i32`
- `drop_loc: Vec3<f32>`

### `ScoreFlag` (`N_SCOREFLAG`)
`N_SCOREFLAG` A player scored with a flag.
  
*Encode+Decode:* `ScoreFlag` on `S2C<Chan1>`

- `client_num: i32`
- `relay_flag: i32`
- `relay_version: i32`
- `goal_flag: i32`
- `goal_version: i32`
- `goal_spawn: i32`
- `team: i32`
- `score: i32`
- `oflags: i32`

### `ReturnFlag` (`N_RETURNFLAG`)
`N_RETURNFLAG` A player returned a flag to its base.
  
*Encode+Decode:* `ReturnFlag` on `S2C<Chan1>`

- `client_num: i32`
- `flag: i32`
- `version: i32`

### `TakeFlagServer` (`N_TAKEFLAG`)
`N_TAKEFLAG` A player picked up a flag.
  
*Encode+Decode:* `TakeFlagServer` on `S2C<Chan1>`

- `client_num: i32`
- `flag: i32`
- `version: i32`

### `ResetFlag` (`N_RESETFLAG`)
`N_RESETFLAG` A flag was reset to its spawn location.
  
*Encode+Decode:* `ResetFlag` on `S2C<Chan1>`

- `flag: i32`
- `version: i32`
- `spawn_index: i32`
- `team: i32`
- `score: i32`

### `InvisFlag` (`N_INVISFLAG`)
`N_INVISFLAG` Marks a flag as invisible or visible.
  
*Encode+Decode:* `InvisFlag` on `S2C<Chan1>`

- `flag: i32`
- `invis: bool`

### `InitTokensServer` (`N_INITTOKENS`)
`N_INITTOKENS` Sends initial token states to joining clients (Collect mode).
  
*Encode+Decode:* `InitTokensServer` on `S2C<Chan1>`

- `scores: [i32; 2]`
- `tokens: Vec<TokenState>`
- `client_tokens: Vec<ClientTokenState>`

### `TakeTokenServer` (`N_TAKETOKEN`)
`N_TAKETOKEN` A player collected a token.
  
*Encode+Decode:* `TakeTokenServer` on `S2C<Chan1>`

- `client_num: i32`
- `id: i32`
- `total: i32`

### `ExpireTokens` (`N_EXPIRETOKENS`)
`N_EXPIRETOKENS` Notifies that specific tokens have expired.
  
*Encode+Decode:* `ExpireTokens` on `S2C<Chan1>`

- `tokens: Vec<i32>`

### `DropTokens` (`N_DROPTOKENS`)
`N_DROPTOKENS` A player dropped their held tokens.
  
*Encode+Decode:* `DropTokens` on `S2C<Chan1>`

- `client_num: i32`
- `drop_loc: Vec3<f32>`
- `tokens: Vec<DroppedToken>`

### `StealTokens` (`N_STEALTOKENS`)
`N_STEALTOKENS` A player stole tokens from the enemy team.
  
*Encode+Decode:* `StealTokens` on `S2C<Chan1>`

- `client_num: i32`
- `team: i32`
- `basenum: i32`
- `enemy_team: i32`
- `score: i32`
- `drop_loc: Vec3<f32>`
- `tokens: Vec<StolenToken>`

### `DepositTokensServer` (`N_DEPOSITTOKENS`)
`N_DEPOSITTOKENS` A player deposited tokens at a base.
  
*Encode+Decode:* `DepositTokensServer` on `S2C<Chan1>`

- `client_num: i32`
- `base: i32`
- `deposited: i32`
- `team: i32`
- `score: i32`
- `flags: i32`

### `SpawnServer` (`N_SPAWN`)
`N_SPAWN` Sent to instruct a client to spawn in the game.
  
*Encode+Decode:* `Client<SpawnServer>` on `S2C<Chan1>`

- `state: PlayerState`

### `Taunt` (`N_TAUNT`)
`N_TAUNT` Indicates that a player is taunting.
  
*Encode+Decode:* `Client<Taunt>` on `S2C<Chan1>` · `Taunt` on `C2S<Chan1>`
  
*(no fields)*

### `CopyServer` (`N_COPY`)
`N_COPY` Coop Edit: Copy geometry.
  
*Encode+Decode:* `Client<CopyServer>` on `S2C<Chan1>`

- `sel: SelInfo`

### `PasteServer` (`N_PASTE`)
`N_PASTE` Coop Edit: Paste geometry.
  
*Encode+Decode:* `Client<PasteServer>` on `S2C<Chan1>`

- `sel: SelInfo`

### `Connect` (`N_CONNECT`)
`N_CONNECT` Attempts to connect to the server with user details and passwords.
  
*Encode+Decode:* `Connect` on `C2S<Chan1>`

- `name: CubeString`
- `player_model: PlayerModel`
- `password_hash: CubeString`
- `auth_desc: CubeString`
- `auth_name: CubeString`

### `AuthAnswer` (`N_AUTHANS`)
`N_AUTHANS` Provides the mathematical answer to a server's AuthChal.
  
*Encode+Decode:* `AuthAnswer` on `C2S<Chan1>`

- `desc: CubeString`
- `id: u32`
- `answer: CubeString`

### `Ping` (`N_PING`)
`N_PING` Measures the roundtrip time.
  
*Encode+Decode:* `Ping` on `C2S<Chan1>` *(unreliable)*

- `time: i32`

### `FromAi` (`N_FROMAI`)
`N_FROMAI` Alters the context of subsequent messages in the packet to act as if they came from the specified AI bot.
  
*Encode+Decode:* `FromAi` on `C2S<Chan1>`

- `qcn: i32`

### `MapCrc` (`N_MAPCRC`)
`N_MAPCRC` The client verifies the CRC of the map they currently have loaded.
  
*Encode+Decode:* `MapCrc` on `C2S<Chan1>`

- `map_name: CubeString`
- `crc: i32`

### `CheckMaps` (`N_CHECKMAPS`)
`N_CHECKMAPS` Forces a manual check of map CRCs on the server.
  
*Encode+Decode:* `CheckMaps` on `C2S<Chan1>`
  
*(no fields)*

### `TrySpawn` (`N_TRYSPAWN`)
`N_TRYSPAWN` Requests to spawn from the server.
  
*Encode+Decode:* `TrySpawn` on `C2S<Chan1>`
  
*(no fields)*

### `SpawnClient` (`N_SPAWN`)
`N_SPAWN` The client actually spawns.
  
*Encode+Decode:* `SpawnClient` on `C2S<Chan1>`

- `life_sequence: i32`
- `gun_select: Gun`

### `Suicide` (`N_SUICIDE`)
`N_SUICIDE` The client forcefully kills themselves.
  
*Encode+Decode:* `Suicide` on `C2S<Chan1>`
  
*(no fields)*

### `Shoot` (`N_SHOOT`)
`N_SHOOT` The client fired a weapon.
  
*Encode+Decode:* `Shoot` on `C2S<Chan1>`

- `id: i32`
- `gun: Gun`
- `from: Vec3<f32>`
- `to: Vec3<f32>`
- `hits: Vec<HitInfo>`

### `Explode` (`N_EXPLODE`)
`N_EXPLODE` Projectile explosion triggered by the client.
  
*Encode+Decode:* `Explode` on `C2S<Chan1>`

- `millis: i32`
- `gun: Gun`
- `id: i32`
- `hits: Vec<HitInfo>`

### `ItemPickup` (`N_ITEMPICKUP`)
`N_ITEMPICKUP` The client requested to pick up an item.
  
*Encode+Decode:* `ItemPickup` on `C2S<Chan1>`

- `item_index: i32`

### `SayTeamClient` (`N_SAYTEAM`)
`N_SAYTEAM` Submits a team chat message.
  
*Encode+Decode:* `SayTeamClient` on `C2S<Chan1>`

- `text: CubeString`

### `SwitchTeam` (`N_SWITCHTEAM`)
`N_SWITCHTEAM` The client requests to switch teams.
  
*Encode+Decode:* `SwitchTeam` on `C2S<Chan1>`

- `team: CubeString`

### `MapVote` (`N_MAPVOTE`)
`N_MAPVOTE` The client votes to change the map and mode.
  
*Encode+Decode:* `MapVote` on `C2S<Chan1>`

- `map_name: CubeString`
- `mode: GameMode`

### `MasterModeClient` (`N_MASTERMODE`)
`N_MASTERMODE` Requests to change the global Master Mode.
  
*Encode+Decode:* `MasterModeClient` on `C2S<Chan1>`

- `mode: MasterMode`

### `ClearBans` (`N_CLEARBANS`)
`N_CLEARBANS` Requests to clear all bans.
  
*Encode+Decode:* `ClearBans` on `C2S<Chan1>`
  
*(no fields)*

### `Kick` (`N_KICK`)
`N_KICK` Requests to kick a player.
  
*Encode+Decode:* `Kick` on `C2S<Chan1>`

- `victim_cn: i32`
- `reason: CubeString`

### `SpectatorClient` (`N_SPECTATOR`)
`N_SPECTATOR` Requests to move someone (or self) to/from the spectator team.
  
*Encode+Decode:* `SpectatorClient` on `C2S<Chan1>`

- `spectator_cn: i32`
- `val: bool`

### `SetTeamClient` (`N_SETTEAM`)
`N_SETTEAM` Requests to force someone to a specific team.
  
*Encode+Decode:* `SetTeamClient` on `C2S<Chan1>`

- `client_num: i32`
- `team: CubeString`

### `ForceIntermission` (`N_FORCEINTERMISSION`)
`N_FORCEINTERMISSION` Requests to force the current match to end.
  
*Encode+Decode:* `ForceIntermission` on `C2S<Chan1>`
  
*(no fields)*

### `RecordDemo` (`N_RECORDDEMO`)
`N_RECORDDEMO` Toggles automatic demo recording for the next match.
  
*Encode+Decode:* `RecordDemo` on `C2S<Chan1>`

- `val: bool`

### `StopDemo` (`N_STOPDEMO`)
`N_STOPDEMO` Stops the current demo recording.
  
*Encode+Decode:* `StopDemo` on `C2S<Chan1>`
  
*(no fields)*

### `ClearDemos` (`N_CLEARDEMOS`)
`N_CLEARDEMOS` Clears a specific demo (or all) from server memory.
  
*Encode+Decode:* `ClearDemos` on `C2S<Chan1>`

- `demo_id: i32`

### `ListDemos` (`N_LISTDEMOS`)
`N_LISTDEMOS` Requests a listing of all available demos on the server.
  
*Encode+Decode:* `ListDemos` on `C2S<Chan1>`
  
*(no fields)*

### `GetDemo` (`N_GETDEMO`)
`N_GETDEMO` Requests to download a specific demo from the server.
  
*Encode+Decode:* `GetDemo` on `C2S<Chan1>`

- `demo_id: i32`
- `tag: i32`

### `GetMap` (`N_GETMAP`)
`N_GETMAP` Requests to download the current map from the server (Coop Edit).
  
*Encode+Decode:* `GetMap` on `C2S<Chan1>`
  
*(no fields)*

### `SetMaster` (`N_SETMASTER`)
`N_SETMASTER` Requests to set master privileges using a password hash.
  
*Encode+Decode:* `SetMaster` on `C2S<Chan1>`

- `client_num: i32`
- `val: bool`
- `hash: CubeString`

### `AddBot` (`N_ADDBOT`)
`N_ADDBOT` Requests to add a bot with specific skill.
  
*Encode+Decode:* `AddBot` on `C2S<Chan1>`

- `skill: i32`

### `DelBot` (`N_DELBOT`)
`N_DELBOT` Requests to delete a bot.
  
*Encode+Decode:* `DelBot` on `C2S<Chan1>`
  
*(no fields)*

### `BotLimit` (`N_BOTLIMIT`)
`N_BOTLIMIT` Modifies the global bot limit.
  
*Encode+Decode:* `BotLimit` on `C2S<Chan1>`

- `limit: i32`

### `BotBalance` (`N_BOTBALANCE`)
`N_BOTBALANCE` Modifies the global bot auto-balance flag.
  
*Encode+Decode:* `BotBalance` on `C2S<Chan1>`

- `balance: bool`

### `AuthTry` (`N_AUTHTRY`)
`N_AUTHTRY` Submits a request to authenticate and claim privileges via cryptographic keys.
  
*Encode+Decode:* `AuthTry` on `C2S<Chan1>`

- `desc: CubeString`
- `name: CubeString`

### `AuthKick` (`N_AUTHKICK`)
`N_AUTHKICK` Submits a request to kick an individual via cryptographic auth verification.
  
*Encode+Decode:* `AuthKick` on `C2S<Chan1>`

- `desc: CubeString`
- `name: CubeString`
- `victim_cn: i32`
- `reason: CubeString`

### `PauseGameClient` (`N_PAUSEGAME`)
`N_PAUSEGAME` Toggles game paused state.
  
*Encode+Decode:* `PauseGameClient` on `C2S<Chan1>`

- `val: bool`

### `GameSpeedClient` (`N_GAMESPEED`)
`N_GAMESPEED` Changes global game speed.
  
*Encode+Decode:* `GameSpeedClient` on `C2S<Chan1>`

- `speed: i32`

### `CopyClient` (`N_COPY`)
`N_COPY` Coop Edit: Client copies geometry to their clipboard.
  
*Encode+Decode:* `CopyClient` on `C2S<Chan1>`

- `sel: SelInfo`

### `PasteClient` (`N_PASTE`)
`N_PASTE` Coop Edit: Client requests to paste geometry from their clipboard.
  
*Encode+Decode:* `PasteClient` on `C2S<Chan1>`

- `sel: SelInfo`

### `ClipboardClient` (`N_CLIPBOARD`)
`N_CLIPBOARD` Coop Edit: Uploads raw clipboard geometry data.
  
*Encode+Decode:* `ClipboardClient` on `C2S<Chan1>`

- `unpack_len: i32`
- `data: Vec<u8>`

### `UndoClient` (`N_UNDO`)
`N_UNDO` Coop Edit: Undo event.
  
*Encode+Decode:* `UndoClient` on `C2S<Chan1>`

- `unpack_len: i32`
- `data: Vec<u8>`

### `RedoClient` (`N_REDO`)
`N_REDO` Coop Edit: Redo event.
  
*Encode+Decode:* `RedoClient` on `C2S<Chan1>`

- `unpack_len: i32`
- `data: Vec<u8>`

### `RepAmmoClient` (`N_REPAMMO`)
`N_REPAMMO` Requests ammo replenishment (Regen Capture).
  
*Encode+Decode:* `RepAmmoClient` on `C2S<Chan1>`
  
*(no fields)*

### `TryDropFlag` (`N_TRYDROPFLAG`)
`N_TRYDROPFLAG` Client attempts to drop the flag they are carrying.
  
*Encode+Decode:* `TryDropFlag` on `C2S<Chan1>`
  
*(no fields)*

### `TakeFlagClient` (`N_TAKEFLAG`)
`N_TAKEFLAG` Client acknowledges picking up a flag.
  
*Encode+Decode:* `TakeFlagClient` on `C2S<Chan1>`

- `flag: i32`
- `version: i32`

### `InitFlagsClient` (`N_INITFLAGS`)
`N_INITFLAGS` Sends initial flag spawn positions to the server (CTF modes).
  
*Encode+Decode:* `InitFlagsClient` on `C2S<Chan1>`

- `flags: Vec<FlagSpawn>`

### `InitTokensClient` (`N_INITTOKENS`)
`N_INITTOKENS` Sends initial token base spawn data to the server (Collect mode).
  
*Encode+Decode:* `InitTokensClient` on `C2S<Chan1>`

- `bases: Vec<TokenBaseSpawn>`

### `TakeTokenClient` (`N_TAKETOKEN`)
`N_TAKETOKEN` Client collected a token.
  
*Encode+Decode:* `TakeTokenClient` on `C2S<Chan1>`

- `id: i32`

### `DepositTokensClient` (`N_DEPOSITTOKENS`)
`N_DEPOSITTOKENS` Client deposited tokens at a base.
  
*Encode+Decode:* `DepositTokensClient` on `C2S<Chan1>`

- `base: i32`

### `EditVar`
`N_EDITVAR` Coop Edit: A map variable was modified.
  
*Encode+Decode:* `Client<EditVar>` on `S2C<Chan1>` · `EditVar` on `C2S<Chan1>`

- `name: CubeString`
- `val: EditVarValue`

### `InitFlagsServer`
`N_INITFLAGS` Sends initial flag states to joining clients (CTF modes).
  
*Encode+Decode:* `InitFlagsServer` on `S2C<Chan1>`

- `scores: [i32; 2]`
- `flags: Vec<FlagState>`

### `BasesClient`
`N_BASES` Sends the list of capture base spawns to the server (Capture / Regen Capture).
  
*Encode+Decode:* `BasesClient` on `C2S<Chan1>`

- `bases: Vec<BaseSpawn>`

---

## Channel 2 Packets (`sauer_proto::channel2`)

Bulk file transfers (demo downloads, map sends). Server → Client only.  
Enum type: `Chan2Server` for `S2C<Chan2>`.  
Data field uses `read_all_remaining` — consumes all bytes to end of packet.

### `DemoPacket` (`N_DEMOPACKET`)
`N_DEMOPACKET` Demo playback marker.
  
*Encode+Decode:* `DemoPacket` on `S2C<Chan0>` · `DemoPacket` on `S2C<Chan1>` · `DemoPacket` on `S2C<Chan2>`
  
*(no fields)*

### `SendDemo` (`N_SENDDEMO`)
`N_SENDDEMO` Sent over Channel 2 to stream a recorded demo file to the client.
  
*Encode+Decode:* `SendDemo` on `S2C<Chan2>`

- `tag: i32`
- `data: Vec<u8>`

### `SendMap` (`N_SENDMAP`)
`N_SENDMAP` Sent over Channel 2 to stream a cooperative edit map file to the client.
  
*Encode+Decode:* `SendMap` on `S2C<Chan2>`

- `data: Vec<u8>`

---

## ExtInfo (`sauer_proto::extinfo`)

Out-of-band UDP protocol for server browser queries. Not covered in detail here — see `sauer_proto::extinfo` module directly.

