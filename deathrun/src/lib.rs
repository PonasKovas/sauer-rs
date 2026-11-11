pub mod aim;
pub mod edit;
pub mod map_state;
pub mod map_sync;
pub mod play;
pub mod trap;
pub mod ui;

pub use edit::DeathrunEdit;
pub use play::DeathrunPlay;

pub const SERVER_BOT_CN: sauer_proto::data::Cn = sauer_proto::data::Cn(0);

/// Adds the bot that pastes geometry and speaks for the server.
fn add_server_bot(server: &mut sauer_server::Server) {
    use cube_string::IntoCubeStringLossy;
    let bot = sauer_server::FakePlayer {
        name: "Server".lossy_cubestring(),
        model: sauer_proto::data::PlayerModel::MrFixit,
        team: Default::default(),
    };
    assert!(
        server.add_fake_player(SERVER_BOT_CN, bot),
        "the server bot cn is already taken"
    );
}

/// Lines the host adds to every welcome wall and to the end of `#help`, such
/// as where to send new maps. Set with `with_notes` on either plugin; none by
/// default. Sauer color codes (`\f0`-`\f8`) work in them.
#[derive(Clone, Default)]
pub struct ServerNotes(pub Vec<cube_string::CubeString>);

impl ServerNotes {
    fn from_strings(notes: Vec<String>) -> Self {
        use cube_string::IntoCubeStringLossy;
        Self(
            notes
                .iter()
                .map(|n| n.as_str().lossy_cubestring())
                .collect(),
        )
    }

    /// Stores the notes where the welcome walls read them, and adds them to `#help`.
    fn install(&self, server: &mut sauer_server::Server) {
        if !self.0.is_empty() {
            server.add_help_lines(
                std::iter::once(ui::section("Notes")).chain(ui::note_lines(&self.0)),
            );
        }
        server.extra.insert(self.clone());
    }
}
