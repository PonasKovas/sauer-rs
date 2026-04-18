use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
pub struct LuaPluginOptions {
    pub plugin_directory: PathBuf,
}

impl LuaPluginOptions {
    pub fn new() -> Self {
        Self {
            plugin_directory: PathBuf::from("plugins"),
        }
    }
}

impl Default for LuaPluginOptions {
    fn default() -> Self {
        Self::new()
    }
}
