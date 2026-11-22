//! Mesh topology helpers used to decide whether a surface is a closed shell.

use std::collections::HashMap;

use cgmath::Vector3;
use ignit_vertex::Vertex;

/// Returns `true` when a triangle edge is used by only one triangle.
///
/// Open surfaces are rendered two-sided in the shadow pass: a single-sided
/// plane or disk still has to block light from either side, and one-sided
/// culling would silently remove it from the shadow map whenever the light is
/// behind its front face.
///
/// Vertices are welded by position first, so UV seams and duplicated ring
/// vertices do not count as boundaries.
pub fn has_boundary_edges(vertices: &[Vertex], indices: &[u32]) -> bool {
    if indices.len() < 3 {
        return true;
    }

    let mut welded = HashMap::with_capacity(vertices.len());
    let mut remap = Vec::with_capacity(vertices.len());
    for vertex in vertices {
        let next = welded.len() as u32;
        let key = quantized_position(vertex.position);
        remap.push(*welded.entry(key).or_insert(next));
    }

    let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
    for triangle in indices.as_chunks::<3>().0 {
        let a = remap[triangle[0] as usize];
        let b = remap[triangle[1] as usize];
        let c = remap[triangle[2] as usize];
        if a == b || b == c || a == c {
            continue;
        }

        for (from, to) in [(a, b), (b, c), (c, a)] {
            let edge = if from < to { (from, to) } else { (to, from) };
            *edges.entry(edge).or_insert(0) += 1;
        }
    }

    edges.values().any(|&uses| uses == 1)
}

/// Welds positions with 0.01 mm resolution in engine units. Generated seams
/// such as `sin(TAU)` versus `sin(0)` land on the same key.
fn quantized_position(position: Vector3<f32>) -> [i64; 3] {
    const SCALE: f32 = 100_000.0;
    [
        (position.x * SCALE).round() as i64,
        (position.y * SCALE).round() as i64,
        (position.z * SCALE).round() as i64,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use cgmath::Vector2;

    fn vertices(positions: &[[f32; 3]]) -> Vec<Vertex> {
        positions
            .iter()
            .map(|position| {
                Vertex::new(
                    Vector3::new(position[0], position[1], position[2]),
                    Vector3::unit_y(),
                    Vector3::unit_x(),
                    Vector2::new(0.0, 0.0),
                )
            })
            .collect()
    }

    fn tetrahedron() -> (Vec<Vertex>, Vec<u32>) {
        let positions = vertices(&[
            [1.0, 1.0, 1.0],
            [1.0, -1.0, -1.0],
            [-1.0, 1.0, -1.0],
            [-1.0, -1.0, 1.0],
        ]);
        let indices = vec![0, 1, 2, 0, 3, 1, 0, 2, 3, 1, 3, 2];
        (positions, indices)
    }

    #[test]
    fn single_triangle_is_open() {
        let vertices = vertices(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]);
        assert!(has_boundary_edges(&vertices, &[0, 1, 2]));
    }

    #[test]
    fn quad_is_open() {
        let vertices = vertices(&[
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ]);
        assert!(has_boundary_edges(&vertices, &[0, 1, 2, 0, 2, 3]));
    }

    #[test]
    fn tetrahedron_is_closed() {
        let (vertices, indices) = tetrahedron();
        assert!(!has_boundary_edges(&vertices, &indices));
    }

    #[test]
    fn duplicated_position_still_counts_as_shared() {
        let (mut vertices, mut indices) = tetrahedron();
        // Split one corner into a duplicate vertex at the same position.
        vertices.push(vertices[0].clone());
        indices[3] = 4;
        assert!(!has_boundary_edges(&vertices, &indices));
    }

    #[test]
    fn empty_mesh_is_open() {
        assert!(has_boundary_edges(&[], &[]));
    }
}
