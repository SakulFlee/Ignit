use cgmath::{Quaternion, Vector3};

/// The TRS property an animation channel targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimationPath {
    Translation,
    Rotation,
    Scale,
}

/// A sampled animation value.
#[derive(Debug, Clone)]
pub enum AnimationValue {
    Vec3(Vector3<f32>),
    Quat(Quaternion<f32>),
}

impl AnimationValue {
    fn lerp(&self, other: &Self, amount: f32) -> Self {
        match (self, other) {
            (Self::Vec3(a), Self::Vec3(b)) => Self::Vec3(a + (b - a) * amount),
            (Self::Quat(a), Self::Quat(b)) => Self::Quat(a.slerp(*b, amount)),
            _ => self.clone(),
        }
    }

    /// Component-wise addition, used by the cubic spline evaluator.
    fn add(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Vec3(a), Self::Vec3(b)) => Self::Vec3(a + b),
            (Self::Quat(a), Self::Quat(b)) => Self::Quat(Quaternion::new(
                a.s + b.s,
                a.v.x + b.v.x,
                a.v.y + b.v.y,
                a.v.z + b.v.z,
            )),
            _ => self.clone(),
        }
    }

    /// Component-wise scaling, used by the cubic spline evaluator.
    fn scale(&self, factor: f32) -> Self {
        match self {
            Self::Vec3(value) => Self::Vec3(value * factor),
            Self::Quat(value) => Self::Quat(Quaternion::new(
                value.s * factor,
                value.v.x * factor,
                value.v.y * factor,
                value.v.z * factor,
            )),
        }
    }

    /// Normalizes a quaternion result; vectors are returned unchanged.
    ///
    /// Cubic spline output is not unit length, and skinned rotation channels
    /// must stay normalized before they reach the joint matrices.
    fn normalized(&self) -> Self {
        match self {
            Self::Quat(value) => {
                let magnitude = (value.s * value.s
                    + value.v.x * value.v.x
                    + value.v.y * value.v.y
                    + value.v.z * value.v.z)
                    .sqrt();
                if magnitude > f32::EPSILON {
                    Self::Quat(Quaternion::new(
                        value.s / magnitude,
                        value.v.x / magnitude,
                        value.v.y / magnitude,
                        value.v.z / magnitude,
                    ))
                } else {
                    self.clone()
                }
            }
            Self::Vec3(_) => self.clone(),
        }
    }
}

/// Keyframe interpolation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Interpolation {
    Step,
    Linear,
    /// Hermite interpolation with per-key in/out tangents.
    CubicSpline,
}

/// Keyframe times and values for one channel.
#[derive(Debug, Clone)]
pub struct AnimationSampler {
    pub times: Vec<f32>,
    pub values: Vec<AnimationValue>,
    /// In-tangents of every keyframe; only populated for
    /// [`Interpolation::CubicSpline`].
    pub in_tangents: Vec<AnimationValue>,
    /// Out-tangents of every keyframe; only populated for
    /// [`Interpolation::CubicSpline`].
    pub out_tangents: Vec<AnimationValue>,
    pub interpolation: Interpolation,
}

impl AnimationSampler {
    pub fn new(times: Vec<f32>, values: Vec<AnimationValue>, interpolation: Interpolation) -> Self {
        Self {
            times,
            values,
            in_tangents: Vec::new(),
            out_tangents: Vec::new(),
            interpolation,
        }
    }

    /// Builds a cubic spline sampler from per-key values and tangents.
    pub fn new_cubic_spline(
        times: Vec<f32>,
        values: Vec<AnimationValue>,
        in_tangents: Vec<AnimationValue>,
        out_tangents: Vec<AnimationValue>,
    ) -> Self {
        Self {
            times,
            values,
            in_tangents,
            out_tangents,
            interpolation: Interpolation::CubicSpline,
        }
    }

    /// Samples the channel at `time` in seconds.
    pub fn sample(&self, time: f32) -> Option<AnimationValue> {
        if self.times.is_empty() || self.values.is_empty() {
            return None;
        }

        if time <= self.times[0] {
            return self.values.first().cloned();
        }

        let last_index = self.times.len().saturating_sub(1);
        if time >= self.times[last_index] {
            return self.values.get(last_index).cloned();
        }

        let index = self.times.partition_point(|&key_time| key_time <= time) - 1;
        let next = index + 1;
        let first = self.values.get(index)?;
        let second = self.values.get(next)?;

        if self.interpolation == Interpolation::Step {
            return Some(first.clone());
        }

        let span = self.times[next] - self.times[index];
        if span.abs() <= f32::EPSILON {
            return Some(first.clone());
        }

        let amount = ((time - self.times[index]) / span).clamp(0.0, 1.0);

        if self.interpolation == Interpolation::CubicSpline {
            let out_tangent = self.out_tangents.get(index)?;
            let in_tangent = self.in_tangents.get(next)?;
            return Some(
                hermite(first, out_tangent, second, in_tangent, span, amount).normalized(),
            );
        }

        Some(first.lerp(second, amount))
    }
}

