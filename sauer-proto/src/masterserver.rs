#[derive(Debug, Clone, PartialEq)]
pub enum MasterClientRequest {
    RegServ(u16),
    ReqAuth(u32, String),
    ConfAuth(u32, String),
}

impl MasterClientRequest {
    pub fn to_string(&self) -> String {
        match self {
            Self::RegServ(port) => format!("regserv {}\n", port),
            Self::ReqAuth(id, name) => format!("reqauth {} {}\n", id, name),
            Self::ConfAuth(id, val) => format!("confauth {} {}\n", id, val),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MasterServerResponse {
    SuccReg,
    FailReg(String),
    ChalAuth(u32, String),
    SuccAuth(u32),
    FailAuth(u32),
    ClearGbans,
    AddGban(String),
}

impl MasterServerResponse {
    pub fn parse(line: &str) -> Option<Self> {
        let line = line.trim();
        if line == "succreg" {
            return Some(Self::SuccReg);
        }
        if line == "cleargbans" {
            return Some(Self::ClearGbans);
        }
        if let Some(rest) = line.strip_prefix("failreg ") {
            return Some(Self::FailReg(rest.to_string()));
        }
        if let Some(rest) = line.strip_prefix("chalauth ") {
            let mut parts = rest.split_whitespace();
            let id = parts.next()?.parse().ok()?;
            let chal = parts.next()?.to_string();
            return Some(Self::ChalAuth(id, chal));
        }
        if let Some(rest) = line.strip_prefix("succauth ") {
            let id = rest.trim().parse().ok()?;
            return Some(Self::SuccAuth(id));
        }
        if let Some(rest) = line.strip_prefix("failauth ") {
            let id = rest.trim().parse().ok()?;
            return Some(Self::FailAuth(id));
        }
        if let Some(rest) = line.strip_prefix("addgban ") {
            return Some(Self::AddGban(rest.trim().to_string()));
        }
        None
    }
}
