use std::{
    error::Error,
    fmt::{Display, Formatter, Result},
};

use ignit_shader_core::ShaderError;

/// Errors produced while building a compute shader.
#[derive(Debug)]
pub enum ComputeShaderError {
    /// The shader module could not be assembled.
    Shader(ShaderError),
    /// A binding declared in the layout has no matching resource.
    MissingBinding { binding: u32 },
    /// Resources were supplied for a shader that declares no bindings.
    BindingsWithoutLayout,
    /// Binding arrays are not supported by the compute binding builder yet.
    UnsupportedBindingArray { binding: u32 },
}

impl From<ShaderError> for ComputeShaderError {
    fn from(error: ShaderError) -> Self {
        Self::Shader(error)
    }
}

impl Display for ComputeShaderError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Self::Shader(error) => write!(f, "Compute shader error: {error}"),
            Self::MissingBinding { binding } => {
                write!(f, "Missing compute binding {binding}")
            }
            Self::BindingsWithoutLayout => {
                write!(
                    f,
                    "Compute resources were supplied without a binding layout"
                )
            }
            Self::UnsupportedBindingArray { binding } => {
                write!(f, "Compute binding {binding} declares an unsupported array")
            }
        }
    }
}

impl Error for ComputeShaderError {}
