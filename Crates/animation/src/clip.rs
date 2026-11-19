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
}

/// Keyframe interpolation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Interpolation {
    Step,
    Linear,
}

/// Keyframe times and values for one channel.
#[derive(Debug, Clone)]
pub struct AnimationSampler {
    pub times: Vec<f32>,
    pub values: Vec<AnimationValue>,
    pub interpolation: Interpolation,
}

impl AnimationSampler {
    pub fn new(times: Vec<f32>, values: Vec<AnimationValue>, interpolation: Interpolation) -> Self {
        Self {
            times,
            values,
            interpolation,
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
        Some(first.lerp(second, amount))
    }
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
