use std::error::Error;

use crate::{
    Chan0Client, Chan0Server, Chan1Client, Chan1Server, Chan2Server, Reader, Writer,
    data::*,
    internal::{Decode, Encode},
    protocol_route::{C2S, Chan0, Chan1, Chan2, ProtocolRoute, S2C},
};
use cube_string::CubeString;

/// Helper to cleanly generate CubeStrings from static string slices
fn cstr(s: &str) -> CubeString {
    CubeString::from_bytes(s.as_bytes().to_vec())
}

macro_rules! assert_roundtrip {
    ($route:path, $packet:expr) => {
        if let Err(e) = assert_roundtrip::<_, $route>($packet) {
            panic!("{} failed: {e}", line!());
        }
    };
}

/// The core roundtrip tester
fn assert_roundtrip<T, R>(packet: T) -> Result<(), Box<dyn Error>>
where
    T: Encode + Decode + PartialEq + std::fmt::Debug,
    R: ProtocolRoute<Message = T>,
{
    let mut writer = Writer::<R>::new();
    writer.push(&packet);
    let buffer = writer.consume();

    let mut reader = Reader::<R>::new(&buffer);
    let decoded = match reader.next() {
        Some(x) => x?,
        None => return Err(format!("No packet decoded").into()),
    };

    if reader.core.remaining() != 0 {
        return Err(format!(
            "Reader did not consume all bytes ({} bytes remaining)\n\n{decoded:?}",
            reader.core.remaining()
        )
        .into());
    }

    if packet != decoded {
        return Err(format!("Roundtrip mismatch: {decoded:?}").into());
    }

    Ok(())
}

#[test]
fn test_chan0_client_roundtrip() {
    type R = C2S<Chan0>;
    assert_roundtrip!(
        R,
        Chan0Client::Position {
            pos: PlayerPosition {
                client_num: 123,
                phys_state: PositionPhysState { phys_state: PhysState::Fall, life_sequence: true, move_dir: -1, strafe: 1 },
                flags: 0x7F, // Enable all bitflags (tests the maximum payload size & sign extensions)
                pos: Vector3 {
                    x: -10.0,
                    y: 20.0,
                    z: -30.0,
                },
                dir: 300,
                roll: 15,
                mag: 100,
                vel_dir: 200,
                fall_mag: 50,
                fall_dir: 100,
            },
        }
    );

    assert_roundtrip!(
        R,
        Chan0Client::Teleport {
            client_num: 42,
            tp: 1,
            td: 2,
        }
    );

    assert_roundtrip!(
        R,
        Chan0Client::JumpPad {
            client_num: 42,
            jp: 99,
        }
    );
}

