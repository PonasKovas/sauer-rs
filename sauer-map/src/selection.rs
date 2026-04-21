#[derive(Debug, Clone)]
pub struct Selection {
    pub o: [i32; 3],
    pub s: [i32; 3],
    pub grid: i32,
    pub orient: u8,
    pub cx: i32,
    pub cxs: i32,
    pub cy: i32,
    pub cys: i32,
    pub corner: i32,
}

impl Selection {
    pub fn bounds(&self) -> ([i32; 3], [i32; 3]) {
        let min = self.o;
        let max = [
            self.o[0] + self.s[0] * self.grid,
            self.o[1] + self.s[1] * self.grid,
            self.o[2] + self.s[2] * self.grid,
        ];
        (min, max)
    }
}
