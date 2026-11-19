//! Compute-pipeline cache.
//!
//! Programs are tracked weakly so pipelines for dropped programs are pruned
//! instead of leaking for the lifetime of the device.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};

use wgpu::{ComputePipeline, Device};

use crate::ComputeShaderProgram;

#[derive(Debug, Default)]
pub struct ComputePipelineCache {
    programs: HashMap<u64, Weak<ComputeShaderProgram>>,
    pipelines: HashMap<u64, Arc<ComputePipeline>>,
}

impl ComputePipelineCache {
    /// Returns the pipeline for a program, creating it on first use.
    pub fn pipeline(
        &mut self,
        program: &Arc<ComputeShaderProgram>,
        device: &Device,
    ) -> Arc<ComputePipeline> {
        self.programs.insert(program.id(), Arc::downgrade(program));
        self.prune();

        self.pipelines
            .entry(program.id())
            .or_insert_with(|| {
                crate::COMPUTE_PIPELINES_CREATED.fetch_add(1, Ordering::Relaxed);
                Arc::new(program.create_pipeline(device))
            })
            .clone()
    }

    /// Drops programs that no shader references and their pipelines.
    pub fn prune(&mut self) {
        self.programs
            .retain(|_, program| program.strong_count() > 0);

        let alive: HashSet<u64> = self.programs.keys().copied().collect();
        self.pipelines.retain(|program, _| alive.contains(program));
    }

    pub fn program_count(&self) -> usize {
        self.programs.len()
    }

    pub fn pipeline_count(&self) -> usize {
        self.pipelines.len()
    }
}
