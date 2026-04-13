use cube_string::CubeString;

#[derive(Debug, Clone, PartialEq)]
pub struct BaseInfo {
    pub ammotype: i32,
    pub owner: CubeString,
    pub enemy: CubeString,
    pub converted: i32,
    pub ammo: i32,
}
