//! Animation evaluation and CPU skinning.
//!
//! Runs after model realization so skinned models have a GPU buffer, and
//! before scene extraction so the renderer sees this frame's vertices.

use std::sync::Arc;

use cgmath::{InnerSpace, Matrix4, Point3, SquareMatrix};
use ignit_ecs::World;
use ignit_ecs_bridge::{
    AnimationPlayer, DeviceResource, LightDescriptor, LightSlotIndex, ModelRealization, Position,
    QueueResource, SkeletonComponent, SkinningMode,
};
use ignit_light::LightType;
use ignit_shadow::ShadowCaster;

/// Animation layers for the current clip and an optional fade-out layer.
fn animation_layers(player: &AnimationPlayer) -> Vec<ignit_animation::AnimationLayer<'_>> {
    match &player.blend {
        Some(blend) => vec![
            ignit_animation::AnimationLayer {
                clip: &player.clip,
                time: player.time,
                weight: blend.progress,
            },
            ignit_animation::AnimationLayer {
                clip: &blend.clip,
                time: blend.time,
                weight: 1.0 - blend.progress,
            },
        ],
        None => vec![ignit_animation::AnimationLayer {
            clip: &player.clip,
            time: player.time,
            weight: 1.0,
        }],
    }
}

/// Advances animation players and deforms skinned vertices.
///
/// Returns the shadow slots of every light whose influence contains an
/// animated model. Those maps must refresh even when the rest of the scene is
/// static, otherwise the shadow keeps the pose it had when the light last
/// changed.
pub fn sys_animate(ecs: &World, delta_time: f32) -> Vec<u32> {
    let (device, queue) = {
        let Some(device) = ecs.get_resource::<DeviceResource>() else {
            return Vec::new();
        };
        let Some(queue) = ecs.get_resource::<QueueResource>() else {
            return Vec::new();
        };
        (device.0.clone(), queue.0.clone())
    };

    let compute_skinning = ecs.get_resource::<crate::systems::skinning::ComputeSkinning>();
    let compute_available = compute_skinning.is_some();

    let mut updates = {
        let Some(players) = ecs.get_component_store::<AnimationPlayer>() else {
            return Vec::new();
        };
        let Some(skeletons) = ecs.get_component_store::<SkeletonComponent>() else {
            return Vec::new();
        };
        let Some(models) = ecs.get_component_store::<ModelRealization>() else {
            return Vec::new();
        };

        let mut updates = Vec::new();
        for &entity in players.dense.as_slice() {
            let Some(player_index) = players.sparse.get(entity).copied().flatten() else {
                continue;
            };
            let Some(skeleton_index) = skeletons.sparse.get(entity).copied().flatten() else {
                continue;
            };
            let Some(model_index) = models.sparse.get(entity).copied().flatten() else {
                continue;
            };

            let mut player = players.components[player_index].clone();
            let model = Arc::clone(&models.components[model_index].0);
            let Some(_skin) = model.skin() else {
                continue;
            };

            let mut skeleton = skeletons.components[skeleton_index].0.clone();
            player.advance(delta_time);
            skeleton.apply_clips(&animation_layers(&player));
            skeleton.update_world(Matrix4::identity());

            let use_compute = match player.skinning {
                SkinningMode::Auto => compute_available,
                SkinningMode::Cpu => false,
                SkinningMode::Compute => compute_available,
            };
            let vertices = if use_compute {
                Vec::new()
            } else {
                _skin.skinned_vertices(&model.source_mesh().vertices, skeleton.skin_matrices())
            };

            updates.push((
                player_index,
                skeleton_index,
                model_index,
                player,
                skeleton,
                use_compute,
                vertices,
            ));
        }

        updates
    };

    if updates.is_empty() {
        return Vec::new();
    }

    let mut animated_models = Vec::with_capacity(updates.len());
    let mut compute_models = Vec::new();
    {
        let Some(players) = ecs.get_component_store_mut::<AnimationPlayer>() else {
            return Vec::new();
        };
        let Some(skeletons) = ecs.get_component_store_mut::<SkeletonComponent>() else {
            return Vec::new();
        };
        let Some(models) = ecs.get_component_store_mut::<ModelRealization>() else {
            return Vec::new();
        };

        for (player_index, skeleton_index, model_index, player, skeleton, use_compute, vertices) in
            updates.drain(..)
        {
            animated_models.push(model_index);
            let skin_matrices = use_compute.then(|| skeleton.skin_matrices().to_vec());

            players.get_mut_store().components[player_index] = player;
            skeletons.get_mut_store().components[skeleton_index] = SkeletonComponent(skeleton);

            let model = Arc::make_mut(&mut models.get_mut_store().components[model_index].0);
            if let Some(matrices) = skin_matrices {
                model.update_skin_matrices(&matrices, &device, &queue);
                compute_models.push(model_index);
            } else {
                model.update_skinned_vertices(&vertices, &device, &queue);
            }
        }
    }

    if let Some(skinner) = compute_skinning
        && !compute_models.is_empty()
    {
        let Some(models) = ecs.get_component_store::<ModelRealization>() else {
            return Vec::new();
        };

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Ignit::SkinCompute"),
        });
        for model_index in compute_models {
            skinner.dispatch(&mut encoder, &device, &models.components[model_index].0);
        }
        queue.submit([encoder.finish()]);
    }

    animated_shadow_lights(ecs, &animated_models)
}

