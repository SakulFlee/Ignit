//! Minimal DDS loader for block-compressed 2D textures.
//!
//! Only BC1, BC3, BC5, and BC7 are supported. The file must be a single 2D
//! image; cube maps, volume textures, and texture arrays are rejected because
//! the engine's `TextureDescriptor::File` path creates 2D textures.

use wgpu::TextureFormat;

use crate::{TextureError, mip::mip_level_count};

const MAGIC: &[u8; 4] = b"DDS ";
const HEADER_SIZE: u32 = 124;
const PIXEL_FORMAT_SIZE: u32 = 32;
const DDPF_FOURCC: u32 = 0x4;
const DDSD_MIPMAPCOUNT: u32 = 0x2_0000;
const CAPS2_CUBEMAP: u32 = 0x200;
const CAPS2_VOLUME: u32 = 0x20_0000;
const DX10_RESOURCE_DIMENSION_TEXTURE2D: u32 = 3;

/// A parsed block-compressed DDS image.
#[derive(Debug, Clone)]
pub struct DdsImage {
    pub format: TextureFormat,
    pub width: u32,
    pub height: u32,
    pub mip_levels: u32,
    /// Tightly packed image data, base level first.
    pub data: Vec<u8>,
}

/// The upload layout of one block-compressed mip level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DdsLevel {
    /// Physical width: the virtual width rounded up to whole blocks.
    pub width: u32,
    /// Physical height: the virtual height rounded up to whole blocks.
    pub height: u32,
    pub bytes_per_row: u32,
    pub rows: u32,
    /// Number of tightly packed bytes this level occupies.
    pub bytes: usize,
}

