struct ShadowSlot {
    light_view_proj: mat4x4<f32>,
    shadow_type: u32,
    layer_index: u32,
    cascade_split_depth: f32,
    bias: f32,
    light_index: u32,
    near_plane: f32,
}

// Keep in sync with `ignit_shadow::ShadowGpuData`.
const MAX_SHADOW_LIGHTS: u32 = 256u;

struct ShadowData {
    slots: array<ShadowSlot, 16>,
    // x = cascade count. Packed as vec4 so the following arrays are 16-byte
    // aligned in the uniform address space.
    header: vec4<u32>,
    // First slot for every light-store index, so shading does not scan the
    // 16-slot table per light. 64 vec4s hold 256 u32 entries.
    first_slot: array<vec4<u32>, 64>,
    slot_count: array<vec4<u32>, 64>,
}
