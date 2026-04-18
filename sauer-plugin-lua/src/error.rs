use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LuaPluginLoadError {
    #[error("while processing {path}: {e}")]
    InPath {
        e: Box<LuaPluginLoadError>,
        path: PathBuf,
    },

    #[error(transparent)]
    ProcessingFile(#[from] std::io::Error),

    #[error("lua error: {0}")]
    LuaError(#[from] mlua::Error),

    #[error("lua error while processing {name}: {e}")]
    LuaErrorInPlugin { e: mlua::Error, name: String },

    #[error("file system path is not valid utf-8")]
    NonUtf8Path,
}

pub(crate) trait LuaPluginErrorExt<T> {
    fn in_path(self, path: &Path) -> Result<T, LuaPluginLoadError>;
}
impl<T, E: Into<LuaPluginLoadError>> LuaPluginErrorExt<T> for Result<T, E> {
    fn in_path(self, path: &Path) -> Result<T, LuaPluginLoadError> {
        match self {
            Ok(x) => Ok(x),
            Err(e) => Err(LuaPluginLoadError::InPath {
                e: Box::new(e.into()),
                path: path.to_owned(),
            }),
        }
    }
}
