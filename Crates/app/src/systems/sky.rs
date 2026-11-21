//! Time-of-day animation for dynamic generated skies.

use ignit_ecs::{ComponentAccess, FunctionSystem, FunctionSystemMetadata, System, World};
use ignit_ecs_bridge::{
    DeltaTime, EnvironmentDescriptorLast, EnvironmentDescriptorResource, SkyClock,
};

/// Full sky regeneration is expensive, so the descriptor changes at ~30 Hz.
const UPDATE_INTERVAL: f32 = 1.0 / 30.0;

/// Builds the sky-clock system with its resource access declaration.
pub fn sky_system() -> Box<dyn System> {
    let mut accumulator = UPDATE_INTERVAL;
    Box::new(FunctionSystem::new(
        FunctionSystemMetadata {
            name: "sys_animate_sky",
            access: ComponentAccess::new()
                .reads::<DeltaTime>()
                .reads::<EnvironmentDescriptorLast>()
                .writes::<SkyClock>()
                .writes::<EnvironmentDescriptorResource>(),
        },
        Box::new(move |world, _commands| step(world, &mut accumulator)),
    ))
}

fn step(world: &World, accumulator: &mut f32) {
    let Some(delta) = world.get_resource::<DeltaTime>() else {
        return;
    };
    let Some(mut clock) = world.get_resource_mut::<SkyClock>() else {
        return;
    };
    let Some(last) = world.get_resource::<EnvironmentDescriptorLast>() else {
        return;
    };
    if last.dynamic_sky_time().is_none() {
        return;
    }

    if !clock.paused {
        clock.hours = (clock.hours + delta.0 as f32 * clock.speed).rem_euclid(24.0);
    }

    *accumulator += delta.0 as f32;
    if *accumulator < UPDATE_INTERVAL {
        return;
    }
    *accumulator %= UPDATE_INTERVAL;

    let (hours, azimuth) = (clock.hours, clock.azimuth);
    drop(clock);

    let Some(descriptor) = last.with_dynamic_sky_time(hours, azimuth) else {
        return;
    };
    if let Some(mut queued) = world.get_resource_mut::<EnvironmentDescriptorResource>() {
        queued.0 = Some(descriptor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ignit_world_environment::{
        GeneratedSkyParameters, SunPosition, WorldEnvironmentDescriptor,
    };

    fn dynamic_sky(hours: f32) -> EnvironmentDescriptorLast {
        EnvironmentDescriptorLast(Some(WorldEnvironmentDescriptor::Generated {
            cube_face_size: 32,
            sampling_type: WorldEnvironmentDescriptor::DEFAULT_SAMPLING_TYPE,
            custom_specular_mip_level_count: None,
            parameters: Some(GeneratedSkyParameters {
                sun_position: SunPosition::TimeOfDay { hours },
                sun_azimuth: 0.0,
                ..GeneratedSkyParameters::default()
            }),
            dynamic: true,
        }))
    }

    fn queued_sky(world: &World) -> (f32, f32) {
        let queued = world
            .get_resource::<EnvironmentDescriptorResource>()
            .expect("descriptor resource");
        let queued = EnvironmentDescriptorLast(queued.0.clone());
        queued.dynamic_sky_time().expect("queued dynamic sky")
    }

    #[test]
    fn paused_clock_pushes_manual_values_without_advancing() {
        let mut world = World::new();
        world.insert_resource(DeltaTime(0.1));
        world.insert_resource(dynamic_sky(14.0));
        world.insert_resource(EnvironmentDescriptorResource(None));
        world.insert_resource(SkyClock {
            hours: 5.0,
            azimuth: 0.3,
            speed: 0.2,
            paused: true,
        });

        let mut accumulator = UPDATE_INTERVAL;
        step(&world, &mut accumulator);

        let (hours, azimuth) = queued_sky(&world);
        assert!((hours - 5.0).abs() < 1e-5);
        assert!((azimuth - 0.3).abs() < 1e-5);
    }

    #[test]
    fn unpaused_clock_advances_before_pushing() {
        let mut world = World::new();
        world.insert_resource(DeltaTime(1.0));
        world.insert_resource(dynamic_sky(14.0));
        world.insert_resource(EnvironmentDescriptorResource(None));
        world.insert_resource(SkyClock {
            hours: 14.0,
            speed: 0.2,
            ..SkyClock::default()
        });

        let mut accumulator = UPDATE_INTERVAL;
        step(&world, &mut accumulator);

        let (hours, _) = queued_sky(&world);
        assert!((hours - 14.2).abs() < 1e-5, "{hours}");
    }

    #[test]
    fn a_scene_without_a_clock_is_left_alone() {
        let mut world = World::new();
        world.insert_resource(DeltaTime(1.0));
        world.insert_resource(dynamic_sky(14.0));
        world.insert_resource(EnvironmentDescriptorResource(None));

        let mut accumulator = UPDATE_INTERVAL;
        step(&world, &mut accumulator);

        let queued = world
            .get_resource::<EnvironmentDescriptorResource>()
            .unwrap();
        assert!(queued.0.is_none());
    }
}
