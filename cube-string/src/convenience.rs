use crate::CubeString;
use alloc::string::String;

/// Convenience trait for converting UTF-8 strings to CubeString, removing any unrepresentable characters.
pub trait IntoCubeStringLossy {
    /// Converts the string into CubeString in a lossy way
    fn lossy_cubestring(&self) -> CubeString;
}

impl IntoCubeStringLossy for &str {
    fn lossy_cubestring(&self) -> CubeString {
        CubeString::from_str_lossy(self)
    }
}
impl IntoCubeStringLossy for String {
    fn lossy_cubestring(&self) -> CubeString {
        CubeString::from_str_lossy(self)
    }
}
