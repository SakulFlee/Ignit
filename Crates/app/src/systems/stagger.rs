use std::collections::HashSet;

use ignit_ecs::World;
use ignit_ecs_bridge::{LightSlotIndex, ShadowDirtyFlag, StaggeredLightConfig};
use ignit_shadow::ShadowCaster;

/// Build the set of light store indices whose shadow maps should be
/// rendered this frame.
///
/// Only dirty casters are rendered; a static scene with no dirty lights or
/// models queues no shadow work at all. Up to `max_updates_per_frame` dirty
/// casters are refreshed per frame, and the pending ones stay dirty for the
/// next frame.
///
/// If `global_dirty` is true (model or scene change), all shadow-casting
/// lights are included regardless of the budget. `new_light_bootstrap` does
/// the same when lights were added this frame.
pub fn sys_stagger_shadow_updates(
    ecs: &mut World,
    global_dirty: bool,
    new_light_bootstrap: bool,
) -> HashSet<u32> {
    let config = ecs
        .get_resource::<StaggeredLightConfig>()
        .map(|c| c.max_updates_per_frame)
        .unwrap_or(1)
        .max(1) as usize;

    let mut result = HashSet::new();

    // Collect all shadow-casting entities and their slot indices.
    // Falls back to sequential indexing if LightSlotIndex store is absent.
    let casters = match ecs.get_component_store::<ShadowCaster>() {
        Some(s) => s,
        None => return result,
    };
    let slot_store = ecs.get_component_store::<LightSlotIndex>();
    let has_slot_store = slot_store.is_some();

    // Build (entity_id, slot_index_or_seq) for each shadow-casting light
    let mut shadow_entities: Vec<(usize, u32)> = Vec::new();
    {
        let mut seq_counter = 0u32;
        let descs = ecs.get_component_store::<ignit_ecs_bridge::LightDescriptor>();
        if has_slot_store {
            // Use LightSlotIndex components
            if let Some(ref ss) = slot_store {
                for &eid in casters.dense.as_slice() {
                    if let Some(c_idx) = casters.sparse.get(eid).copied().flatten() {
                        if !casters.components[c_idx].enabled {
                            continue;
                        }
                        if let Some(s_idx) = ss.sparse.get(eid).copied().flatten() {
                            shadow_entities.push((eid, ss.components[s_idx].0));
                        }
                    }
                }
            }
        } else {
            // Fallback: sequential counter from LightDescriptor iteration order
            if let Some(ref ds) = descs {
                for &eid in ds.dense.as_slice() {
                    let has_desc = ds.sparse.get(eid).copied().flatten().is_some();
                    let has_shadow = casters
                        .sparse
                        .get(eid)
                        .copied()
                        .flatten()
                        .map(|ci| casters.components[ci].enabled)
                        .unwrap_or(false);
                    if has_desc && has_shadow {
                        shadow_entities.push((eid, seq_counter));
                    }
                    if has_desc {
                        seq_counter += 1;
                    }
                }
            }
        }
    }

    // Collect dirty entity IDs
    let dirty_eids: Vec<usize> = match ecs.get_component_store::<ShadowDirtyFlag>() {
        Some(ref ds) => ds
            .dense
            .iter()
            .copied()
            .filter(|&eid| {
                ds.sparse
                    .get(eid)
                    .copied()
                    .flatten()
                    .map(|idx| ds.components[idx].is_dirty())
                    .unwrap_or(false)
            })
            .collect(),
        None => Vec::new(),
    };

    // --- Global dirty (model change) or new-light bootstrap → all shadows ---
    if global_dirty || new_light_bootstrap {
        for (_, slot) in &shadow_entities {
            result.insert(*slot);
        }
        if let Some(mut store) = ecs.get_component_store_mut::<ShadowDirtyFlag>() {
            for &eid in &dirty_eids {
                if let Some(idx) = store.sparse.get(eid).copied().flatten() {
                    store.components[idx].clear();
                }
            }
        }
        return result;
    }

    // A static scene has no shadow work to do. Previously this path
    // round-robined clean casters and re-rendered their maps every frame.
    if dirty_eids.is_empty() {
        return result;
    }

    // Apply the dirty-only update budget. Casters left dirty are handled on
    // following frames.
    let mut budget = config;
    let mut processed: HashSet<usize> = HashSet::new();
    for &eid in &dirty_eids {
        if budget == 0 {
            break;
        }
        let slot = if has_slot_store {
            ecs.get_component_store::<LightSlotIndex>()
                .and_then(|ss| ss.get_component(eid).copied())
                .map(|si| si.0)
        } else {
            shadow_entities
                .iter()
                .find(|(e, _)| *e == eid)
                .map(|(_, s)| *s)
        };
        if let Some(slot) = slot {
            result.insert(slot);
            processed.insert(eid);
            budget -= 1;
        }
    }

    // Clear ShadowDirtyFlag for processed entities
    if let Some(mut store) = ecs.get_component_store_mut::<ShadowDirtyFlag>() {
        for &eid in &processed {
            if let Some(idx) = store.sparse.get(eid).copied().flatten() {
                store.components[idx].clear();
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use cgmath::Vector3;
    use ignit_ecs_bridge::LightDescriptor;

    fn caster_world(dirty: bool) -> World {
        let mut world = World::new();
        let entity = world.spawn_entity();
        world
            .attach_component(
                &entity,
                LightDescriptor::new_point(Vector3::new(1.0, 1.0, 1.0), 20.0),
            )
            .unwrap();
        world
            .attach_component(&entity, ShadowCaster::default())
            .unwrap();
        world.attach_component(&entity, LightSlotIndex(0)).unwrap();
        world
            .attach_component(&entity, ShadowDirtyFlag(dirty))
            .unwrap();
        world
    }

    #[test]
    fn a_static_scene_queues_no_shadow_updates() {
        let mut world = caster_world(false);

        let updates = sys_stagger_shadow_updates(&mut world, false, false);

        assert!(updates.is_empty());
    }

    #[test]
    fn dirty_casters_are_updated_and_cleared() {
        let mut world = caster_world(true);

        let updates = sys_stagger_shadow_updates(&mut world, false, false);

        assert!(updates.contains(&0));
        let store = world.get_component_store::<ShadowDirtyFlag>().unwrap();
        assert!(!store.components[0].is_dirty());
    }
}
