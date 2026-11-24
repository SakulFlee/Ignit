use std::fmt::{Display, Formatter, Result as FmtResult};

use wgpu::TextureFormat;

#[derive(Debug)]
pub enum TextureError {
    ImageError(image::ImageError),
    IOError(std::io::Error),
    FsError(ignit_file_manager::FsError),
    /// A DDS file could not be parsed, or used an unsupported layout.
    Dds(String),
    /// The adapter does not support the required compressed format.
    UnsupportedFormat(TextureFormat),
    /// The supplied pixel data is smaller than the `width x height x
    /// depth_or_array_layers` extent requires. Writing it would leave the tail
    /// of the texture undefined, so this is rejected instead of uploaded.
    DataSizeMismatch {
        expected: usize,
        actual: usize,
        width: u32,
        height: u32,
        depth_or_array_layers: u32,
        bytes_per_pixel: u32,
    },
    /// A GPU texture readback failed, timed out, or could not be mapped.
    Readback(String),
}

impl Display for TextureError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::ImageError(e) => write!(f, "Image error: {e}"),
            Self::IOError(e) => write!(f, "IO error: {e}"),
            Self::FsError(e) => write!(f, "File system error: {e}"),
            Self::Dds(message) => write!(f, "DDS error: {message}"),
            Self::UnsupportedFormat(format) => {
                write!(f, "Adapter does not support texture format {format:?}")
            }
            Self::DataSizeMismatch {
                expected,
                actual,
                width,
                height,
                depth_or_array_layers,
                bytes_per_pixel,
            } => write!(
                f,
                "Texture data size mismatch: expected at least {expected} bytes \
                 ({width}x{height}x{depth_or_array_layers} @ {bytes_per_pixel} bytes/pixel), \
                 got {actual} bytes"
            ),
            Self::Readback(message) => write!(f, "Texture readback error: {message}"),
        }
    }
}

impl std::error::Error for TextureError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ImageError(e) => Some(e),
            Self::IOError(e) => Some(e),
            Self::FsError(e) => Some(e),
            Self::Dds(_)
            | Self::UnsupportedFormat(_)
            | Self::DataSizeMismatch { .. }
            | Self::Readback(_) => None,
        }
    }
}
