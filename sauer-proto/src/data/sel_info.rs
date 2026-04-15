use crate::{
    data::{Orient, Vector3},
    internal::{DecodeCore, EncodeCore, ReaderCore, WriterCore},
    ParseError,
};

/// Cube selection information for cooperative editing geometry modifications.
#[derive(Debug, Clone, PartialEq)]
pub struct SelInfo {
    pub o: Vector3<i32>,
    pub s: Vector3<i32>,
    pub grid: i32,
    pub orient: Orient,
    pub cx: i32,
    pub cxs: i32,
    pub cy: i32,
    pub cys: i32,
    pub corner: i32,
}

impl DecodeCore for SelInfo {
    fn decode(r: &mut ReaderCore) -> Result<Self, ParseError> {
        let o = Vector3 {
            x: r.get_int()?,
            y: r.get_int()?,
            z: r.get_int()?,
        };
        let s = Vector3 {
            x: r.get_int()?,
            y: r.get_int()?,
            z: r.get_int()?,
        };
        let grid = r.get_int()?;
        let orient_val = r.get_int()?;
        let orient = Orient::try_from(orient_val).map_err(|_| ParseError::InvalidEnumValue {
            value: orient_val,
            enum_name: "Orient",
        })?;
        Ok(SelInfo {
            o,
            s,
            grid,
            orient,
            cx: r.get_int()?,
            cxs: r.get_int()?,
            cy: r.get_int()?,
            cys: r.get_int()?,
            corner: r.get_int()?,
        })
    }
}

impl EncodeCore for SelInfo {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(self.o.x);
        w.put_int(self.o.y);
        w.put_int(self.o.z);
        w.put_int(self.s.x);
        w.put_int(self.s.y);
        w.put_int(self.s.z);
        w.put_int(self.grid);
        w.put_int(self.orient as i32);
        w.put_int(self.cx);
        w.put_int(self.cxs);
        w.put_int(self.cy);
        w.put_int(self.cys);
        w.put_int(self.corner);
    }
}
