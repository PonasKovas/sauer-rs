use cube_string::CubeString;

/// Stores the number of frags a particular team has.
#[derive(Debug, Clone, PartialEq)]
pub struct TeamInfo {
    pub team: CubeString,
    pub frags: i32,
}
