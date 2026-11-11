use crate::{
    ParseError,
    data::{Cn, Vec3, vec3},
    internal::{SauerRead, SauerWrite, constants::DMF},
};

/// Full state of a flag, as `N_INITFLAGS` sends it.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FlagState {
    pub version: i32,
    pub spawn_index: i32,
    pub location: FlagLocation,
    pub invis: bool,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FlagLocation {
    /// At its spawn.
    Home,
    Carried {
        owner: Cn,
    },
    Dropped {
        pos: Vec3<f32>,
    },
}

/// Which team holds a flag in `M_HOLD` modes.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FlagStateHoldTeam {
    None,
    Team { team: i32, hold_time: i32 },
}

/// The flags in `N_INITFLAGS`. Hold modes add a hold team to every flag,
/// so the variant must match the game mode or the client misreads the rest.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum InitFlags {
    Plain(Vec<FlagState>),
    Hold(Vec<(FlagState, FlagStateHoldTeam)>),
}

impl InitFlags {
    pub fn len(&self) -> usize {
        match self {
            InitFlags::Plain(f) => f.len(),
            InitFlags::Hold(f) => f.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl SauerWrite for FlagState {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        self.version.sauer_write(out);
        self.spawn_index.sauer_write(out);
        // Owner (-1 if not carried), invis, then whether and where it lies dropped.
        let owner = match self.location {
            FlagLocation::Carried { owner } => Some(owner),
            _ => None,
        };
        owner.sauer_write(out);
        self.invis.sauer_write(out);
        match &self.location {
            FlagLocation::Carried { .. } => {}
            FlagLocation::Home => false.sauer_write(out),
            FlagLocation::Dropped { pos } => {
                true.sauer_write(out);
                vec3::write_quantized::<DMF>(pos, out);
            }
        }
    }
}

impl SauerRead for FlagState {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        let version = i32::sauer_read(reader)?;
        let spawn_index = i32::sauer_read(reader)?;
        let owner = Option::<Cn>::sauer_read(reader)?;
        let invis = bool::sauer_read(reader)?;
        let location = match owner {
            Some(owner) => FlagLocation::Carried { owner },
            None if bool::sauer_read(reader)? => FlagLocation::Dropped {
                pos: vec3::read_quantized::<DMF>(reader)?,
            },
            None => FlagLocation::Home,
        };
        Ok(Self {
            version,
            spawn_index,
            location,
            invis,
        })
    }
}

impl SauerWrite for FlagStateHoldTeam {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        match self {
            FlagStateHoldTeam::None => (-1i32).sauer_write(out),
            FlagStateHoldTeam::Team { team, hold_time } => {
                team.sauer_write(out);
                hold_time.sauer_write(out);
            }
        }
    }
}

impl SauerRead for FlagStateHoldTeam {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        let team = i32::sauer_read(reader)?;
        if team < 0 {
            return Ok(FlagStateHoldTeam::None);
        }
        Ok(FlagStateHoldTeam::Team {
            team,
            hold_time: i32::sauer_read(reader)?,
        })
    }
}

#[cfg(feature = "lua")]
const _: () = {
    use mlua::{FromLua, IntoLua, Lua, Table, Value};

    /// A table with `version`, `spawn_index` and `invis`. The location is
    /// `owner` when carried, `drop_loc` when dropped and neither when home.
    impl IntoLua for FlagState {
        fn into_lua(self, lua: &Lua) -> mlua::Result<Value> {
            let table = lua.create_table()?;
            table.set("version", self.version)?;
            table.set("spawn_index", self.spawn_index)?;
            table.set("invis", self.invis)?;
            match self.location {
                FlagLocation::Home => {}
                FlagLocation::Carried { owner } => table.set("owner", owner)?,
                FlagLocation::Dropped { pos } => table.set("drop_loc", pos)?,
            }
            Ok(Value::Table(table))
        }
    }

    impl FromLua for FlagState {
        fn from_lua(value: Value, lua: &Lua) -> mlua::Result<Self> {
            let table = Table::from_lua(value, lua)?;
            let owner: Option<Cn> = table.get("owner")?;
            let drop_loc: Option<Vec3<f32>> = table.get("drop_loc")?;
            let location = match (owner, drop_loc) {
                (Some(owner), _) => FlagLocation::Carried { owner },
                (None, Some(pos)) => FlagLocation::Dropped { pos },
                (None, None) => FlagLocation::Home,
            };
            Ok(Self {
                version: table.get("version")?,
                spawn_index: table.get("spawn_index")?,
                location,
                invis: table.get::<Option<bool>>("invis")?.unwrap_or(false),
            })
        }
    }

    /// A list of flag tables. In hold modes each also has `hold`:
    /// `{ team, hold_time }`, or `{}` for no team.
    impl IntoLua for InitFlags {
        fn into_lua(self, lua: &Lua) -> mlua::Result<Value> {
            let list = lua.create_table()?;
            match self {
                InitFlags::Plain(flags) => {
                    for f in flags {
                        list.raw_push(f)?;
                    }
                }
                InitFlags::Hold(flags) => {
                    for (f, hold) in flags {
                        let Value::Table(t) = f.into_lua(lua)? else {
                            unreachable!("FlagState is a table")
                        };
                        let hold_table = lua.create_table()?;
                        if let FlagStateHoldTeam::Team { team, hold_time } = hold {
                            hold_table.set("team", team)?;
                            hold_table.set("hold_time", hold_time)?;
                        }
                        t.set("hold", hold_table)?;
                        list.raw_push(t)?;
                    }
                }
            }
            Ok(Value::Table(list))
        }
    }

    /// Hold mode if the flags have `hold`; all of them or none must.
    impl FromLua for InitFlags {
        fn from_lua(value: Value, lua: &Lua) -> mlua::Result<Self> {
            let tables: Vec<Table> = Vec::from_lua(value, lua)?;
            let holds = tables
                .iter()
                .map(|t| t.get::<Option<Table>>("hold"))
                .collect::<mlua::Result<Vec<_>>>()?;
            let flags = tables
                .into_iter()
                .map(|t| FlagState::from_lua(Value::Table(t), lua))
                .collect::<mlua::Result<Vec<_>>>()?;
            if holds.iter().all(Option::is_none) {
                return Ok(InitFlags::Plain(flags));
            }
            let holds = holds
                .into_iter()
                .map(|h| {
                    let h = h.ok_or_else(|| {
                        mlua::Error::runtime("either every flag has `hold` or none does")
                    })?;
                    Ok(match h.get::<Option<i32>>("team")? {
                        None => FlagStateHoldTeam::None,
                        Some(team) => FlagStateHoldTeam::Team {
                            team,
                            hold_time: h.get("hold_time")?,
                        },
                    })
                })
                .collect::<mlua::Result<Vec<_>>>()?;
            Ok(InitFlags::Hold(flags.into_iter().zip(holds).collect()))
        }
    }
};
