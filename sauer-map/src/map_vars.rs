//! Map vars as a client has them right after N_NEWMAP.
//!
//! The client resets every map var to its engine default, then runs
//! data/default_map_settings.cfg, which only sets the skybox. Values come
//! from the VARR/FVARR/HVARR/SVARR definitions in the Sauerbraten source.

use cube_string::IntoCubeStringLossy;
use ogz_rs::data::Variable;

enum Default {
    I(i32),
    F(f32),
    S(&'static str),
}
use Default::{F, I, S};

const DEFAULTS: &[(&str, Default)] = &[
    ("ambient", I(0x191919)),
    ("atmo", I(0)),
    ("atmoalpha", F(1.0)),
    ("atmobright", F(1.0)),
    ("atmodensity", F(1.0)),
    ("atmohaze", F(0.1)),
    ("atmoheight", F(1.0)),
    ("atmoozone", F(1.0)),
    ("atmoplanetsize", F(1.0)),
    ("atmosundisk", I(0)),
    ("atmosundiskbright", F(1.0)),
    ("atmosundiskcorona", F(0.4)),
    ("atmosundisksize", F(12.0)),
    ("atmosunlight", I(0)),
    ("atmosunlightscale", F(1.0)),
    ("blurlms", I(0)),
    ("blurskylight", I(0)),
    ("bumperror", I(3)),
    ("causticcontrast", F(0.6)),
    ("causticmillis", I(75)),
    ("causticscale", I(50)),
    ("cloudalpha", F(1.0)),
    ("cloudbox", S("")),
    ("cloudboxalpha", F(1.0)),
    ("cloudboxcolour", I(0xFFFFFF)),
    ("cloudclip", F(0.5)),
    ("cloudcolour", I(0xFFFFFF)),
    ("cloudfade", F(0.2)),
    ("cloudheight", F(0.2)),
    ("cloudlayer", S("")),
    ("cloudoffsetx", F(0.0)),
    ("cloudoffsety", F(0.0)),
    ("cloudscale", F(1.0)),
    ("cloudscrollx", F(0.0)),
    ("cloudscrolly", F(0.0)),
    ("cloudsubdiv", I(16)),
    ("envmapbb", I(0)),
    ("envmapradius", I(128)),
    ("fog", I(4000)),
    ("fogcolour", I(0x8099B3)),
    ("fogdomecap", I(1)),
    ("fogdomeclip", F(1.0)),
    ("fogdomeclouds", I(1)),
    ("fogdomecolour", I(0)),
    ("fogdomeheight", F(-0.5)),
    ("fogdomemax", F(0.0)),
    ("fogdomemin", F(0.0)),
    ("glass2colour", I(0x2080C0)),
    ("glass3colour", I(0x2080C0)),
    ("glass4colour", I(0x2080C0)),
    ("glasscolour", I(0x2080C0)),
    ("grassalpha", F(1.0)),
    ("grassanimmillis", I(3000)),
    ("grassanimscale", F(0.03)),
    ("grasscolour", I(0xFFFFFF)),
    ("grassscale", I(2)),
    ("lava2colour", I(0xFF4000)),
    ("lava2fog", I(50)),
    ("lava3colour", I(0xFF4000)),
    ("lava3fog", I(50)),
    ("lava4colour", I(0xFF4000)),
    ("lava4fog", I(50)),
    ("lavacolour", I(0xFF4000)),
    ("lavafog", I(50)),
    ("lerpangle", I(44)),
    ("lerpsubdiv", I(2)),
    ("lerpsubdivsize", I(4)),
    ("lighterror", I(8)),
    ("lightlod", I(0)),
    ("lightprecision", I(32)),
    ("maptitle", S("Untitled Map by Unknown")),
    ("minimapclip", I(0)),
    ("minimapcolour", I(0)),
    ("minimapheight", I(0)),
    ("refractclear", I(0)),
    ("refractsky", I(0)),
    ("shadowmapambient", I(0)),
    ("shadowmapangle", I(0)),
    ("skyboxcolour", I(0xFFFFFF)),
    ("skylight", I(0)),
    ("skytexture", I(1)),
    ("skytexturelight", I(1)),
    ("spincloudlayer", F(0.0)),
    ("spinclouds", F(0.0)),
    ("spinsky", F(0.0)),
    ("sunlight", I(0)),
    ("sunlightpitch", I(90)),
    ("sunlightscale", F(1.0)),
    ("sunlightyaw", I(0)),
    ("water2colour", I(0x144650)),
    ("water2fallcolour", I(0)),
    ("water2fog", I(150)),
    ("water2spec", I(150)),
    ("water3colour", I(0x144650)),
    ("water3fallcolour", I(0)),
    ("water3fog", I(150)),
    ("water3spec", I(150)),
    ("water4colour", I(0x144650)),
    ("water4fallcolour", I(0)),
    ("water4fog", I(150)),
    ("water4spec", I(150)),
    ("watercolour", I(0x144650)),
    ("waterfallcolour", I(0)),
    ("waterfog", I(150)),
    ("waterspec", I(150)),
    ("yawcloudlayer", I(0)),
    ("yawclouds", I(0)),
    ("yawsky", I(0)),
    ("skybox", S("skyboxes/remus/sky01")),
];

/// Read-only vars that a map file carries but the client never changes.
pub fn is_read_only(name: &str) -> bool {
    matches!(name, "mapversion" | "mapscale" | "mapsize")
}

pub fn new_map_vars() -> impl Iterator<Item = Variable> {
    DEFAULTS.iter().map(|(name, value)| {
        let name = name.lossy_cubestring();
        match value {
            I(value) => Variable::Int {
                name,
                value: *value,
            },
            F(value) => Variable::Float {
                name,
                value: *value,
            },
            S(value) => Variable::String {
                name,
                value: value.lossy_cubestring(),
            },
        }
    })
}
