use std::{
    error::Error,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicU64, Ordering},
    },
};

use cgmath::{EuclideanSpace, Matrix4, Point3, Vector3, Vector4};
use hashbrown::HashMap;
use wgpu::{Buffer, BufferDescriptor, BufferUsages, Device, Queue, TextureFormat};

use ignit_core::cache::{Cache, CacheEntry};
use ignit_instance::Instance;
use ignit_material_shader::{MaterialShader, MaterialShaderDescriptor, PipelineCache};
use ignit_math::Transform;
pub use ignit_mesh::{Mesh, MeshDescriptor};
use ignit_vertex::Vertex;
use ulid::Ulid;

mod descriptor;
pub use descriptor::*;

static MATERIAL_CACHE_HITS: AtomicU64 = AtomicU64::new(0);
static MATERIAL_CACHE_MISSES: AtomicU64 = AtomicU64::new(0);

/// Returns cumulative material cache hits and misses.
pub fn material_cache_stats() -> (u64, u64) {
    (
        MATERIAL_CACHE_HITS.load(Ordering::Relaxed),
        MATERIAL_CACHE_MISSES.load(Ordering::Relaxed),
    )
}

#[derive(Debug, Clone)]
pub struct Model {
    mesh: Arc<Mesh>,
    materials: Vec<Arc<MaterialShader>>,
    /// Source descriptors kept for cheap identity checks. A transform-only
    /// update can compare these `Arc`s without re-hashing the whole mesh.
    source_mesh: Arc<MeshDescriptor>,
    source_materials: Vec<Arc<MaterialShaderDescriptor>>,
    /// Skin binding used by the CPU skinning path.
    source_skin: Option<Arc<ignit_animation::SkinData>>,
    /// Optional GPU buffer receiving CPU- or compute-skinned vertices.
    skinned_vertex_buffer: Option<Buffer>,
    skinned_vertex_capacity: u32,
    /// Rest-pose vertex bytes for compute skinning.
    skin_rest_vertex_buffer: Option<Buffer>,
    /// Per-vertex joint indices as `u32x4`, for compute skinning.
    skin_joint_buffer: Option<Buffer>,
    /// Per-vertex weights as `f32x4`, for compute skinning.
    skin_weight_buffer: Option<Buffer>,
    /// Skin matrices consumed by compute skinning.
    skin_matrix_buffer: Option<Buffer>,
    skin_matrix_capacity: u32,
    instance_count: u32,
    instance_capacity: u32,
    instance_buffer: Buffer,
    /// CPU copy of the instance buffer, used to compact visible instances
    /// without re-encoding every transform each frame.
    instance_data: Vec<u8>,
    /// World-space bounding sphere per instance: `[center.x, center.y, center.z, radius]`.
    instance_bounds: Vec<[f32; 4]>,
    /// Hash of the mesh and materials. Instance transforms are excluded so an
    /// animation update can reuse the existing GPU buffer.
    source_hash: Ulid,
}

