use cube_string::CubeString;
use sauer_proto::data::Privilege;

pub struct AuthUser {
    pub name: CubeString,
    /// Auth domain (e.g. the server's auth name, or an external domain).
    pub desc: CubeString,
    /// Public key in Sauerbraten `[+-]hex_x` format.
    pub pubkey: CubeString,
    pub privilege: Privilege,
}

pub struct BasePluginOptions {
    /// Displayed in the server browser.
    pub server_desc: CubeString,
    /// Auth domain for server authentication (leave empty to disable).
    pub server_auth: CubeString,
    /// Server join password. Clients must supply the correct Tiger hash in N_CONNECT.
    pub password: Option<CubeString>,
    /// Admin password. Clients hashing this correctly via N_SETMASTER get Admin privilege.
    pub admin_password: Option<CubeString>,
    /// Users that may authenticate via N_AUTHTRY / N_AUTHANS ECC auth.
    pub auth_users: Vec<AuthUser>,
}

impl BasePluginOptions {
    pub fn new() -> Self {
        Self {
            server_desc: CubeString::from_str_lossy("Rust Server"),
            server_auth: CubeString::new(),
            password: None,
            admin_password: None,
            auth_users: Vec::new(),
        }
    }
}

impl Default for BasePluginOptions {
    fn default() -> Self {
        Self::new()
    }
}
