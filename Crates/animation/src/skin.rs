use std::hash::{Hash, Hasher};

use cgmath::{InnerSpace, Matrix4, Vector3, Vector4};

use ignit_vertex::Vertex;

/// Per-vertex joint indices and weights.
#[derive(Debug, Clone, Default)]
pub struct SkinData {
    /// Up to four joint indices per vertex.
    pub joints: Vec<[u16; 4]>,
    /// Up to four weights per vertex, matching [`Self::joints`].
    pub weights: Vec<[f32; 4]>,
}

impl SkinData {
    pub fn new(joints: Vec<[u16; 4]>, weights: Vec<[f32; 4]>) -> Self {
        Self { joints, weights }
    }

    pub fn is_empty(&self) -> bool {
        self.joints.is_empty()
    }

    pub fn len(&self) -> usize {
        self.joints.len().min(self.weights.len())
    }

    /// Applies the skinning matrices to the rest-pose vertices.
    ///
    /// Vertices with no usable weights are copied unchanged.
    pub fn skinned_vertices(&self, rest: &[Vertex], skin_matrices: &[Matrix4<f32>]) -> Vec<Vertex> {
        rest.iter()
            .enumerate()
            .map(|(index, vertex)| self.skin_vertex(index, vertex, skin_matrices))
            .collect()
    }

    fn skin_vertex(&self, index: usize, vertex: &Vertex, skin_matrices: &[Matrix4<f32>]) -> Vertex {
        let Some(joints) = self.joints.get(index) else {
            return vertex.clone();
        };
        let Some(weights) = self.weights.get(index) else {
            return vertex.clone();
        };

        let mut total = 0.0;
        for index in 0..4 {
            let joint = joints[index] as usize;
            if skin_matrices.get(joint).is_some() {
                total += weights[index];
            }
        }
        if total <= f32::EPSILON {
            return vertex.clone();
        }

        let mut columns = [Vector4::new(0.0, 0.0, 0.0, 0.0); 4];
        for index in 0..4 {
            let joint = joints[index] as usize;
            let Some(matrix) = skin_matrices.get(joint) else {
                continue;
            };
            let normalized = weights[index] / total;

            for (column, value) in columns.iter_mut().enumerate() {
                *value += matrix[column] * normalized;
            }
        }
        let matrix = Matrix4::from_cols(columns[0], columns[1], columns[2], columns[3]);

        let position = transform_point(&matrix, vertex.position);
        let normal = transform_direction(&matrix, vertex.normal);
        let tangent = transform_direction(&matrix, vertex.tangent);
        let bitangent = transform_direction(&matrix, vertex.bitangent);

        Vertex::new_with_bitangent(position, normal, tangent, bitangent, vertex.uv)
    }
}

fn transform_point(matrix: &Matrix4<f32>, value: Vector3<f32>) -> Vector3<f32> {
    let transformed = matrix * Vector4::new(value.x, value.y, value.z, 1.0);
    Vector3::new(transformed.x, transformed.y, transformed.z)
}

fn transform_direction(matrix: &Matrix4<f32>, value: Vector3<f32>) -> Vector3<f32> {
    let transformed = matrix * Vector4::new(value.x, value.y, value.z, 0.0);
    let result = Vector3::new(transformed.x, transformed.y, transformed.z);
    if result.magnitude2() > f32::EPSILON {
        result.normalize()
    } else {
        value
    }
}

impl PartialEq for SkinData {
    fn eq(&self, other: &Self) -> bool {
        self.joints == other.joints
            && self.weights.len() == other.weights.len()
            && self.weights.iter().zip(&other.weights).all(|(a, b)| {
                a.iter()
                    .zip(b.iter())
                    .all(|(a, b)| a.to_bits() == b.to_bits())
            })
    }
}

impl Eq for SkinData {}

impl Hash for SkinData {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.joints.hash(state);
        self.weights.len().hash(state);
        for weights in &self.weights {
            for weight in weights {
                weight.to_bits().hash(state);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cgmath::{SquareMatrix, Vector2};

    fn vertex() -> Vertex {
        Vertex::new(
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector2::new(0.5, 0.5),
        )
    }

    #[test]
    fn identity_matrices_keep_the_rest_vertex() {
        let skin = SkinData::new(vec![[0, 0, 0, 0]], vec![[1.0, 0.0, 0.0, 0.0]]);
        let result = skin.skinned_vertices(&[vertex()], &[Matrix4::identity()]);

        assert_eq!(result[0].position, Vector3::new(1.0, 0.0, 0.0));
    }

    #[test]
    fn translation_matrix_moves_the_vertex() {
        let skin = SkinData::new(vec![[0, 0, 0, 0]], vec![[1.0, 0.0, 0.0, 0.0]]);
        let matrix = Matrix4::from_translation(Vector3::new(2.0, 3.0, 0.0));
        let result = skin.skinned_vertices(&[vertex()], &[matrix]);

        assert_eq!(result[0].position, Vector3::new(3.0, 3.0, 0.0));
    }

    #[test]
    fn weights_are_normalized() {
        let skin = SkinData::new(vec![[0, 0, 0, 0]], vec![[2.0, 0.0, 0.0, 0.0]]);
        let matrix = Matrix4::from_translation(Vector3::new(1.0, 0.0, 0.0));
        let result = skin.skinned_vertices(&[vertex()], &[matrix]);

        assert_eq!(result[0].position, Vector3::new(2.0, 0.0, 0.0));
    }

    #[test]
    fn missing_joint_indices_fall_back_to_the_rest_vertex() {
        let skin = SkinData::new(vec![[7, 0, 0, 0]], vec![[1.0, 0.0, 0.0, 0.0]]);
        let result = skin.skinned_vertices(&[vertex()], &[Matrix4::identity()]);

        assert_eq!(result[0].position, Vector3::new(1.0, 0.0, 0.0));
    }
}