/// Cubic Hermite interpolation between two keys.
///
/// glTF stores the tangents scaled by seconds, so they are multiplied by the
/// span before they contribute.
fn hermite(
    first: &AnimationValue,
    first_out_tangent: &AnimationValue,
    second: &AnimationValue,
    second_in_tangent: &AnimationValue,
    span: f32,
    amount: f32,
) -> AnimationValue {
    let squared = amount * amount;
    let cubed = squared * amount;
    let h00 = 2.0 * cubed - 3.0 * squared + 1.0;
    let h10 = cubed - 2.0 * squared + amount;
    let h01 = -2.0 * cubed + 3.0 * squared;
    let h11 = cubed - squared;

    first
        .scale(h00)
        .add(&first_out_tangent.scale(span * h10))
        .add(&second.scale(h01))
        .add(&second_in_tangent.scale(span * h11))
}

/// One animated property of one node.
#[derive(Debug, Clone)]
pub struct AnimationChannel {
    pub node_index: usize,
    pub path: AnimationPath,
    pub sampler: AnimationSampler,
}

/// A named set of animation channels.
#[derive(Debug, Clone)]
pub struct AnimationClip {
    pub name: String,
    pub duration: f32,
    pub channels: Vec<AnimationChannel>,
}

impl AnimationClip {
    pub fn new(name: impl Into<String>, channels: Vec<AnimationChannel>) -> Self {
        let duration = channels
            .iter()
            .filter_map(|channel| channel.sampler.times.last().copied())
            .fold(0.0, f32::max);

        Self {
            name: name.into(),
            duration,
            channels,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cgmath::InnerSpace;

    fn sampler(interpolation: Interpolation) -> AnimationSampler {
        AnimationSampler::new(
            vec![0.0, 1.0, 2.0],
            vec![
                AnimationValue::Vec3(Vector3::new(0.0, 0.0, 0.0)),
                AnimationValue::Vec3(Vector3::new(2.0, 0.0, 0.0)),
                AnimationValue::Vec3(Vector3::new(2.0, 4.0, 0.0)),
            ],
            interpolation,
        )
    }

    fn vec3(value: AnimationValue) -> Vector3<f32> {
        match value {
            AnimationValue::Vec3(value) => value,
            AnimationValue::Quat(_) => panic!("expected vec3"),
        }
    }

    #[test]
    fn step_interpolation_keeps_the_previous_key() {
        let value = sampler(Interpolation::Step).sample(0.75).unwrap();

        assert_eq!(vec3(value), Vector3::new(0.0, 0.0, 0.0));
    }

    #[test]
    fn linear_interpolation_blends_between_keys() {
        let value = sampler(Interpolation::Linear).sample(0.5).unwrap();

        assert_eq!(vec3(value), Vector3::new(1.0, 0.0, 0.0));
    }

    #[test]
    fn cubic_spline_uses_key_tangents() {
        let sampler = AnimationSampler::new_cubic_spline(
            vec![0.0, 1.0],
            vec![
                AnimationValue::Vec3(Vector3::new(0.0, 0.0, 0.0)),
                AnimationValue::Vec3(Vector3::new(1.0, 0.0, 0.0)),
            ],
            vec![
                AnimationValue::Vec3(Vector3::new(0.0, 0.0, 0.0)),
                AnimationValue::Vec3(Vector3::new(0.0, 0.0, 0.0)),
            ],
            vec![
                AnimationValue::Vec3(Vector3::new(1.0, 0.0, 0.0)),
                AnimationValue::Vec3(Vector3::new(0.0, 0.0, 0.0)),
            ],
        );

        // At the midpoint the outgoing tangent of the first key contributes
        // 0.125 * span and the second key contributes 0.5.
        let value = vec3(sampler.sample(0.5).unwrap());
        assert!((value.x - 0.625).abs() < 1e-6, "{value:?}");
    }

    #[test]
    fn cubic_spline_normalizes_quaternion_results() {
        let identity = AnimationValue::Quat(Quaternion::new(1.0, 0.0, 0.0, 0.0));
        let sampler = AnimationSampler::new_cubic_spline(
            vec![0.0, 1.0],
            vec![identity.clone(), identity.clone()],
            vec![
                AnimationValue::Quat(Quaternion::new(0.0, 0.0, 0.0, 0.0)),
                AnimationValue::Quat(Quaternion::new(0.0, 0.0, 0.0, 0.0)),
            ],
            vec![
                AnimationValue::Quat(Quaternion::new(0.0, 4.0, 0.0, 0.0)),
                AnimationValue::Quat(Quaternion::new(0.0, 0.0, 0.0, 0.0)),
            ],
        );

        let AnimationValue::Quat(value) = sampler.sample(0.5).unwrap() else {
            panic!("expected a sampled quaternion");
        };
        assert!((value.magnitude() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn sampling_clamps_outside_the_key_range() {
        let sampler = sampler(Interpolation::Linear);

        assert_eq!(
            vec3(sampler.sample(-1.0).unwrap()),
            Vector3::new(0.0, 0.0, 0.0)
        );
        assert_eq!(
            vec3(sampler.sample(10.0).unwrap()),
            Vector3::new(2.0, 4.0, 0.0)
        );
    }

    #[test]
    fn empty_samplers_return_none() {
        let sampler = AnimationSampler::new(Vec::new(), Vec::new(), Interpolation::Linear);

        assert!(sampler.sample(0.0).is_none());
    }

    #[test]
    fn clip_duration_uses_the_last_key() {
        let clip = AnimationClip::new(
            "Walk",
            vec![AnimationChannel {
                node_index: 0,
                path: AnimationPath::Translation,
                sampler: sampler(Interpolation::Linear),
            }],
        );

        assert_eq!(clip.duration, 2.0);
    }
}