/// Shadow slots of the lights that can see any of the animated models.
fn animated_shadow_lights(ecs: &World, model_indices: &[usize]) -> Vec<u32> {
    let (Some(models), Some(lights), Some(positions), Some(casters), Some(slots)) = (
        ecs.get_component_store::<ModelRealization>(),
        ecs.get_component_store::<LightDescriptor>(),
        ecs.get_component_store::<Position>(),
        ecs.get_component_store::<ShadowCaster>(),
        ecs.get_component_store::<LightSlotIndex>(),
    ) else {
        return Vec::new();
    };

    let mut animated_bounds = Vec::new();
    for &model_index in model_indices {
        for bound in models.components[model_index].0.instance_bounds() {
            animated_bounds.push((Point3::new(bound[0], bound[1], bound[2]), bound[3]));
        }
    }
    if animated_bounds.is_empty() {
        return Vec::new();
    }

    let mut affected = Vec::new();
    for &entity in casters.dense.as_slice() {
        let Some(caster_index) = casters.sparse.get(entity).copied().flatten() else {
            continue;
        };
        if !casters.components[caster_index].enabled {
            continue;
        }
        let Some(light_index) = lights.sparse.get(entity).copied().flatten() else {
            continue;
        };
        let light = &lights.components[light_index];

        let affects_animation = match light.light_type {
            LightType::Directional { .. } => true,
            _ => {
                let Some(position_index) = positions.sparse.get(entity).copied().flatten() else {
                    continue;
                };
                let center = positions.components[position_index].0;
                animated_bounds.iter().any(|&(model_center, radius)| {
                    (model_center - center).magnitude() <= light.range + radius
                })
            }
        };

        if affects_animation && let Some(slot_index) = slots.sparse.get(entity).copied().flatten() {
            affected.push(slots.components[slot_index].0);
        }
    }

    affected
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, RwLock};

    use cgmath::{Vector2, Vector3};
    use ignit_animation::{
        AnimationChannel, AnimationClip, AnimationPath, AnimationSampler, AnimationValue,
        Interpolation, JointDescriptor, SkeletonDescriptor, SkinData,
    };
    use ignit_core::cache::Cache;
    use ignit_ecs_bridge::{
        DeviceResource, ModelInstances, ModelRealization, QueueResource, SkeletonComponent,
    };
    use ignit_material_shader::{MaterialShaderDescriptor, PipelineCache};
    use ignit_math::Transform;
    use ignit_mesh::{MeshDescriptor, Vertex};
    use ignit_model::{Model, ModelDescriptor};

    fn animated_world(compute: bool) -> World {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());

        let mesh = Arc::new(MeshDescriptor::new(
            vec![Vertex::new(
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(0.0, 1.0, 0.0),
                Vector3::new(1.0, 0.0, 0.0),
                Vector2::new(0.0, 0.0),
            )],
            vec![0, 0, 0],
        ));
        let material = Arc::new(MaterialShaderDescriptor::default());
        let skeleton_descriptor = Arc::new(SkeletonDescriptor {
            joints: vec![JointDescriptor {
                node_index: 0,
                name: "root".into(),
                parent: None,
                rest: ignit_math::Transform::identity(),
            }],
            inverse_bind_matrices: vec![Matrix4::identity()],
        });
        let clip = Arc::new(AnimationClip::new(
            "Move",
            vec![AnimationChannel {
                node_index: 0,
                path: AnimationPath::Translation,
                sampler: AnimationSampler::new(
                    vec![0.0, 1.0],
                    vec![
                        AnimationValue::Vec3(Vector3::new(0.0, 0.0, 0.0)),
                        AnimationValue::Vec3(Vector3::new(1.0, 0.0, 0.0)),
                    ],
                    Interpolation::Linear,
                ),
            }],
        ));
        let mut instances = ModelInstances::new();
        instances.add_instance(Transform::identity());

        let descriptor = ModelDescriptor {
            label: "Animated".into(),
            mesh,
            materials: vec![material],
            transforms: instances.0,
            skin: Some(Arc::new(SkinData::new(
                vec![[0; 4]],
                vec![[1.0, 0.0, 0.0, 0.0]],
            ))),
            skeleton: Some(skeleton_descriptor.clone()),
        };

        let mesh_cache = RwLock::new(Cache::default());
        let material_cache = RwLock::new(Cache::default());
        let pipeline_cache = Mutex::new(PipelineCache::default());
        let model = Model::from_descriptor(
            &descriptor,
            &ignit_renderer::SCENE_COLOR_FORMAT,
            &device,
            &queue,
            &mesh_cache,
            &material_cache,
            &pipeline_cache,
            None,
        )
        .expect("skinned model");

        let mut world = ignit_ecs::World::new();
        if compute {
            world.insert_resource(crate::systems::skinning::ComputeSkinning::new(&device));
        }
        world.insert_resource(DeviceResource(Arc::new(device)));
        world.insert_resource(QueueResource(Arc::new(queue)));
        let entity = world.spawn_entity();
        world
            .attach_component(&entity, ModelRealization(Arc::new(model)))
            .unwrap();
        world
            .attach_component(
                &entity,
                SkeletonComponent(ignit_animation::Skeleton::from_descriptor(
                    &skeleton_descriptor,
                )),
            )
            .unwrap();
        let mut player = AnimationPlayer::new(clip);
        player.skinning = if compute {
            SkinningMode::Compute
        } else {
            SkinningMode::Cpu
        };
        world.attach_component(&entity, player).unwrap();

        world
    }

    #[test]
    fn sys_animate_advances_players_and_skins_vertices() {
        let world = animated_world(false);

        sys_animate(&world, 0.5);

        let players = world.get_component_store::<AnimationPlayer>().unwrap();
        assert!((players.components[0].time - 0.5).abs() < 1e-6);
        let skeletons = world.get_component_store::<SkeletonComponent>().unwrap();
        assert!(
            (skeletons.components[0]
                .0
                .local_transform(0)
                .unwrap()
                .position
                .x
                - 0.5)
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn sys_animate_reports_lights_that_see_the_model() {
        let mut world = animated_world(false);

        let near = world.spawn_entity();
        world
            .attach_component(
                &near,
                LightDescriptor::new_point(Vector3::new(1.0, 1.0, 1.0), 10.0),
            )
            .unwrap();
        world
            .attach_component(&near, Position(Point3::new(0.0, 1.0, 0.0)))
            .unwrap();
        world
            .attach_component(&near, ShadowCaster::default())
            .unwrap();
        world.attach_component(&near, LightSlotIndex(7)).unwrap();

        let far = world.spawn_entity();
        world
            .attach_component(
                &far,
                LightDescriptor::new_point(Vector3::new(1.0, 1.0, 1.0), 10.0),
            )
            .unwrap();
        world
            .attach_component(&far, Position(Point3::new(100.0, 0.0, 0.0)))
            .unwrap();
        world
            .attach_component(&far, ShadowCaster::default())
            .unwrap();
        world.attach_component(&far, LightSlotIndex(8)).unwrap();

        let slots = sys_animate(&world, 0.5);

        assert_eq!(slots, vec![7]);
    }

    #[test]
    fn sys_animate_dispatches_compute_skinning() {
        let world = animated_world(true);

        sys_animate(&world, 0.5);

        let models = world.get_component_store::<ModelRealization>().unwrap();
        let model = &models.components[0].0;
        assert!(model.skin_matrix_buffer().is_some());
        assert!(model.compute_skinned_vertex_buffer().is_some());
        assert_eq!(model.skinned_vertex_count(), 1);
    }

    #[test]
    fn animation_layers_crossfade_the_outgoing_clip() {
        let incoming = Arc::new(AnimationClip::new("walk", Vec::new()));
        let outgoing = Arc::new(AnimationClip::new("idle", Vec::new()));
        let mut player = AnimationPlayer::new(incoming);
        player.blend = Some(ignit_ecs_bridge::AnimationBlend {
            clip: outgoing,
            time: 0.75,
            progress: 0.25,
            duration: 1.0,
        });

        let layers = animation_layers(&player);

        assert_eq!(layers.len(), 2);
        assert!((layers[0].weight - 0.25).abs() < 1e-6);
        assert!((layers[1].weight - 0.75).abs() < 1e-6);
        assert!((layers[1].time - 0.75).abs() < 1e-6);
    }
}