#[test]
fn test_chan1_client_roundtrip() {
    type R = C2S<Chan1>;

    // Connection & Auth
    assert_roundtrip!(
        R,
        Chan1Client::Connect {
            name: cstr("SauerPlayer"),
            player_model: PlayerModel::Ogro2,
            password_hash: cstr("hash_123"),
            auth_desc: cstr("local"),
            auth_name: cstr("admin"),
        }
    );
    assert_roundtrip!(
        R,
        Chan1Client::AuthAnswer {
            desc: cstr("domain"),
            id: 42,
            answer: cstr("answer123"),
        }
    );
    assert_roundtrip!(
        R,
        Chan1Client::AuthTry {
            desc: cstr("admin"),
            name: cstr("player1"),
        }
    );
    assert_roundtrip!(
        R,
        Chan1Client::AuthKick {
            desc: cstr("admin"),
            name: cstr("player1"),
            victim_cn: 5,
            reason: cstr("spam"),
        }
    );

    // Basic Client State & Queries
    assert_roundtrip!(R, Chan1Client::Ping { time: 100 });
    assert_roundtrip!(R, Chan1Client::ClientPing { ping: 45 });
    assert_roundtrip!(R, Chan1Client::FromAi { qcn: 10 });
    assert_roundtrip!(R, Chan1Client::CheckMaps);
    assert_roundtrip!(
        R,
        Chan1Client::MapCrc {
            map_name: cstr("complex"),
            crc: 123456789,
        }
    );

    // Spawning & Death
    assert_roundtrip!(R, Chan1Client::TrySpawn);
    assert_roundtrip!(
        R,
        Chan1Client::Spawn {
            life_sequence: 1,
            gun_select: Gun::Rl,
        }
    );
    assert_roundtrip!(R, Chan1Client::Suicide);

    // Combat Events
    assert_roundtrip!(R, Chan1Client::GunSelect { gun: Gun::Rl });
    assert_roundtrip!(
        R,
        Chan1Client::Shoot {
            id: 1,
            gun: Gun::Sg,
            from: Vector3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            to: Vector3 {
                x: 4.0,
                y: 5.0,
                z: 6.0,
            },
            hits: vec![HitInfo {
                target_cn: 2,
                life_sequence: 1,
                dist: 10.0,
                rays: 1,
                dir: Vector3 {
                    x: 0.0,
                    y: 0.0,
                    z: 1.0,
                },
            }],
        }
    );
    assert_roundtrip!(
        R,
        Chan1Client::Explode {
            millis: 1000,
            gun: Gun::Rl,
            id: 2,
            hits: vec![HitInfo {
                target_cn: 3,
                life_sequence: 2,
                dist: 5.0,
                rays: 3,
                dir: Vector3 {
                    x: 0.5,
                    y: -0.5,
                    z: 0.25,
                },
            }],
        }
    );

    // Chat & Personal Customizations
    assert_roundtrip!(
        R,
        Chan1Client::Text {
            text: cstr("Hello server"),
        }
    );
    assert_roundtrip!(
        R,
        Chan1Client::SayTeam {
            text: cstr("Need backup"),
        }
    );
    assert_roundtrip!(
        R,
        Chan1Client::SwitchName {
            name: cstr("NewName"),
        }
    );
    assert_roundtrip!(R, Chan1Client::SwitchModel { model: PlayerModel::Snoutx10k });
    assert_roundtrip!(R, Chan1Client::SwitchTeam { team: cstr("evil") });

    // Items
    assert_roundtrip!(R, Chan1Client::ItemPickup { item_index: 5 });
    assert_roundtrip!(
        R,
        Chan1Client::ItemList {
            items: vec![
                ItemSpawn {
                    item_index: 0,
                    item_type: ItemType::Quad,
                },
                ItemSpawn {
                    item_index: 1,
                    item_type: ItemType::Health,
                },
            ],
        }
    );

    // Matches, Demos, Bots
    assert_roundtrip!(
        R,
        Chan1Client::MapVote {
            map_name: cstr("forge"),
            mode: GameMode::TeamPlay,
        }
    );
    assert_roundtrip!(R, Chan1Client::ForceIntermission);
    assert_roundtrip!(R, Chan1Client::RecordDemo { val: true });
    assert_roundtrip!(R, Chan1Client::StopDemo);
    assert_roundtrip!(R, Chan1Client::ClearDemos { demo_id: 2 });
    assert_roundtrip!(R, Chan1Client::ListDemos);
    assert_roundtrip!(
        R,
        Chan1Client::GetDemo {
            demo_id: 1,
            tag: 42,
        }
    );
    assert_roundtrip!(R, Chan1Client::AddBot { skill: 85 });
    assert_roundtrip!(R, Chan1Client::DelBot);
    assert_roundtrip!(R, Chan1Client::BotLimit { limit: 10 });
    assert_roundtrip!(R, Chan1Client::BotBalance { balance: true });
    assert_roundtrip!(R, Chan1Client::PauseGame { val: true });
    assert_roundtrip!(R, Chan1Client::GameSpeed { speed: 150 });
    assert_roundtrip!(
        R,
        Chan1Client::Spectator {
            spectator_cn: 2,
            val: true,
        }
    );

    // Admin commands
    assert_roundtrip!(
        R,
        Chan1Client::MasterMode {
            mode: MasterMode::Locked,
        }
    );
    assert_roundtrip!(R, Chan1Client::ClearBans);
    assert_roundtrip!(
        R,
        Chan1Client::Kick {
            victim_cn: 2,
            reason: cstr("Teamkilling"),
        }
    );
    assert_roundtrip!(
        R,
        Chan1Client::SetTeam {
            client_num: 3,
            team: cstr("good"),
        }
    );
    assert_roundtrip!(
        R,
        Chan1Client::SetMaster {
            client_num: 1,
            val: true,
            hash: cstr("password_hash"),
        }
    );
    assert_roundtrip!(
        R,
        Chan1Client::ServCmd {
            cmd: cstr("ban player2"),
        }
    );

    // Coop Edit Map Events
    assert_roundtrip!(R, Chan1Client::GetMap);
    assert_roundtrip!(R, Chan1Client::NewMap { size: 12 });
    assert_roundtrip!(R, Chan1Client::EditMode { val: true });
    assert_roundtrip!(R, Chan1Client::Remip);
    assert_roundtrip!(R, Chan1Client::Copy);
    assert_roundtrip!(R, Chan1Client::Paste);
    assert_roundtrip!(
        R,
        Chan1Client::Clipboard {
            unpack_len: 100,
            pack_len: 2,
            data: vec![0xAA, 0xBB],
        }
    );
    assert_roundtrip!(
        R,
        Chan1Client::Undo {
            unpack_len: 100,
            pack_len: 2,
            data: vec![0xCC, 0xDD],
        }
    );
    assert_roundtrip!(
        R,
        Chan1Client::Redo {
            unpack_len: 100,
            pack_len: 2,
            data: vec![0xEE, 0xFF],
        }
    );

    let sel = SelInfo {
        o: Vector3 { x: 0, y: 0, z: 0 },
        s: Vector3 { x: 1, y: 1, z: 1 },
        grid: 2,
        orient: Orient::Front,
        cx: 4,
        cxs: 5,
        cy: 6,
        cys: 7,
        corner: 8,
    };
    assert_roundtrip!(
        R,
        Chan1Client::EditFace {
            sel: sel.clone(),
            dir: 1,
            mode: EditFaceMode::Corner,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Client::EditTex {
            sel: sel.clone(),
            tex: 1,
            all_faces: true,
            extra_data: vec![1, 2],
        }
    );
    assert_roundtrip!(
        R,
        Chan1Client::EditMat {
            sel: sel.clone(),
            mat: 1,
            filter: 2,
        }
    );
    assert_roundtrip!(R, Chan1Client::Flip { sel: sel.clone() });
    assert_roundtrip!(
        R,
        Chan1Client::Rotate {
            sel: sel.clone(),
            dir: 1,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Client::Replace {
            sel: sel.clone(),
            old_tex: 1,
            new_tex: 2,
            in_sel: 3,
            extra_data: vec![3, 4],
        }
    );
    assert_roundtrip!(R, Chan1Client::DelCube { sel: sel.clone() });
    assert_roundtrip!(
        R,
        Chan1Client::EditVSlot {
            sel: sel.clone(),
            delta: 1,
            all_faces: true,
            extra_data: vec![5, 6],
        }
    );
    assert_roundtrip!(
        R,
        Chan1Client::EditEnt {
            ent_id: 1,
            x: 2,
            y: 3,
            z: 4,
            type_: EntityType::Particles,
            attr1: 6,
            attr2: 7,
            attr3: 8,
            attr4: 9,
            attr5: 10,
        }
    );

    // Edit Vars
    assert_roundtrip!(
        R,
        Chan1Client::EditVar {
            name: cstr("fog"),
            val: EditVarValue::Int(100),
        }
    );
    assert_roundtrip!(
        R,
        Chan1Client::EditVar {
            name: cstr("waterfallenv"),
            val: EditVarValue::Float(0.5),
        }
    );
    assert_roundtrip!(
        R,
        Chan1Client::EditVar {
            name: cstr("maptitle"),
            val: EditVarValue::String(cstr("My Map")),
        }
    );

    // CTF Mode
    assert_roundtrip!(R, Chan1Client::TryDropFlag);
    assert_roundtrip!(
        R,
        Chan1Client::TakeFlag {
            flag: 1,
            version: 2,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Client::InitFlags {
            flags: vec![FlagSpawn {
                team: 1,
                pos: Vector3 {
                    x: 10.0,
                    y: 20.0,
                    z: 30.0,
                },
            }],
        }
    );

    // Capture Mode
    assert_roundtrip!(
        R,
        Chan1Client::Bases {
            bases: vec![BaseSpawn {
                ammotype: Some(ItemType::Shells),
                pos: Vector3 {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                },
            }],
        }
    );
    assert_roundtrip!(R, Chan1Client::RepAmmo);

    // Collect / Tokens Mode
    assert_roundtrip!(
        R,
        Chan1Client::InitTokens {
            bases: vec![TokenBaseSpawn {
                team: 2,
                pos: Vector3 {
                    x: 5.0,
                    y: 6.0,
                    z: 7.0,
                },
            }],
        }
    );
    assert_roundtrip!(R, Chan1Client::TakeToken { id: 5 });
    assert_roundtrip!(R, Chan1Client::DepositTokens { base: 2 });
}

#[test]
fn test_chan0_server_roundtrip() {
    type R = S2C<Chan0>;

    assert_roundtrip!(R, Chan0Server::DemoPacket);

    assert_roundtrip!(
        R,
        Chan0Server::Position {
            pos: PlayerPosition {
                client_num: 123,
                phys_state: PositionPhysState { phys_state: PhysState::Fall, life_sequence: true, move_dir: -1, strafe: 1 },
                flags: 0x7F, // Enable all bitflags (so we test the maximum payload size & sign extension)
                pos: Vector3 {
                    x: -10.0,
                    y: 20.0,
                    z: -30.0,
                },
                dir: 300,
                roll: 15,
                mag: 100,
                vel_dir: 200,
                fall_mag: 50,
                fall_dir: 100,
            },
        }
    );

    assert_roundtrip!(
        R,
        Chan0Server::Teleport {
            client_num: 42,
            tp: 1,
            td: 2,
        }
    );

    assert_roundtrip!(
        R,
        Chan0Server::JumpPad {
            client_num: 42,
            jp: 99,
        }
    );
}

#[test]
fn test_chan2_server_roundtrip() {
    type R = S2C<Chan2>;

    assert_roundtrip!(R, Chan2Server::DemoPacket);

    assert_roundtrip!(
        R,
        Chan2Server::SendDemo {
            tag: 123,
            data: vec![0x01, 0x02, 0x03, 0x04],
        }
    );

    assert_roundtrip!(
        R,
        Chan2Server::SendMap {
            data: vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF],
        }
    );
}

#[test]
fn test_chan1_server_roundtrip() {
    type R = S2C<Chan1>;

    // Standard State Info
    assert_roundtrip!(R, Chan1Server::DemoPacket);
    assert_roundtrip!(
        R,
        Chan1Server::ServInfo {
            client_num: 1,
            protocol: 258,
            session_id: 12345,
            has_password: true,
            server_desc: cstr("Test Server"),
            server_auth: cstr("auth.domain.com"),
        }
    );
    assert_roundtrip!(R, Chan1Server::Welcome);
    assert_roundtrip!(
        R,
        Chan1Server::PauseGame {
            is_paused: true,
            client_num: 42,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::GameSpeed {
            speed: 150,
            client_num: 42,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::MapChange {
            map_name: cstr("complex"),
            mode: GameMode::TeamPlay,
            has_items: true,
        }
    );

    // Chat & Audio (These trigger the N_CLIENT envelope)
    assert_roundtrip!(
        R,
        Chan1Server::Sound {
            client_num: 1,
            sound_id: 12,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::Text {
            client_num: 2,
            text: cstr("Hello world!"),
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::SayTeam {
            client_num: 3,
            text: cstr("Defend the base!"),
        }
    );

    // Player Actions & Events
    assert_roundtrip!(R, Chan1Server::ForceDeath { client_num: 42 });
    assert_roundtrip!(
        R,
        Chan1Server::ItemList {
            items: vec![
                ItemSpawn {
                    item_index: 0,
                    item_type: ItemType::Health,
                },
                ItemSpawn {
                    item_index: 1,
                    item_type: ItemType::Quad,
                },
            ],
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::InitClient {
            client_num: 5,
            name: cstr("SauerPlayer"),
            team: cstr("good"),
            player_model: PlayerModel::Inky,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::SwitchName {
            client_num: 6,
            name: cstr("NewName"),
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::SwitchModel {
            client_num: 7,
            model: PlayerModel::CaptainCannon,
        }
    );
    assert_roundtrip!(R, Chan1Server::ClientDisconnected { client_num: 8 });

    // Spawning
    let p_state = PlayerState {
        life_sequence: 1,
        health: 100,
        max_health: 100,
        armour: 50,
        armour_type: ArmourType::Green,
        gun_select: Gun::Rl,
        ammo: [10, 20, 30, 40, 50, 60], // Must be exactly 6 elements (SG to Pistol)
    };
    assert_roundtrip!(
        R,
        Chan1Server::Spawn {
            client_num: 9,
            state: p_state.clone(),
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::SpawnState {
            client_num: 10,
            state: p_state.clone(),
        }
    );

    // Combat
    assert_roundtrip!(
        R,
        Chan1Server::ShotFx {
            client_num: 11,
            gun: Gun::Cg,
            id: 100,
            from: Vector3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            to: Vector3 {
                x: 4.0,
                y: 5.0,
                z: 6.0,
            },
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::ExplodeFx {
            client_num: 12,
            gun: Gun::Rl,
            id: 101,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::Damage {
            target_cn: 1,
            actor_cn: 2,
            damage: 30,
            armour: 10,
            health: 60,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::HitPush {
            target_cn: 1,
            gun: Gun::Sg,
            damage: 15,
            dir: Vector3 {
                x: 0.5,
                y: -0.5,
                z: 0.25,
            }, // Floats mapped exactly by DMF
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::Died {
            victim_cn: 1,
            actor_cn: 2,
            actor_frags: 5,
            team_frags: 10,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::GunSelect {
            client_num: 1,
            gun: Gun::Sg,
        }
    );
    assert_roundtrip!(R, Chan1Server::Taunt { client_num: 1 });

    // Scores & Sync State
    assert_roundtrip!(
        R,
        Chan1Server::TeamInfo {
            teams: vec![
                TeamInfo {
                    team: cstr("good"),
                    frags: 100,
                },
                TeamInfo {
                    team: cstr("evil"),
                    frags: 80,
                },
            ],
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::Resume {
            clients: vec![ClientResumeState {
                client_num: 1,
                state: ClientState::Alive,
                frags: 10,
                flags: 1,
                deaths: 2,
                quad_millis: 5000,
                player_state: p_state,
            }],
        }
    );
    assert_roundtrip!(R, Chan1Server::ItemSpawn { item_index: 5 });
    assert_roundtrip!(
        R,
        Chan1Server::ItemAck {
            item_index: 5,
            client_num: 1,
        }
    );

    // Coop Edit Map Events
    assert_roundtrip!(
        R,
        Chan1Server::Clipboard {
            client_num: 1,
            unpack_len: 100,
            pack_len: 2,
            data: vec![0xAA, 0xBB],
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::Undo {
            client_num: 1,
            unpack_len: 100,
            pack_len: 2,
            data: vec![0xCC, 0xDD],
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::Redo {
            client_num: 1,
            unpack_len: 100,
            pack_len: 2,
            data: vec![0xEE, 0xFF],
        }
    );

    let sel = SelInfo {
        o: Vector3 { x: 0, y: 0, z: 0 },
        s: Vector3 { x: 1, y: 1, z: 1 },
        grid: 2,
        orient: Orient::Front,
        cx: 4,
        cxs: 5,
        cy: 6,
        cys: 7,
        corner: 8,
    };
    assert_roundtrip!(
        R,
        Chan1Server::EditFace {
            client_num: 1,
            sel: sel.clone(),
            dir: 1,
            mode: EditFaceMode::Corner,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::EditTex {
            client_num: 1,
            sel: sel.clone(),
            tex: 1,
            all_faces: false,
            extra_data: vec![1, 2],
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::EditMat {
            client_num: 1,
            sel: sel.clone(),
            mat: 2,
            filter: 3,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::Flip {
            client_num: 1,
            sel: sel.clone(),
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::Copy {
            client_num: 1,
            sel: sel.clone(),
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::Paste {
            client_num: 1,
            sel: sel.clone(),
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::Rotate {
            client_num: 1,
            sel: sel.clone(),
            dir: 1,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::Replace {
            client_num: 1,
            sel: sel.clone(),
            old_tex: 1,
            new_tex: 2,
            in_sel: 1,
            extra_data: vec![3, 4],
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::DelCube {
            client_num: 1,
            sel: sel.clone(),
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::EditVSlot {
            client_num: 1,
            sel: sel.clone(),
            delta: 1,
            all_faces: false,
            extra_data: vec![5, 6],
        }
    );
    assert_roundtrip!(R, Chan1Server::Remip { client_num: 1 });

    assert_roundtrip!(
        R,
        Chan1Server::EditEnt {
            client_num: 1,
            ent_id: 2,
            pos: Vector3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            type_: EntityType::PlayerStart,
            attr1: 4,
            attr2: 5,
            attr3: 6,
            attr4: 7,
            attr5: 8,
        }
    );

    // Edit Vars
    assert_roundtrip!(
        R,
        Chan1Server::EditVar {
            client_num: 1,
            name: cstr("test_int"),
            val: EditVarValue::Int(42),
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::EditVar {
            client_num: 1,
            name: cstr("test_float"),
            val: EditVarValue::Float(3.14),
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::EditVar {
            client_num: 1,
            name: cstr("test_str"),
            val: EditVarValue::String(cstr("foo")),
        }
    );

    // Admin, Match, & Server Settings
    assert_roundtrip!(R, Chan1Server::Pong { ping: 50 });
    assert_roundtrip!(
        R,
        Chan1Server::ClientPing {
            client_num: 1,
            ping: 50,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::TimeUp {
            seconds_remaining: 10,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::ServMsg {
            text: cstr("Server is restarting"),
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::SendDemoList {
            demos: vec![cstr("demo1"), cstr("demo2")],
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::DemoPlayback {
            is_on: true,
            client_num: 1,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::CurrentMaster {
            master_mode: MasterMode::Locked,
            clients: vec![MasterClient {
                client_num: 1,
                privilege: Privilege::Admin,
            }],
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::MasterMode {
            master_mode: MasterMode::Open,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::EditMode {
            client_num: 1,
            is_editing: true,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::Spectator {
            client_num: 1,
            is_spectator: true,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::SetTeam {
            client_num: 1,
            team: cstr("good"),
            reason: 0,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::Announce {
            item_type: ItemType::Quad,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::NewMap {
            client_num: Some(1),
            size: 10,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::NewMap {
            client_num: None,
            size: 12,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::ReqAuth {
            auth_domain: cstr("local"),
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::AuthChal {
            desc: cstr("desc"),
            id: 123,
            challenge: cstr("chal"),
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::InitAi {
            bot_cn: 1,
            owner_cn: 2,
            ai_type: AiType::Bot,
            skill: 85,
            player_model: PlayerModel::CaptainCannon,
            name: cstr("Bot"),
            team: cstr("good"),
        }
    );
    assert_roundtrip!(R, Chan1Server::ServCmd { cmd: cstr("kick") });

    // Capture Mode
    assert_roundtrip!(
        R,
        Chan1Server::Bases {
            bases: vec![BaseInfo {
                ammotype: Some(ItemType::Shells),
                owner: cstr("good"),
                enemy: cstr("evil"),
                converted: false,
                ammo: 10,
            }],
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::BaseInfo {
            base: 1,
            owner: cstr("good"),
            enemy: cstr(""),
            converted: false,
            ammo: 5,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::BaseScore {
            base: 1,
            team: cstr("good"),
            total: 100,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::RepAmmo {
            client_num: 1,
            ammotype: ItemType::Shells,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::BaseRegen {
            client_num: 1,
            health: 100,
            armour: 50,
            ammotype: ItemType::Shells,
            ammo: 10,
        }
    );

    // CTF Mode
    assert_roundtrip!(
        R,
        Chan1Server::InitFlags {
            scores: [10, 20],
            flags: vec![FlagState {
                version: 1,
                spawn_index: 2,
                owner: FlagStateOwner::Owned { owner: 1 },
                invis: false,
                hold: Some(FlagStateHoldTeam::None),
            }],
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::DropFlag {
            client_num: 1,
            flag: 2,
            version: 3,
            drop_loc: Vector3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::ScoreFlag {
            client_num: 1,
            relay_flag: 2,
            relay_version: 3,
            goal_flag: 4,
            goal_version: 5,
            goal_spawn: 6,
            team: 7,
            score: 8,
            oflags: 9,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::ReturnFlag {
            client_num: 1,
            flag: 2,
            version: 3,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::TakeFlag {
            client_num: 1,
            flag: 2,
            version: 3,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::ResetFlag {
            flag: 1,
            version: 2,
            spawn_index: 3,
            team: 4,
            score: 5,
        }
    );
    assert_roundtrip!(R, Chan1Server::InvisFlag { flag: 1, invis: true });

    // Collect / Tokens Mode
    assert_roundtrip!(
        R,
        Chan1Server::InitTokens {
            scores: [1, 2],
            tokens: vec![TokenState {
                id: 1,
                team: 2,
                yaw: 90,
                pos: Vector3 {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                },
            }],
            client_tokens: vec![ClientTokenState {
                client_num: 1,
                tokens: 5,
            }],
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::TakeToken {
            client_num: 1,
            id: 2,
            total: 3,
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::ExpireTokens {
            tokens: vec![1, 2, 3],
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::DropTokens {
            client_num: 1,
            drop_loc: Vector3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            tokens: vec![DroppedToken {
                id: 1,
                team: 2,
                yaw: 90,
            }],
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::StealTokens {
            client_num: 1,
            team: 2,
            basenum: 3,
            enemy_team: 4,
            score: 5,
            drop_loc: Vector3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            tokens: vec![StolenToken { id: 1, yaw: 90 }],
        }
    );
    assert_roundtrip!(
        R,
        Chan1Server::DepositTokens {
            client_num: 1,
            base: 2,
            deposited: 3,
            team: 4,
            score: 5,
            flags: 6,
        }
    );
}
