use std::num::NonZeroU64;

use wgpu::{BindGroupEntry, BindingResource, Buffer, BufferBinding, Sampler, TextureView};

/// A GPU resource bound to a compute shader.
#[derive(Debug, Clone)]
pub enum ComputeBindingResource {
    /// The whole buffer.
    Buffer(Buffer),
    /// A sub-range of a buffer.
    BufferRange {
        buffer: Buffer,
        offset: u64,
        size: u64,
    },
    TextureView(TextureView),
    Sampler(Sampler),
}

impl ComputeBindingResource {
    fn as_binding_resource(&self) -> BindingResource<'_> {
        match self {
            Self::Buffer(buffer) => BindingResource::Buffer(buffer.as_entire_buffer_binding()),
            Self::BufferRange {
                buffer,
                offset,
                size,
            } => BindingResource::Buffer(BufferBinding {
                buffer,
                offset: *offset,
                size: NonZeroU64::new(*size),
            }),
            Self::TextureView(view) => BindingResource::TextureView(view),
            Self::Sampler(sampler) => BindingResource::Sampler(sampler),
        }
    }
}

/// Resources for a compute shader's own bind group.
#[derive(Debug, Clone, Default)]
pub struct ComputeBindings {
    entries: Vec<(u32, ComputeBindingResource)>,
}

impl ComputeBindings {
    pub fn new() -> Self {
        Self::default()
    }

    /// Binds an entire buffer.
    pub fn buffer(mut self, binding: u32, buffer: &Buffer) -> Self {
        self.entries
            .push((binding, ComputeBindingResource::Buffer(buffer.clone())));
        self
    }

    /// Binds a sub-range of a buffer.
    pub fn buffer_range(mut self, binding: u32, buffer: &Buffer, offset: u64, size: u64) -> Self {
        assert!(size > 0, "buffer range size must be non-zero");
        self.entries.push((
            binding,
            ComputeBindingResource::BufferRange {
                buffer: buffer.clone(),
                offset,
                size,
            },
        ));
        self
    }

    /// Binds a texture view.
    pub fn texture_view(mut self, binding: u32, view: &TextureView) -> Self {
        self.entries
            .push((binding, ComputeBindingResource::TextureView(view.clone())));
        self
    }

    /// Binds a sampler.
    pub fn sampler(mut self, binding: u32, sampler: &Sampler) -> Self {
        self.entries
            .push((binding, ComputeBindingResource::Sampler(sampler.clone())));
        self
    }

    /// Whether a resource exists for the given binding.
    pub fn has_binding(&self, binding: u32) -> bool {
        self.entries
            .iter()
            .any(|(entry_binding, _)| *entry_binding == binding)
    }

    /// Returns the buffer bound at `binding`, including buffer ranges.
    pub fn buffer_at(&self, binding: u32) -> Option<&Buffer> {
        self.entries.iter().find_map(|(entry_binding, resource)| {
            if *entry_binding != binding {
                return None;
            }

            match resource {
                ComputeBindingResource::Buffer(buffer)
                | ComputeBindingResource::BufferRange { buffer, .. } => Some(buffer),
                ComputeBindingResource::TextureView(_) | ComputeBindingResource::Sampler(_) => None,
            }
        })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn bind_group_entries(&self) -> Vec<BindGroupEntry<'_>> {
        let mut entries = self.entries.iter().collect::<Vec<_>>();
        entries.sort_by_key(|(binding, _)| *binding);

        entries
            .into_iter()
            .map(|(binding, resource)| BindGroupEntry {
                binding: *binding,
                resource: resource.as_binding_resource(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_lookup_includes_ranges() {
        let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        let bindings = ComputeBindings::new().buffer_range(2, &buffer, 16, 32);

        assert!(bindings.has_binding(2));
        assert!(bindings.buffer_at(2).is_some());
        assert!(!bindings.has_binding(1));
    }
}
