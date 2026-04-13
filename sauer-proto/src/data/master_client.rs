use num_enum::TryFromPrimitive;

/// Represents a master/admin client configuration broadcast to everyone.
#[derive(Debug, Clone, PartialEq)]
pub struct MasterClient {
    pub client_num: i32,
    pub privilege: Privilege,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, TryFromPrimitive)]
#[repr(i32)]
pub enum Privilege {
    None = 0,
    Auth = 1,
    Master = 2,
    Admin = 3,
}
