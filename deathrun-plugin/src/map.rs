use ogz_rs::OgzMap;
use sauer_map::World;
use sauer_server::Server;
use std::sync::RwLock;

/// All world-state the deathrun plugin tracks.
///
/// `world` is the live-edited geometry (octree, textures, materials).
/// `meta`  is the rest of the parsed OgzMap (config, vslots, entities,
/// lightmaps, pvs …).  When we need to send an OGZ file we swap the meta's
/// octree for `world.to_ogz_octree()` and serialize.
pub struct MapState {
    pub world: World,
    pub meta: OgzMap,
}

impl MapState {
    /// Build a `MapState` by parsing raw OGZ bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, ogz_rs::OgzError> {
        let ogz = OgzMap::parse(data)?;
        let world = World::from_ogz(&ogz);
        Ok(Self { world, meta: ogz })
    }

    /// Serialize the current `World` state into an OGZ byte stream.
    ///
    /// The resulting file has the same config/vslots/entities/lightmaps as
    /// the original upload but with the octree rebuilt from the tracked
    /// `World`, reflecting all texture, material and geometry edits applied
    /// since the map was uploaded.
    pub fn to_ogz_bytes(&self) -> Result<Vec<u8>, ogz_rs::OgzError> {
        let mut ogz = self.meta.clone();
        ogz.octree = self.world.to_ogz_octree();
        ogz.serialize()
    }
}

pub(crate) fn get_map_state(server: &Server) -> &RwLock<Option<MapState>> {
    server.extra.get().expect("MapState not initialised")
}
