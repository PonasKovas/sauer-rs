//! Kicks, temporary IP bans and the admin password, as in the engine's
//! `fpsgame/server.cpp` (`trykick`, `addban`, `checkpassword`).

use crate::Server;
use sauer_proto::data::Cn;
use std::net::IpAddr;
use std::time::{Duration, Instant};
use tiger::{Digest, Tiger};

/// Engine disconnect reasons (`shared/iengine.h`), shown to the client.
pub const DISC_KICK: u32 = 3;
pub const DISC_IPBAN: u32 = 5;

/// How long `/kick` bans the player's IP, like the engine.
const KICK_BAN: Duration = Duration::from_secs(4 * 60 * 60);

/// The hash the client sends for `/setmaster PASSWORD`: Tiger of
/// `"<cn> <sessionid> <password>"`, written as hex with the low nibble of each
/// byte first (engine `hashstring`).
pub fn hash_password(cn: Cn, session_id: i32, password: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Tiger::digest(format!("{cn} {session_id} {password}").as_bytes());
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        out.push(HEX[(b & 0xF) as usize] as char);
        out.push(HEX[(b >> 4) as usize] as char);
    }
    out
}

impl Server {
    /// Disconnects a client, telling it why (one of the `DISC_*` codes).
    pub fn disconnect_with(&self, id: u64, reason: u32) {
        if let Some(peer) = self.clients.get(&id) {
            peer.try_disconnect_with(reason);
        }
    }

    pub fn ban_ip(&self, ip: IpAddr) {
        self.bans.write().push((ip, Instant::now() + KICK_BAN));
    }

    pub fn is_banned(&self, ip: IpAddr) -> bool {
        let mut bans = self.bans.write();
        let now = Instant::now();
        bans.retain(|(_, until)| *until > now);
        bans.iter().any(|(b, _)| *b == ip)
    }

    pub fn clear_bans(&self) {
        self.bans.write().clear();
    }

    /// Whether `password` hashed for this client matches the admin password.
    pub fn check_admin_pass(&self, id: u64, hash: &[u8]) -> bool {
        if hash.is_empty() {
            return false;
        }
        let pass = self.config.read().admin_pass.clone();
        let (Some(cn), Some(session_id)) =
            (self.get_cn(id), self.players.with(id, |p| p.session_id))
        else {
            return false;
        };
        let ok = !pass.is_empty() && hash == hash_password(cn, session_id, &pass).as_bytes();
        let result = match (pass.is_empty(), ok) {
            (true, _) => "no admin_pass is configured",
            (false, true) => "accepted",
            (false, false) => "wrong password",
        };
        tracing::info!("cn {cn} tried an admin password: {result}");
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::hash_password;
    use sauer_proto::data::Cn;

    /// Expected values come from the engine's own `shared/crypto.cpp`
    /// (`tiger::hash` + `hashstring`) compiled on its own.
    #[test]
    fn matches_engine_hash() {
        assert_eq!(
            hash_password(Cn(0), 12345, "hunter2"),
            "f1b00dd400900b9f191d7d0c362bc232225b07beec287582"
        );
        assert_eq!(
            hash_password(Cn(7), -98765, "hello world"),
            "04769a5d1ea8d1065daa8753bb7db9cedd4c3deb8ab5d375"
        );
        assert_eq!(
            hash_password(
                Cn(3),
                42,
                "a-much-longer-password-that-goes-over-sixty-four-bytes-to-hit-a-second-block"
            ),
            "c5928910c469156f704ff7cd2fdf29f86a1d167c1815b293"
        );
    }
}
