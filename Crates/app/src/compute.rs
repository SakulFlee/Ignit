//! Per-frame compute workloads.
//!
//! Modules register [`ComputeWorkload`]s in the [`ComputeWorkloads`] ECS
//! resource. The runtime records them before the scene render so compute
//! output is visible to the shadow and model passes.

use std::sync::{Arc, Mutex};

use wgpu::{CommandEncoder, Device, Queue};

/// Recording context handed to a [`ComputeWorkload`].
pub struct ComputeWorkloadContext<'a> {
    pub encoder: &'a mut CommandEncoder,
    pub device: &'a Device,
    pub queue: &'a Queue,
    /// Read-only ECS access for resources such as animation state.
    pub ecs: &'a ignit_ecs::World,
}

/// A compute dispatch registered with the runtime.
///
/// Workloads are recorded before the scene graph runs, ahead of the shadow
/// pass. The renderer's world bind group does not exist yet at that point, so
/// workloads that need camera or light data must provide their own bindings.
///
/// Workloads must not mutate the ECS structurally; use internal
/// `Mutex`/atomics for per-frame state.
pub trait ComputeWorkload: Send + Sync {
    /// Human-readable name used for diagnostics.
    fn name(&self) -> &str {
        "compute"
    }

    /// Records the workload's commands.
    fn record(&self, ctx: &mut ComputeWorkloadContext<'_>);
}

/// ECS resource containing the registered compute workloads.
#[derive(Default)]
pub struct ComputeWorkloads(Mutex<Vec<Arc<dyn ComputeWorkload>>>);

impl ComputeWorkloads {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a workload.
    pub fn add(&self, workload: Arc<dyn ComputeWorkload>) {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(workload);
    }

    /// Clones the registered workload list.
    ///
    /// The runtime uses a snapshot so workloads are not recorded while the
    /// lock is held.
    pub fn snapshot(&self) -> Vec<Arc<dyn ComputeWorkload>> {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub fn len(&self) -> usize {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingWorkload(AtomicUsize);

    impl ComputeWorkload for CountingWorkload {
        fn name(&self) -> &str {
            "counting"
        }

        fn record(&self, _ctx: &mut ComputeWorkloadContext<'_>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn snapshots_registered_workloads() {
        let workloads = ComputeWorkloads::new();
        let counter = Arc::new(CountingWorkload(AtomicUsize::new(0)));

        workloads.add(counter.clone());

        let snapshot = workloads.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].name(), "counting");
        assert_eq!(counter.0.load(Ordering::Relaxed), 0);
    }
}
