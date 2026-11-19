use ignit_camera::CameraDescriptor;
use ignit_light::LightDescriptor;
use ignit_model::ModelDescriptor;
use std::error::Error;
use std::sync::Arc;

use ignit_animation::AnimationClip;

#[derive(Debug, Default)]
pub struct GltfImportResult {
    pub models: Vec<ModelDescriptor>,
    pub cameras: Vec<CameraDescriptor>,
    pub lights: Vec<LightDescriptor>,
    pub animations: Vec<Arc<AnimationClip>>,
    pub errors: Vec<Box<dyn Error>>,
}

impl GltfImportResult {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn extend(&mut self, other: Self) {
        self.models.extend(other.models);
        self.cameras.extend(other.cameras);
        self.lights.extend(other.lights);
        self.animations.extend(other.animations);
        self.errors.extend(other.errors);
    }
}
