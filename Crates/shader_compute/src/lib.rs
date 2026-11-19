use std::sync::atomic::{AtomicU64, Ordering};

pub use ignit_shader_core::{ShaderDescriptor, ShaderError};

mod binding;
pub use binding::*;

mod descriptor;
pub use descriptor::*;

mod error;
pub use error::*;

mod pipeline_cache;
pub use pipeline_cache::*;

mod program;
pub use program::*;

mod shader;
pub use shader::*;

static COMPUTE_PIPELINES_CREATED: AtomicU64 = AtomicU64::new(0);

/// Returns the cumulative number of compute pipelines created.
pub fn compute_pipelines_created() -> u64 {
    COMPUTE_PIPELINES_CREATED.load(Ordering::Relaxed)
}
