//! Dedicated pre-game UI phase.
//!
//! UI systems need `&mut World` (reconcile, layout, hit-testing, and draw-list
//! construction all write components), which the normal game `Schedule` does
//! not expose. The runtime runs this phase after the core schedule and before
//! game systems, so games see the same-frame input capture and UI events.

use ignit_ecs::World;

pub trait UiSystem: Send {
    fn name(&self) -> &str;
    fn run(&mut self, world: &mut World);
}

impl UiSystem for Box<dyn UiSystem> {
    fn name(&self) -> &str {
        (**self).name()
    }

    fn run(&mut self, world: &mut World) {
        (**self).run(world);
    }
}

#[derive(Default)]
pub struct UiSchedule {
    systems: Vec<Box<dyn UiSystem>>,
}

impl UiSchedule {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_system(&mut self, system: Box<dyn UiSystem>) {
        self.systems.push(system);
    }

    pub fn system_count(&self) -> usize {
        self.systems.len()
    }

    pub fn run(&mut self, world: &mut World) {
        for system in &mut self.systems {
            system.run(world);
        }
    }
}

/// Convenience adapter for a closure UI phase.
pub struct FunctionUiSystem {
    name: &'static str,
    run: Box<dyn FnMut(&mut World) + Send>,
}

impl FunctionUiSystem {
    pub fn new(name: &'static str, run: impl FnMut(&mut World) + Send + 'static) -> Self {
        Self {
            name,
            run: Box::new(run),
        }
    }
}

impl UiSystem for FunctionUiSystem {
    fn name(&self) -> &str {
        self.name
    }

    fn run(&mut self, world: &mut World) {
        (self.run)(world);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn ui_schedule_preserves_registration_order() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let first = Arc::clone(&order);
        let second = Arc::clone(&order);
        let mut schedule = UiSchedule::new();
        schedule.add_system(Box::new(FunctionUiSystem::new("first", move |_| {
            first.lock().unwrap().push(1);
        })));
        schedule.add_system(Box::new(FunctionUiSystem::new("second", move |_| {
            second.lock().unwrap().push(2);
        })));
        let mut world = World::new();
        schedule.run(&mut world);
        assert_eq!(*order.lock().unwrap(), vec![1, 2]);
        assert_eq!(schedule.system_count(), 2);
    }
}
