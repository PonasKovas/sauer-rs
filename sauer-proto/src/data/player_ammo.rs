/// Ammo counts for each weapon slot.
///
/// Fields are ordered to match the wire protocol (Shotgun → Pistol).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlayerAmmo {
    pub shotgun: i32,
    pub chaingun: i32,
    pub rocket_launcher: i32,
    pub rifle: i32,
    pub grenade_launcher: i32,
    pub pistol: i32,
}
