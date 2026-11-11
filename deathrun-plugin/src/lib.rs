pub mod edit;
pub mod geo_sync;
pub mod map_state;
pub mod play;
mod recreate;
pub mod trap;
pub mod ui;

pub use edit::DeathrunEdit;
pub use play::DeathrunPlay;

pub const SERVER_BOT_CN: sauer_proto::data::Cn = sauer_proto::data::Cn(0);

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
