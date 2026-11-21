// Computes skinned vertices from the rest pose, joints, weights, and skin
// matrices. The vertex stream is bound as raw floats with stride 14 to match
// `ignit_vertex::Vertex`.

const STRIDE: u32 = 14u;

@group(0) @binding(0) var<storage, read> rest: array<f32>;
@group(0) @binding(1) var<storage, read> joints: array<vec4<u32>>;
@group(0) @binding(2) var<storage, read> weights: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> matrices: array<mat4x4<f32>>;
@group(0) @binding(4) var<storage, read_write> output: array<f32>;

fn transform_direction(matrix: mat4x4<f32>, value: vec3<f32>) -> vec3<f32> {
    let transformed = (matrix * vec4<f32>(value, 0.0)).xyz;
    if (dot(transformed, transformed) > 1e-12) {
        return normalize(transformed);
    }
    return value;
}

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {
    let vertex_count = arrayLength(&joints);
    if (id.x >= vertex_count) {
        return;
    }

    let base = id.x * STRIDE;
    let position = vec3<f32>(rest[base], rest[base + 1u], rest[base + 2u]);
    let normal = vec3<f32>(rest[base + 3u], rest[base + 4u], rest[base + 5u]);
    let tangent = vec3<f32>(rest[base + 6u], rest[base + 7u], rest[base + 8u]);
    let bitangent = vec3<f32>(rest[base + 9u], rest[base + 10u], rest[base + 11u]);
    let uv = vec2<f32>(rest[base + 12u], rest[base + 13u]);

    let vertex_joints = joints[id.x];
    let vertex_weights = weights[id.x];
    let matrix_count = arrayLength(&matrices);

    var total = 0.0;
    for (var influence = 0u; influence < 4u; influence = influence + 1u) {
        if (vertex_joints[influence] < matrix_count) {
            total = total + vertex_weights[influence];
        }
    }

    var skinned_position = position;
    var skinned_normal = normal;
    var skinned_tangent = tangent;
    var skinned_bitangent = bitangent;

    if (total > 0.0) {
        var matrix = mat4x4<f32>(
            vec4<f32>(0.0, 0.0, 0.0, 0.0),
            vec4<f32>(0.0, 0.0, 0.0, 0.0),
            vec4<f32>(0.0, 0.0, 0.0, 0.0),
            vec4<f32>(0.0, 0.0, 0.0, 0.0),
        );
        for (var influence = 0u; influence < 4u; influence = influence + 1u) {
            let joint = vertex_joints[influence];
            if (joint < matrix_count) {
                matrix = matrix + matrices[joint] * (vertex_weights[influence] / total);
            }
        }

        skinned_position = (matrix * vec4<f32>(position, 1.0)).xyz;
        skinned_normal = transform_direction(matrix, normal);
        skinned_tangent = transform_direction(matrix, tangent);
        skinned_bitangent = transform_direction(matrix, bitangent);
    }

    output[base] = skinned_position.x;
    output[base + 1u] = skinned_position.y;
    output[base + 2u] = skinned_position.z;
    output[base + 3u] = skinned_normal.x;
    output[base + 4u] = skinned_normal.y;
    output[base + 5u] = skinned_normal.z;
    output[base + 6u] = skinned_tangent.x;
    output[base + 7u] = skinned_tangent.y;
    output[base + 8u] = skinned_tangent.z;
    output[base + 9u] = skinned_bitangent.x;
    output[base + 10u] = skinned_bitangent.y;
    output[base + 11u] = skinned_bitangent.z;
    output[base + 12u] = uv.x;
    output[base + 13u] = uv.y;
}