fn dds_error(message: impl Into<String>) -> TextureError {
    TextureError::Dds(message.into())
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

/// Parses the DDS header and validates that the payload contains every level.
pub fn parse_dds(bytes: &[u8]) -> Result<DdsImage, TextureError> {
    if bytes.len() < 128 {
        return Err(dds_error("file is shorter than the DDS header"));
    }
    if &bytes[0..4] != MAGIC {
        return Err(dds_error("missing DDS magic"));
    }
    if read_u32(bytes, 4) != HEADER_SIZE {
        return Err(dds_error("unexpected DDS header size"));
    }
    if read_u32(bytes, 76) != PIXEL_FORMAT_SIZE {
        return Err(dds_error("unexpected DDS pixel format size"));
    }

    let height = read_u32(bytes, 12);
    let width = read_u32(bytes, 16);
    if width == 0 || height == 0 {
        return Err(dds_error("DDS image has a zero extent"));
    }

    let caps2 = read_u32(bytes, 112);
    if caps2 & CAPS2_CUBEMAP != 0 {
        return Err(dds_error("cube-map DDS files are not supported"));
    }
    if caps2 & CAPS2_VOLUME != 0 {
        return Err(dds_error("volume DDS files are not supported"));
    }

    let pixel_flags = read_u32(bytes, 80);
    if pixel_flags & DDPF_FOURCC == 0 {
        return Err(dds_error(
            "uncompressed DDS files are not supported; use a BC1/3/5/7 file",
        ));
    }

    let fourcc = [bytes[84], bytes[85], bytes[86], bytes[87]];
    let (format, data_start) = if &fourcc == b"DX10" {
        if bytes.len() < 148 {
            return Err(dds_error("truncated DDS DX10 header"));
        }
        let dxgi = read_u32(bytes, 128);
        let dimension = read_u32(bytes, 132);
        let array_size = read_u32(bytes, 140);
        if dimension != DX10_RESOURCE_DIMENSION_TEXTURE2D {
            return Err(dds_error("only 2D DDS textures are supported"));
        }
        if array_size != 1 {
            return Err(dds_error("texture-array DDS files are not supported"));
        }
        let format = dxgi_format(dxgi)
            .ok_or_else(|| dds_error(format!("unsupported DXGI format {dxgi}")))?;
        (format, 148usize)
    } else {
        let format = legacy_format(&fourcc).ok_or_else(|| {
            dds_error(format!(
                "unsupported DDS FourCC {:?}",
                String::from_utf8_lossy(&fourcc)
            ))
        })?;
        (format, 128usize)
    };

    let max_levels = mip_level_count(width, height);
    let mip_flag = read_u32(bytes, 8) & DDSD_MIPMAPCOUNT != 0;
    let mip_levels = if mip_flag {
        read_u32(bytes, 28).clamp(1, max_levels)
    } else {
        1
    };
    let total = chain_len(format, width, height, mip_levels)?;
    let end = data_start
        .checked_add(total)
        .ok_or_else(|| dds_error("DDS payload size overflowed"))?;
    if bytes.len() < end {
        return Err(dds_error(format!(
            "DDS payload is truncated: expected {total} bytes for {mip_levels} levels, got {}",
            bytes.len().saturating_sub(data_start)
        )));
    }

    Ok(DdsImage {
        format,
        width,
        height,
        mip_levels,
        data: bytes[data_start..end].to_vec(),
    })
}

impl DdsImage {
    /// Upload layout of `level`, including the block-rounded extents.
    pub(crate) fn level(&self, level: u32) -> Option<DdsLevel> {
        level_layout(
            self.format,
            (self.width >> level).max(1),
            (self.height >> level).max(1),
        )
    }
}

fn legacy_format(fourcc: &[u8; 4]) -> Option<TextureFormat> {
    match fourcc {
        b"DXT1" => Some(TextureFormat::Bc1RgbaUnormSrgb),
        b"DXT5" => Some(TextureFormat::Bc3RgbaUnormSrgb),
        b"ATI2" | b"BC5U" => Some(TextureFormat::Bc5RgUnorm),
        b"BC5S" => Some(TextureFormat::Bc5RgSnorm),
        _ => None,
    }
}

fn dxgi_format(dxgi: u32) -> Option<TextureFormat> {
    match dxgi {
        71 => Some(TextureFormat::Bc1RgbaUnorm),
        72 => Some(TextureFormat::Bc1RgbaUnormSrgb),
        77 => Some(TextureFormat::Bc3RgbaUnorm),
        78 => Some(TextureFormat::Bc3RgbaUnormSrgb),
        83 => Some(TextureFormat::Bc5RgUnorm),
        84 => Some(TextureFormat::Bc5RgSnorm),
        98 => Some(TextureFormat::Bc7RgbaUnorm),
        99 => Some(TextureFormat::Bc7RgbaUnormSrgb),
        _ => None,
    }
}

/// Block-rounded layout of one mip level.
pub(crate) fn level_layout(format: TextureFormat, width: u32, height: u32) -> Option<DdsLevel> {
    let (block_width, block_height) = format.block_dimensions();
    let block_bytes = format.block_copy_size(None)? as usize;
    let blocks_x = width.div_ceil(block_width).max(1);
    let blocks_y = height.div_ceil(block_height).max(1);

    Some(DdsLevel {
        width: blocks_x * block_width,
        height: blocks_y * block_height,
        bytes_per_row: blocks_x * block_bytes as u32,
        rows: blocks_y,
        bytes: blocks_x as usize * blocks_y as usize * block_bytes,
    })
}

fn chain_len(
    format: TextureFormat,
    width: u32,
    height: u32,
    mip_levels: u32,
) -> Result<usize, TextureError> {
    let mut total = 0usize;
    for level in 0..mip_levels.max(1) {
        let layout = level_layout(format, (width >> level).max(1), (height >> level).max(1))
            .ok_or_else(|| dds_error(format!("{format:?} is not a block-compressed format")))?;
        total = total
            .checked_add(layout.bytes)
            .ok_or_else(|| dds_error("DDS mip chain size overflowed"))?;
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(fourcc: &[u8; 4], width: u32, height: u32, mips: u32, caps2: u32) -> Vec<u8> {
        let mut bytes = vec![0u8; 128];
        bytes[0..4].copy_from_slice(MAGIC);
        bytes[4..8].copy_from_slice(&HEADER_SIZE.to_le_bytes());
        bytes[12..16].copy_from_slice(&height.to_le_bytes());
        bytes[16..20].copy_from_slice(&width.to_le_bytes());
        let flags = if mips > 1 { DDSD_MIPMAPCOUNT } else { 0 };
        bytes[8..12].copy_from_slice(&flags.to_le_bytes());
        bytes[28..32].copy_from_slice(&mips.to_le_bytes());
        bytes[76..80].copy_from_slice(&PIXEL_FORMAT_SIZE.to_le_bytes());
        bytes[80..84].copy_from_slice(&DDPF_FOURCC.to_le_bytes());
        bytes[84..88].copy_from_slice(fourcc);
        bytes[112..116].copy_from_slice(&caps2.to_le_bytes());
        bytes
    }

    fn with_payload(mut header: Vec<u8>, len: usize) -> Vec<u8> {
        header.extend(std::iter::repeat_n(0x55u8, len));
        header
    }

    fn with_dx10(
        mut header: Vec<u8>,
        dxgi: u32,
        dimension: u32,
        array_size: u32,
        len: usize,
    ) -> Vec<u8> {
        header[84..88].copy_from_slice(b"DX10");
        header.extend_from_slice(&dxgi.to_le_bytes());
        header.extend_from_slice(&dimension.to_le_bytes());
        header.extend_from_slice(&0u32.to_le_bytes());
        header.extend_from_slice(&array_size.to_le_bytes());
        header.extend_from_slice(&0u32.to_le_bytes());
        header.extend(std::iter::repeat_n(0x55u8, len));
        header
    }

    #[test]
    fn dxt1_parses_one_level() {
        let image = parse_dds(&with_payload(header(b"DXT1", 4, 4, 1, 0), 8)).expect("DXT1");
        assert_eq!(image.format, TextureFormat::Bc1RgbaUnormSrgb);
        assert_eq!((image.width, image.height), (4, 4));
        assert_eq!(image.mip_levels, 1);
        assert_eq!(image.data.len(), 8);
    }

    #[test]
    fn dxt5_parses_a_two_level_chain() {
        // 8x8 = 2x2 blocks * 16 bytes, then 4x4 = 1 block * 16 bytes.
        let image = parse_dds(&with_payload(header(b"DXT5", 8, 8, 2, 0), 80)).expect("DXT5");
        assert_eq!(image.format, TextureFormat::Bc3RgbaUnormSrgb);
        assert_eq!(image.mip_levels, 2);
        assert_eq!(image.data.len(), 80);
    }

    #[test]
    fn dx10_bc7_uses_the_explicit_format() {
        let image = parse_dds(&with_dx10(
            header(b"DX10", 8, 4, 1, 0),
            99,
            DX10_RESOURCE_DIMENSION_TEXTURE2D,
            1,
            32,
        ))
        .expect("BC7");
        assert_eq!(image.format, TextureFormat::Bc7RgbaUnormSrgb);
        assert_eq!((image.width, image.height), (8, 4));
        assert_eq!(image.data.len(), 32);
    }

    #[test]
    fn mip_count_is_clamped_to_the_full_chain() {
        // A 4x4 BC1 image can have at most three levels.
        let image = parse_dds(&with_payload(header(b"DXT1", 4, 4, 9, 0), 24)).expect("DXT1");
        assert_eq!(image.mip_levels, 3);
        assert_eq!(image.data.len(), 24);
    }

    #[test]
    fn mip_count_requires_the_header_flag() {
        let mut bytes = with_payload(header(b"DXT1", 4, 4, 9, 0), 8);
        // The helper sets DDSD_MIPMAPCOUNT when the count is above one.
        bytes[8..12].copy_from_slice(&0u32.to_le_bytes());

        let image = parse_dds(&bytes).expect("DXT1");
        assert_eq!(image.mip_levels, 1);
        assert_eq!(image.data.len(), 8);
    }

    #[test]
    fn level_layout_rounds_extents_to_blocks() {
        let layout = level_layout(TextureFormat::Bc1RgbaUnorm, 5, 3).expect("BC1");
        assert_eq!(layout.width, 8);
        assert_eq!(layout.height, 4);
        assert_eq!(layout.bytes_per_row, 16);
        assert_eq!(layout.rows, 1);
        assert_eq!(layout.bytes, 16);
    }

    #[test]
    fn rejects_bad_headers_and_unsupported_files() {
        assert!(parse_dds(&[]).is_err());

        let mut bad_magic = with_payload(header(b"DXT1", 4, 4, 1, 0), 8);
        bad_magic[0] = b'X';
        assert!(parse_dds(&bad_magic).is_err());

        assert!(parse_dds(&with_payload(header(b"ABCD", 4, 4, 1, 0), 8)).is_err());
        assert!(parse_dds(&with_payload(header(b"DXT1", 4, 4, 1, CAPS2_CUBEMAP), 8)).is_err());
        assert!(parse_dds(&with_payload(header(b"DXT1", 4, 4, 1, CAPS2_VOLUME), 8)).is_err());
        assert!(parse_dds(&with_payload(header(b"DXT1", 4, 4, 1, 0), 4)).is_err());
    }

    #[test]
    fn rejects_non_2d_dx10_files() {
        assert!(parse_dds(&with_dx10(header(b"DX10", 8, 4, 1, 0), 98, 4, 1, 16,)).is_err());
        assert!(
            parse_dds(&with_dx10(
                header(b"DX10", 8, 4, 1, 0),
                98,
                DX10_RESOURCE_DIMENSION_TEXTURE2D,
                2,
                16,
            ))
            .is_err()
        );
    }
}
