//! Core engine schedule — built-in systems that run every frame.
//!
//! These systems handle engine-level bookkeeping: frame timing, input
//! snapshots, and other state that the runtime writes into the ECS world
//! before user systems execute.

use ignit_ecs::ResMut;
use ignit_ecs_bridge::{DeltaTime, FrameCounter, TotalTime};

/// Increments the total elapsed time each tick.
pub fn sys_accumulate_time(mut total: ResMut<TotalTime>, dt: ResMut<DeltaTime>) {
    total.0 += dt.0;
}

/// Increments the frame counter each tick.
pub fn sys_update_frame_counter(mut counter: ResMut<FrameCounter>) {
    counter.0 += 1;
}

/// Returns the core engine systems as a pre-built schedule.
pub fn make_core_schedule() -> ignit_ecs::Schedule {
    let mut schedule = ignit_ecs::Schedule::new();
    schedule.add_system(sys_accumulate_time);
    schedule.add_system(sys_update_frame_counter);
    schedule
}
