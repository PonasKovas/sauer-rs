use ogz_rs::data::Material;

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Branch(Box<[Node; 8]>),
    Leaf(Cube),
}

#[derive(Debug, Clone, PartialEq, Copy)]
pub struct Cube {
    pub edges: [u8; 12],
    pub textures: [u16; 6],
    pub material: Material,
}

impl Cube {
    pub fn is_empty(&self) -> bool {
        self.edges == [0x00; 12]
    }

    pub fn is_entirely_solid(&self) -> bool {
        self.edges == [0x80; 12]
    }
}
