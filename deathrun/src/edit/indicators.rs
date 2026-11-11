//! What editors see around them: the particle that shows a trap is being edited.

use super::DeathrunEdit;
use crate::aim::Aim;
use crate::map_state::get_server_state;
use sauer_proto::data::{Direction, EntityType};
use sauer_server::Server;

const TRAP_EDIT_PARTICLE_ATTRS: [i32; 5] = [34, 100, 100, 0, 0];
const TRAP_EDIT_PARTICLE_YAW_OFFSET: f32 = 30.0;
const TRAP_EDIT_PARTICLE_PITCH_OFFSET: f32 = 0.0;
const TRAP_EDIT_PARTICLE_MAX_DIST: f32 = 1000.0;

impl DeathrunEdit {
    /// The particle beside the view of everyone while a trap is being edited,
    /// so they see their edits go into the trap. Moves at most every 100 ms.
    pub(super) fn update_edit_particle(
        &self,
        server: &Server,
        id: u64,
        aim: &Aim,
        dir: Direction,
        editing: bool,
    ) {
        if !editing {
            if self.last_particle_update.remove(&id).is_some() {
                let mut state = get_server_state(server).write();
                state
                    .virt
                    .clear(id, "edit_particle")
                    .send(server, id, crate::SERVER_BOT_CN);
            }
            return;
        }
        let now = tokio::time::Instant::now();
        match self.last_particle_update.get_mut(&id) {
            Some(last) if now.duration_since(*last).as_millis() < 100 => return,
            Some(mut last) => *last = now,
            None => {
                self.last_particle_update.insert(id, now);
            }
        }

        // Off to the side of the view ray, a bit in front of whatever is there.
        let [f, r, u] = crate::aim::view_basis(dir.yaw as f32, dir.pitch as f32);
        let local = crate::aim::forward(
            TRAP_EDIT_PARTICLE_YAW_OFFSET,
            TRAP_EDIT_PARTICLE_PITCH_OFFSET,
        );
        let dir: [f32; 3] =
            std::array::from_fn(|i| r[i] * local[0] + f[i] * local[1] + u[i] * local[2]);
        let side = Aim {
            origin: aim.origin,
            forward: dir,
        };
        let dist = {
            let state = get_server_state(server).read();
            sauer_map::raycast::raycast_with_filter(
                &state.map,
                side.origin,
                side.forward,
                TRAP_EDIT_PARTICLE_MAX_DIST,
                |c| !c.is_empty() || c.material != sauer_core::Material::default(),
            )
            .map_or(TRAP_EDIT_PARTICLE_MAX_DIST, |hit| (hit.dist - 1.0).max(1.0))
        };
        let particle = sauer_kit::virt_ents::FakeEnt {
            ent_type: EntityType::Particles,
            position: side.at(dist),
            attrs: TRAP_EDIT_PARTICLE_ATTRS,
        };
        let mut state = get_server_state(server).write();
        state
            .show_virt(id, "edit_particle", vec![particle])
            .send(server, id, crate::SERVER_BOT_CN);
    }
}
