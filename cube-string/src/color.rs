use crate::raw::COLOR_ESCAPE;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Color {
    Green = 0,
    Blue = 1,
    Yellow = 2,
    Red = 3,
    Gray = 4,
    Magenta = 5,
    Orange = 6,
    White = 7,
    Cyan = 8,
}

impl Color {
    pub fn escape_seq(self) -> [u8; 2] {
        [COLOR_ESCAPE, b'0' + self as u8]
    }
}
