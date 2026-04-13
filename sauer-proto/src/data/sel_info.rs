use crate::{
    ParseError,
    data::Vector3,
    internal::{ReaderCore, WriterCore},
};

/// Cube selection information for cooperative editing geometry modifications.
#[derive(Debug, Clone, PartialEq)]
pub struct SelInfo {
    pub o: Vector3<i32>,
    pub s: Vector3<i32>,
    pub grid: i32,
    pub orient: i32,
    pub cx: i32,
    pub cxs: i32,
    pub cy: i32,
    pub cys: i32,
    pub corner: i32,
}

impl SelInfo {
    pub(crate) fn decode(r: &mut ReaderCore) -> Result<Self, ParseError> {
        Ok(SelInfo {
            o: Vector3 {
                x: r.get_int()?,
                y: r.get_int()?,
                z: r.get_int()?,
            },
            s: Vector3 {
                x: r.get_int()?,
                y: r.get_int()?,
                z: r.get_int()?,
            },
            grid: r.get_int()?,
            orient: r.get_int()?,
            cx: r.get_int()?,
            cxs: r.get_int()?,
            cy: r.get_int()?,
            cys: r.get_int()?,
            corner: r.get_int()?,
        })
    }

    pub(crate) fn encode(&self, w: &mut WriterCore) {
        w.put_int(self.o.x);
        w.put_int(self.o.y);
        w.put_int(self.o.z);
        w.put_int(self.s.x);
        w.put_int(self.s.y);
        w.put_int(self.s.z);
        w.put_int(self.grid);
        w.put_int(self.orient);
        w.put_int(self.cx);
        w.put_int(self.cxs);
        w.put_int(self.cy);
        w.put_int(self.cys);
        w.put_int(self.corner);
    }
}
