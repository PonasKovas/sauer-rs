use cube_string::CubeString;

pub struct BasePluginOptions {
    /// Displayed in the server browser.
    pub server_desc: CubeString,
    /// Auth domain for server authentication (leave empty to disable).
    pub server_auth: CubeString,
    /// Whether the server has a password set.
    pub has_password: bool,
}

impl BasePluginOptions {
    pub fn new() -> Self {
        Self {
            server_desc: CubeString::from_str_lossy("Rust Server"),
            server_auth: CubeString::new(),
            has_password: false,
        }
    }
}

impl Default for BasePluginOptions {
    fn default() -> Self {
        Self::new()
    }
}
