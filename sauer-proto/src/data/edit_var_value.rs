use cube_string::CubeString;

/// Strict typing for variable modification in cooperative editing (`N_EDITVAR`).
#[derive(Debug, Clone, PartialEq)]
pub enum EditVarValue {
    Int(i32),
    Float(f32),
    String(CubeString),
}
