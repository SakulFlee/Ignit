use std::collections::HashMap;

use cgmath::{InnerSpace, Matrix4, SquareMatrix};

use ignit_math::Transform;

use crate::{AnimationClip, AnimationPath, AnimationValue};

/// Rest-pose data for one joint.
#[derive(Debug, Clone)]
pub struct JointDescriptor {
    /// glTF node index this joint belongs to.
    pub node_index: usize,
    pub name: String,
    /// Parent joint index, not node index.
    pub parent: Option<usize>,
    pub rest: Transform,
}

/// Immutable skeleton data produced by the importer.
#[derive(Debug, Clone)]
pub struct SkeletonDescriptor {
    pub joints: Vec<JointDescriptor>,
    pub inverse_bind_matrices: Vec<Matrix4<f32>>,
}

/// Mutable joint state for one skeleton instance.
#[derive(Debug, Clone)]
pub struct JointState {
    pub node_index: usize,
    pub name: String,
    pub parent: Option<usize>,
    pub rest: Transform,
    pub local: Transform,
    pub world: Matrix4<f32>,
}

/// Runtime skeleton with current joint transforms and skinning matrices.
#[derive(Debug, Clone)]
pub struct Skeleton {
    joints: Vec<JointState>,
    inverse_bind_matrices: Vec<Matrix4<f32>>,
    node_to_joint: HashMap<usize, usize>,
    skin_matrices: Vec<Matrix4<f32>>,
}

impl Skeleton {
    pub fn from_descriptor(descriptor: &SkeletonDescriptor) -> Self {
        let mut node_to_joint = HashMap::with_capacity(descriptor.joints.len());
        let joints = descriptor
            .joints
            .iter()
            .enumerate()
            .map(|(index, joint)| {
                node_to_joint.insert(joint.node_index, index);
                JointState {
                    node_index: joint.node_index,
                    name: joint.name.clone(),
                    parent: joint.parent,
                    rest: joint.rest,
                    local: joint.rest,
                    world: Matrix4::identity(),
                }
            })
            .collect::<Vec<_>>();

        let count = joints.len();
        let mut skeleton = Self {
            joints,
            inverse_bind_matrices: descriptor.inverse_bind_matrices.clone(),
            node_to_joint,
            skin_matrices: vec![Matrix4::identity(); count],
        };
        skeleton.update_world(Matrix4::identity());
        skeleton
    }

    pub fn joints(&self) -> &[JointState] {
        &self.joints
    }

    pub fn skin_matrices(&self) -> &[Matrix4<f32>] {
        &self.skin_matrices
    }

    pub fn local_transform(&self, index: usize) -> Option<&Transform> {
        self.joints.get(index).map(|joint| &joint.local)
    }

    pub fn set_local_transform(&mut self, index: usize, transform: Transform) {
        if let Some(joint) = self.joints.get_mut(index) {
            joint.local = transform;
        }
    }

    pub fn joint_index_for_node(&self, node_index: usize) -> Option<usize> {
        self.node_to_joint.get(&node_index).copied()
    }

    /// Resets every joint to its rest transform.
    pub fn reset_to_rest(&mut self) {
        for joint in &mut self.joints {
            joint.local = joint.rest;
        }
        self.update_world(Matrix4::identity());
    }

    /// Applies the sampled channels of `clip` at `time` to the local joints.
    pub fn apply_clip(&mut self, clip: &AnimationClip, time: f32) {
        for channel in &clip.channels {
            let Some(&joint_index) = self.node_to_joint.get(&channel.node_index) else {
                continue;
            };
            let Some(value) = channel.sampler.sample(time) else {
                continue;
            };
            let Some(joint) = self.joints.get_mut(joint_index) else {
                continue;
            };

            match (channel.path, value) {
                (AnimationPath::Translation, AnimationValue::Vec3(value)) => {
                    joint.local.position = value;
                }
                (AnimationPath::Rotation, AnimationValue::Quat(value)) => {
                    joint.local.rotation = value.normalize();
                }
                (AnimationPath::Scale, AnimationValue::Vec3(value)) => {
                    joint.local.scale = value;
                }
                _ => {}
            }
        }
    }

