#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
pub struct BasePluginOptions {
    //
}

impl BasePluginOptions {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for BasePluginOptions {
    fn default() -> Self {
        Self::new()
    }
}
