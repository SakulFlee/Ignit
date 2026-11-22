use wgpu::{
    Buffer, BufferUsages, Device, Queue,
    util::{BufferInitDescriptor, DeviceExt},
};

pub use ignit_vertex::Vertex;

mod descriptor;
pub use descriptor::*;

mod cache;
pub use cache::*;

mod bounds;
pub use bounds::*;

mod topology;
pub use topology::has_boundary_edges;

#[derive(Debug)]
pub struct Mesh {
    vertex_buffer: Buffer,
    index_buffer: Buffer,
    index_count: u32,
    bounding_sphere: Option<BoundingSphere>,
    bounding_box: Option<BoundingBox>,
    is_open: bool,
}

impl Mesh {
    pub fn from_descriptor(descriptor: &MeshDescriptor, device: &Device, _queue: &Queue) -> Self {
        Self::from_data(&descriptor.vertices, &descriptor.indices, device)
    }

    pub fn from_data(vertices: &[Vertex], indices: &[u32], device: &Device) -> Self {
        let is_open = has_boundary_edges(vertices, indices);

        let vertex_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Mesh Vertex Buffer"),
            contents: &vertices
                .iter()
                .flat_map(|x| x.to_bytes())
                .collect::<Vec<u8>>(),
            usage: BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Mesh Index Buffer"),
            contents: &indices
                .iter()
                .flat_map(|x| x.to_le_bytes())
                .collect::<Vec<u8>>(),
            usage: BufferUsages::INDEX,
        });

        let positions: Vec<_> = vertices.iter().map(|v| v.position).collect();
        let bounding_sphere = BoundingSphere::compute_from_positions(&positions);
        let bounding_box = BoundingBox::compute_from_positions(&positions);

        Self {
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
            bounding_sphere,
            bounding_box,
            is_open,
        }
    }

    /// Whether the mesh has a boundary edge, meaning it is not a closed shell.
    ///
    /// Open surfaces need two-sided rasterization in the shadow pass so they
    /// still block light from whichever side the light reaches them.
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    pub fn bounding_sphere(&self) -> Option<&BoundingSphere> {
        self.bounding_sphere.as_ref()
    }

    pub fn bounding_box(&self) -> Option<&BoundingBox> {
        self.bounding_box.as_ref()
    }

    pub fn vertex_buffer(&self) -> &Buffer {
        &self.vertex_buffer
    }

    pub fn index_buffer(&self) -> &Buffer {
        &self.index_buffer
    }

    pub fn index_count(&self) -> u32 {
        self.index_count
    }
}