impl Model {
    #[allow(clippy::too_many_arguments)]
    pub fn from_descriptor<'cache>(
        descriptor: &ModelDescriptor,
        color_format: &TextureFormat,
        device: &Device,
        queue: &Queue,
        mesh_cache: &'cache RwLock<Cache<Arc<MeshDescriptor>, Mesh>>,
        material_cache: &'cache RwLock<Cache<Arc<MaterialShaderDescriptor>, MaterialShader>>,
        pipeline_cache: &Mutex<PipelineCache>,
        gbuffer_color_formats: Option<&[TextureFormat]>,
    ) -> Result<Self, Box<dyn Error + 'cache>> {
        // --- Mesh ---
        let mesh = match mesh_cache.write() {
            Ok(mut lock) => lock
                .entry(descriptor.mesh.clone())
                .or_insert(CacheEntry::new(Mesh::from_descriptor(
                    &descriptor.mesh,
                    device,
                    queue,
                )))
                .clone_inner(),
            Err(e) => return Err(Box::new(e)),
        };

        // --- Material ---
        let mut materials = Vec::new();
        for material_descriptor in &descriptor.materials {
            let material = match material_cache.write() {
                Ok(mut lock) => {
                    if let Some(entry) = lock.get(material_descriptor) {
                        MATERIAL_CACHE_HITS.fetch_add(1, Ordering::Relaxed);
                        entry.clone_inner()
                    } else {
                        MATERIAL_CACHE_MISSES.fetch_add(1, Ordering::Relaxed);

                        let mut pipeline_cache = pipeline_cache.lock().unwrap();
                        let material = MaterialShader::from_descriptor_with_cache(
                            material_descriptor,
                            Some(*color_format),
                            device,
                            queue,
                            &mut pipeline_cache,
                            gbuffer_color_formats,
                        )?;
                        let entry = CacheEntry::new(material);
                        let material = entry.clone_inner();
                        lock.insert(material_descriptor.clone(), entry);
                        material
                    }
                }
                Err(e) => return Err(Box::new(e)),
            };
            materials.push(material);
        }

        let instance_capacity = (descriptor.transforms.len() as u32).max(1);
        let instance_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("Model Instance Buffer"),
            size: instance_capacity as u64 * 64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let skinned_vertex_capacity = descriptor.mesh.vertices.len().max(1) as u32;
        let skinned_vertex_buffer = descriptor.skin.as_ref().map(|_| {
            device.create_buffer(&BufferDescriptor {
                label: Some("Model Skinned Vertex Buffer"),
                size: u64::from(skinned_vertex_capacity) * Vertex::BYTE_SIZE as u64,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST | BufferUsages::STORAGE,
                mapped_at_creation: false,
            })
        });
        if let Some(buffer) = &skinned_vertex_buffer {
            queue.write_buffer(buffer, 0, &vertices_to_bytes(&descriptor.mesh.vertices));
        }

        let (skin_rest_vertex_buffer, skin_joint_buffer, skin_weight_buffer) =
            match descriptor.skin.as_ref() {
                Some(skin) => {
                    let rest = vertices_to_bytes(&descriptor.mesh.vertices);
                    let rest_buffer = device.create_buffer(&BufferDescriptor {
                        label: Some("Model Skin Rest Vertex Buffer"),
                        size: rest.len().max(4) as u64,
                        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    queue.write_buffer(&rest_buffer, 0, &rest);

                    let joints = skin_joints_to_bytes(skin);
                    let joint_buffer = device.create_buffer(&BufferDescriptor {
                        label: Some("Model Skin Joint Buffer"),
                        size: joints.len().max(4) as u64,
                        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    queue.write_buffer(&joint_buffer, 0, &joints);

                    let weights = skin_weights_to_bytes(skin);
                    let weight_buffer = device.create_buffer(&BufferDescriptor {
                        label: Some("Model Skin Weight Buffer"),
                        size: weights.len().max(4) as u64,
                        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    queue.write_buffer(&weight_buffer, 0, &weights);

                    (Some(rest_buffer), Some(joint_buffer), Some(weight_buffer))
                }
                None => (None, None, None),
            };

        let skin_matrix_capacity = descriptor
            .skeleton
            .as_ref()
            .map(|skeleton| skeleton.joints.len().max(1) as u32)
            .unwrap_or(1);
        let skin_matrix_buffer = descriptor.skin.as_ref().map(|_| {
            device.create_buffer(&BufferDescriptor {
                label: Some("Model Skin Matrix Buffer"),
                size: u64::from(skin_matrix_capacity) * 64,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });

        let mut model = Self {
            mesh,
            materials,
            source_mesh: descriptor.mesh.clone(),
            source_materials: descriptor.materials.clone(),
            source_skin: descriptor.skin.clone(),
            skinned_vertex_buffer,
            skinned_vertex_capacity,
            skin_rest_vertex_buffer,
            skin_joint_buffer,
            skin_weight_buffer,
            skin_matrix_buffer,
            skin_matrix_capacity,
            instance_count: 0,
            instance_capacity,
            instance_buffer,
            instance_data: Vec::with_capacity(instance_capacity as usize * 64),
            instance_bounds: Vec::with_capacity(instance_capacity as usize),
            source_hash: descriptor.instance_hash(),
        };
        model.update_instances(&descriptor.transforms, device, queue);
        Ok(model)
    }

    /// Update the instance buffer in place.
    ///
    /// The buffer is only reallocated when the new instance count exceeds the
    /// existing capacity. This is the path animation will use every frame.
    pub fn update_instances(
        &mut self,
        transforms: &HashMap<Ulid, Transform>,
        device: &Device,
        queue: &Queue,
    ) {
        let instance_count = transforms.len() as u32;
        if instance_count > self.instance_capacity {
            let new_capacity = instance_count.next_power_of_two().max(1);
            self.instance_buffer = device.create_buffer(&BufferDescriptor {
                label: Some("Model Instance Buffer"),
                size: new_capacity as u64 * 64,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.instance_capacity = new_capacity;
            self.instance_data.reserve(new_capacity as usize * 64);
            self.instance_bounds.reserve(new_capacity as usize);
        }

        self.instance_data.clear();
        self.instance_bounds.clear();
        let bounding_sphere = self.mesh.bounding_sphere();

        for transform in transforms.values() {
            let instance = Instance::from(transform);
            self.instance_data
                .extend(instance.to_buffer_data_flattened());

            if let Some(sphere) = &bounding_sphere {
                let matrix = transform.to_matrix();
                let center =
                    matrix * Vector4::new(sphere.center.x, sphere.center.y, sphere.center.z, 1.0);
                let max_scale = transform
                    .scale
                    .x
                    .max(transform.scale.y)
                    .max(transform.scale.z);
                self.instance_bounds.push([
                    center.x,
                    center.y,
                    center.z,
                    sphere.radius * max_scale,
                ]);
            } else {
                self.instance_bounds.push([0.0, 0.0, 0.0, 0.0]);
            }
        }

        if instance_count > 0 {
            queue.write_buffer(&self.instance_buffer, 0, &self.instance_data);
        }
        self.instance_count = instance_count;
    }

    pub fn mesh(&self) -> &Mesh {
        &self.mesh
    }

    /// Rest-pose mesh descriptor, including the CPU vertex data.
    pub fn source_mesh(&self) -> &Arc<MeshDescriptor> {
        &self.source_mesh
    }

    /// Skin binding for CPU skinning, if this model is skinned.
    pub fn skin(&self) -> Option<&Arc<ignit_animation::SkinData>> {
        self.source_skin.as_ref()
    }

    /// Vertex buffer used by the render and shadow passes.
    ///
    /// Skinned models use the buffer written by the animation system;
    /// everything else uses the static mesh buffer.
    pub fn vertex_buffer(&self) -> &Buffer {
        self.skinned_vertex_buffer
            .as_ref()
            .unwrap_or_else(|| self.mesh.vertex_buffer())
    }

    /// Uploads CPU-skinned vertices, reallocating the buffer when needed.
    pub fn update_skinned_vertices(&mut self, vertices: &[Vertex], device: &Device, queue: &Queue) {
        if self.source_skin.is_none() {
            return;
        }

        let required = vertices.len().max(1) as u32;
        if self.skinned_vertex_buffer.is_none() || required > self.skinned_vertex_capacity {
            let capacity = required.next_power_of_two().max(1);
            self.skinned_vertex_buffer = Some(device.create_buffer(&BufferDescriptor {
                label: Some("Model Skinned Vertex Buffer"),
                size: u64::from(capacity) * Vertex::BYTE_SIZE as u64,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.skinned_vertex_capacity = capacity;
        }

        if let Some(buffer) = &self.skinned_vertex_buffer {
            queue.write_buffer(buffer, 0, &vertices_to_bytes(vertices));
        }
    }

    /// Rest-pose vertex storage buffer used by compute skinning.
    pub fn skin_rest_buffer(&self) -> Option<&Buffer> {
        self.skin_rest_vertex_buffer.as_ref()
    }

    /// Per-vertex joint indices used by compute skinning.
    pub fn skin_joint_buffer(&self) -> Option<&Buffer> {
        self.skin_joint_buffer.as_ref()
    }

    /// Per-vertex weights used by compute skinning.
    pub fn skin_weight_buffer(&self) -> Option<&Buffer> {
        self.skin_weight_buffer.as_ref()
    }

    /// Skin-matrix storage buffer used by compute skinning.
    pub fn skin_matrix_buffer(&self) -> Option<&Buffer> {
        self.skin_matrix_buffer.as_ref()
    }

    /// Output buffer written by compute skinning.
    pub fn compute_skinned_vertex_buffer(&self) -> Option<&Buffer> {
        self.skinned_vertex_buffer.as_ref()
    }

    /// Number of vertices in the skinned output.
    pub fn skinned_vertex_count(&self) -> u32 {
        self.source_mesh.vertices.len() as u32
    }

    /// Uploads the skin matrices consumed by compute skinning.
    pub fn update_skin_matrices(
        &mut self,
        matrices: &[Matrix4<f32>],
        device: &Device,
        queue: &Queue,
    ) {
        if self.source_skin.is_none() {
            return;
        }

        let required = matrices.len().max(1) as u32;
        if self.skin_matrix_buffer.is_none() || required > self.skin_matrix_capacity {
            let capacity = required.next_power_of_two().max(1);
            self.skin_matrix_buffer = Some(device.create_buffer(&BufferDescriptor {
                label: Some("Model Skin Matrix Buffer"),
                size: u64::from(capacity) * 64,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.skin_matrix_capacity = capacity;
        }

        if let Some(buffer) = &self.skin_matrix_buffer {
            queue.write_buffer(buffer, 0, &matrices_to_bytes(matrices));
        }
    }

    pub fn materials(&self) -> &Vec<Arc<MaterialShader>> {
        &self.materials
    }

    /// Whether any material on this model casts shadows.
    pub fn casts_shadows(&self) -> bool {
        self.materials
            .iter()
            .any(|material| material.cast_shadows())
    }

    pub fn instance_count(&self) -> u32 {
        self.instance_count
    }

    pub fn instance_buffer(&self) -> &Buffer {
        &self.instance_buffer
    }

    /// CPU copy of the instance buffer, matching [`Self::instance_buffer`].
    pub fn instance_data(&self) -> &[u8] {
        &self.instance_data
    }

    /// World-space bounding sphere per instance: `[center.x, center.y, center.z, radius]`.
    pub fn instance_bounds(&self) -> &[[f32; 4]] {
        &self.instance_bounds
    }

    /// Average world-space center of all instances.
    pub fn instance_center(&self) -> Option<Point3<f32>> {
        if self.instance_bounds.is_empty() {
            return None;
        }

        let mut center = Vector3::new(0.0, 0.0, 0.0);
        for bound in &self.instance_bounds {
            center += Vector3::new(bound[0], bound[1], bound[2]);
        }
        Some(Point3::from_vec(center / self.instance_bounds.len() as f32))
    }

    /// Hash of the mesh and materials, used to decide whether a model can
    /// update its instances in place.
    pub fn source_hash(&self) -> Ulid {
        self.source_hash
    }

    /// Whether `descriptor` still refers to the same mesh and material
    /// descriptors used to build this model.
    ///
    /// This is a pointer-identity check, not a content hash. Descriptors are
    /// immutable values held in shared `Arc`s, so a changed source necessarily
    /// produces a different `Arc`.
    pub fn source_matches(&self, descriptor: &ModelDescriptor) -> bool {
        Arc::ptr_eq(&self.source_mesh, &descriptor.mesh)
            && option_arc_ptr_eq(&self.source_skin, &descriptor.skin)
            && self.source_materials.len() == descriptor.materials.len()
            && self
                .source_materials
                .iter()
                .zip(&descriptor.materials)
                .all(|(a, b)| Arc::ptr_eq(a, b))
    }
}

fn vertices_to_bytes(vertices: &[Vertex]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vertices.len() * Vertex::BYTE_SIZE);
    for vertex in vertices {
        bytes.extend(vertex.to_bytes());
    }
    bytes
}

fn skin_joints_to_bytes(skin: &ignit_animation::SkinData) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(skin.len() * 16);
    for (joints, _) in skin.joints.iter().zip(skin.weights.iter()) {
        for joint in joints {
            bytes.extend_from_slice(&u32::from(*joint).to_le_bytes());
        }
    }
    bytes
}

fn skin_weights_to_bytes(skin: &ignit_animation::SkinData) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(skin.len() * 16);
    for (_, weights) in skin.joints.iter().zip(skin.weights.iter()) {
        for weight in weights {
            bytes.extend_from_slice(&weight.to_le_bytes());
        }
    }
    bytes
}

fn matrices_to_bytes(matrices: &[Matrix4<f32>]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(matrices.len() * 64);
    for matrix in matrices {
        for column in 0..4 {
            for row in 0..4 {
                bytes.extend_from_slice(&matrix[column][row].to_le_bytes());
            }
        }
    }
    bytes
}

fn option_arc_ptr_eq<T>(left: &Option<Arc<T>>, right: &Option<Arc<T>>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => Arc::ptr_eq(left, right),
        (None, None) => true,
        (Some(_), None) | (None, Some(_)) => false,
    }
}
