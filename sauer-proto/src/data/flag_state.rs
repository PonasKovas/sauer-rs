use crate::data::Vec3;

/// Full state of a flag, as `N_INITFLAGS` sends it.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FlagState {
    pub version: i32,
    pub spawn_index: i32,
    pub location: FlagLocation,
    pub invis: bool,
    /// Must only be `Some` if the gamemode is `M_HOLD`!
    ///
    /// Client will fail to parse it otherwise and the stream will corrupt!
    pub hold: Option<FlagStateHoldTeam>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FlagLocation {
    /// At its spawn.
    Home,
    Carried {
        owner: i32,
    },
    Dropped {
        pos: Vec3<f32>,
    },
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FlagStateHoldTeam {
    None,
    Team { team: i32, hold_time: i32 },
}

#[cfg(feature = "lua")]
const _: () = {
    use mlua::{FromLua, IntoLua, Lua, Table, Value};

    /// A table with `version`, `spawn_index` and `invis`. The location is
    /// `owner` when carried, `drop_loc` when dropped and neither when home.
    /// `hold` is only there in `M_HOLD`: `{ team, hold_time }`, or `{}` for
    /// no team.
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
            if let Some(hold) = self.hold {
                let hold_table = lua.create_table()?;
                if let FlagStateHoldTeam::Team { team, hold_time } = hold {
                    hold_table.set("team", team)?;
                    hold_table.set("hold_time", hold_time)?;
                }
                table.set("hold", hold_table)?;
            }
            Ok(Value::Table(table))
        }
    }

    impl FromLua for FlagState {
        fn from_lua(value: Value, lua: &Lua) -> mlua::Result<Self> {
            let table = Table::from_lua(value, lua)?;
            let owner: Option<i32> = table.get("owner")?;
            let drop_loc: Option<Vec3<f32>> = table.get("drop_loc")?;
            let location = match (owner, drop_loc) {
                (Some(owner), _) => FlagLocation::Carried { owner },
                (None, Some(pos)) => FlagLocation::Dropped { pos },
                (None, None) => FlagLocation::Home,
            };
            let hold = match table.get::<Option<Table>>("hold")? {
                None => None,
                Some(hold) => Some(match hold.get::<Option<i32>>("team")? {
                    None => FlagStateHoldTeam::None,
                    Some(team) => FlagStateHoldTeam::Team {
                        team,
                        hold_time: hold.get("hold_time")?,
                    },
                }),
            };
            Ok(Self {
                version: table.get("version")?,
                spawn_index: table.get("spawn_index")?,
                location,
                invis: table.get::<Option<bool>>("invis")?.unwrap_or(false),
                hold,
            })
        }
    }
};