    /// Recomputes world and skinning matrices from the current local joints.
    pub fn update_world(&mut self, root: Matrix4<f32>) {
        let count = self.joints.len();
        if count == 0 {
            self.skin_matrices.clear();
            return;
        }

        let locals = self
            .joints
            .iter()
            .map(|joint| joint.local.to_matrix())
            .collect::<Vec<_>>();
        let parents = self
            .joints
            .iter()
            .map(|joint| joint.parent)
            .collect::<Vec<_>>();

        let mut world = vec![Matrix4::identity(); count];
        let mut computed = vec![false; count];
        for index in 0..count {
            compute_world(index, &parents, &locals, root, &mut world, &mut computed);
        }

        for (index, joint) in self.joints.iter_mut().enumerate() {
            joint.world = world[index];
        }

        self.skin_matrices.resize(count, Matrix4::identity());
        for index in 0..count {
            let inverse_bind = self
                .inverse_bind_matrices
                .get(index)
                .copied()
                .unwrap_or_else(Matrix4::identity);
            self.skin_matrices[index] = world[index] * inverse_bind;
        }
    }
}

fn compute_world(
    index: usize,
    parents: &[Option<usize>],
    locals: &[Matrix4<f32>],
    root: Matrix4<f32>,
    world: &mut [Matrix4<f32>],
    computed: &mut [bool],
) -> Matrix4<f32> {
    if computed[index] {
        return world[index];
    }

    computed[index] = true;
    let matrix = match parents[index] {
        Some(parent) => {
            compute_world(parent, parents, locals, root, world, computed) * locals[index]
        }
        None => root * locals[index],
    };
    world[index] = matrix;
    matrix
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AnimationChannel;
    use cgmath::{Quaternion, Vector3};

    fn joint(node: usize, parent: Option<usize>, position: Vector3<f32>) -> JointDescriptor {
        JointDescriptor {
            node_index: node,
            name: format!("joint{node}"),
            parent,
            rest: Transform::new(
                position,
                Quaternion::new(1.0, 0.0, 0.0, 0.0),
                Vector3::new(1.0, 1.0, 1.0),
            ),
        }
    }

    fn descriptor() -> SkeletonDescriptor {
        SkeletonDescriptor {
            joints: vec![
                joint(0, None, Vector3::new(1.0, 0.0, 0.0)),
                joint(1, Some(0), Vector3::new(0.0, 1.0, 0.0)),
            ],
            inverse_bind_matrices: vec![Matrix4::identity(), Matrix4::identity()],
        }
    }

    #[test]
    fn parent_transforms_accumulate() {
        let skeleton = Skeleton::from_descriptor(&descriptor());

        assert_eq!(
            skeleton.joints()[1].world[3],
            cgmath::Vector4::new(1.0, 1.0, 0.0, 1.0)
        );
    }

    #[test]
    fn skin_matrices_use_inverse_bind_matrices() {
        let mut descriptor = descriptor();
        descriptor.inverse_bind_matrices[0] =
            Matrix4::from_translation(Vector3::new(1.0, 0.0, 0.0));
        let skeleton = Skeleton::from_descriptor(&descriptor);

        assert_eq!(
            skeleton.skin_matrices()[0][3],
            cgmath::Vector4::new(2.0, 0.0, 0.0, 1.0)
        );
    }

    #[test]
    fn applying_a_clip_changes_the_target_joint() {
        let mut skeleton = Skeleton::from_descriptor(&descriptor());
        let clip = AnimationClip::new(
            "Move",
            vec![AnimationChannel {
                node_index: 1,
                path: AnimationPath::Translation,
                sampler: crate::AnimationSampler::new(
                    vec![0.0],
                    vec![AnimationValue::Vec3(Vector3::new(4.0, 5.0, 6.0))],
                    crate::Interpolation::Linear,
                ),
            }],
        );

        skeleton.apply_clip(&clip, 0.0);
        skeleton.update_world(Matrix4::identity());

        assert_eq!(
            skeleton.local_transform(1).unwrap().position,
            Vector3::new(4.0, 5.0, 6.0)
        );
        assert_eq!(
            skeleton.joints()[1].world[3],
            cgmath::Vector4::new(5.0, 5.0, 6.0, 1.0)
        );
    }

    #[test]
    fn resetting_restores_the_rest_pose() {
        let mut skeleton = Skeleton::from_descriptor(&descriptor());
        skeleton.set_local_transform(
            0,
            Transform::new(
                Vector3::new(9.0, 9.0, 9.0),
                Quaternion::new(1.0, 0.0, 0.0, 0.0),
                Vector3::new(1.0, 1.0, 1.0),
            ),
        );

        skeleton.reset_to_rest();

        assert_eq!(
            skeleton.local_transform(0).unwrap().position,
            Vector3::new(1.0, 0.0, 0.0)
        );
    }
}
