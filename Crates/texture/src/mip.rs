//! CPU mip-chain generation for 8-bit textures.
//!
//! Image files only carry the base level. Building the chain here keeps the
//! upload path simple: the compact chain is concatenated behind the base level
//! and [`crate::Texture::from_data`] uploads one level at a time.

use std::sync::OnceLock;

use wgpu::{Extent3d, TextureDimension};

/// Hard upper bound on chain length.
///
/// WGPU's default limits allow 16 mip levels; the largest 2D texture most
/// devices accept is 16384 px, which needs 15 levels.
const MAX_MIP_LEVELS: u32 = 16;

/// Number of levels in a full chain for the given extents.
///
/// A 1x1 texture has one level, a 4x4 texture has three (4, 2, 1), and a 3x3
/// texture has two (3, 1) because every level halves the extents, rounding
/// down and clamping at one. WGPU's [`Extent3d::max_mips`] defines the same
/// rule.
pub fn mip_level_count(width: u32, height: u32) -> u32 {
    let extent = Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    extent
        .max_mips(TextureDimension::D2)
        .clamp(1, MAX_MIP_LEVELS)
}

/// Builds a compact 8-bit mip chain, base level first.
///
/// `channels` is the number of interleaved bytes per pixel (1, 2, 3, or 4).
/// With `srgb`, the first three channels are filtered in linear space and
/// re-encoded; alpha and single/two-channel data stay linear.
///
/// Returns `None` when the extents or the pixel buffer are inconsistent, or
/// when the channel count is unsupported.
pub fn build_mip_chain_8bit(
    pixels: &[u8],
    width: u32,
    height: u32,
    channels: u32,
    srgb: bool,
) -> Option<(Vec<u8>, u32)> {
    if width == 0 || height == 0 || !(1..=4).contains(&channels) {
        return None;
    }

    let channels = channels as usize;
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(channels))?;
    if pixels.len() != expected {
        return None;
    }

    let levels = mip_level_count(width, height);
    let mut chain = Vec::with_capacity(expected + expected / 3 + channels);
    chain.extend_from_slice(pixels);

    let mut source = pixels.to_vec();
    let mut source_width = width;
    let mut source_height = height;
    for _ in 1..levels {
        let (next, next_width, next_height) =
            downsample(&source, source_width, source_height, channels, srgb);
        chain.extend_from_slice(&next);
        source = next;
        source_width = next_width;
        source_height = next_height;
    }

    Some((chain, levels))
}

/// Box-filters one level into the next.
///
/// Destination pixels average the full source area they cover, so odd extents
/// keep their last row and column instead of dropping them.
fn downsample(
    source: &[u8],
    source_width: u32,
    source_height: u32,
    channels: usize,
    srgb: bool,
) -> (Vec<u8>, u32, u32) {
    let width = (source_width / 2).max(1);
    let height = (source_height / 2).max(1);
    let mut pixels = vec![0u8; width as usize * height as usize * channels];
    let srgb_channels = if srgb { 3.min(channels) } else { 0 };

    for y in 0..height {
        let y0 = y as u64 * source_height as u64 / height as u64;
        let y1 = ((y as u64 + 1) * source_height as u64 / height as u64)
            .max(y0 + 1)
            .min(source_height as u64);

        for x in 0..width {
            let x0 = x as u64 * source_width as u64 / width as u64;
            let x1 = ((x as u64 + 1) * source_width as u64 / width as u64)
                .max(x0 + 1)
                .min(source_width as u64);
            let samples = (x1 - x0) as u32 * (y1 - y0) as u32;
            let destination = (y as usize * width as usize + x as usize) * channels;

            for channel in 0..channels {
                if channel < srgb_channels {
                    let mut sum = 0.0f32;
                    for source_y in y0..y1 {
                        for source_x in x0..x1 {
                            let index = ((source_y as u32 * source_width + source_x as u32)
                                as usize)
                                * channels
                                + channel;
                            sum += srgb_to_linear(source[index]);
                        }
                    }
                    pixels[destination + channel] = linear_to_srgb(sum / samples as f32);
                } else {
                    let mut sum = 0u32;
                    for source_y in y0..y1 {
                        for source_x in x0..x1 {
                            let index = ((source_y as u32 * source_width + source_x as u32)
                                as usize)
                                * channels
                                + channel;
                            sum += source[index] as u32;
                        }
                    }
                    pixels[destination + channel] = ((sum + samples / 2) / samples).min(255) as u8;
                }
            }
        }
    }

    (pixels, width, height)
}

/// Decodes one sRGB-encoded channel value into linear space.
fn srgb_to_linear(value: u8) -> f32 {
    static TABLE: OnceLock<[f32; 256]> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut table = [0.0f32; 256];
        for (value, slot) in table.iter_mut().enumerate() {
            let encoded = value as f32 / 255.0;
            *slot = if encoded <= 0.040_45 {
                encoded / 12.92
            } else {
                ((encoded + 0.055) / 1.055).powf(2.4)
            };
        }
        table
    });

    table[value as usize]
}

/// Encodes one linear channel value into sRGB space.
fn linear_to_srgb(value: f32) -> u8 {
    let linear = value.clamp(0.0, 1.0);
    let encoded = if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };

    (encoded * 255.0).round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_length_rounds_down_to_one_by_one() {
        assert_eq!(mip_level_count(1, 1), 1);
        assert_eq!(mip_level_count(4, 4), 3);
        assert_eq!(mip_level_count(3, 3), 2);
        assert_eq!(mip_level_count(1024, 1024), 11);
        assert_eq!(mip_level_count(16384, 1), 15);
        assert_eq!(mip_level_count(1, 0), 1);
    }

    #[test]
    fn a_uniform_image_produces_a_uniform_chain() {
        let pixels = vec![0x40u8; 4 * 4 * 4];
        let (chain, levels) = build_mip_chain_8bit(&pixels, 4, 4, 4, false).expect("chain builds");

        assert_eq!(levels, 3);
        // 4x4 + 2x2 + 1x1, four bytes per pixel.
        assert_eq!(chain.len(), (16 + 4 + 1) * 4);
        assert!(chain.iter().all(|byte| *byte == 0x40));
    }

    #[test]
    fn odd_extents_keep_the_last_row_and_column() {
        // A 3x3 R8 image where only the final row and column are white.
        let pixels = [
            0, 0, 255, //
            0, 0, 255, //
            255, 255, 255,
        ];
        let (chain, levels) = build_mip_chain_8bit(&pixels, 3, 3, 1, false).expect("chain builds");

        assert_eq!(levels, 2);
        assert_eq!(chain.len(), 9 + 1);
        // Five of nine samples are white: 5 / 9 * 255 rounds to 142.
        assert_eq!(chain[9], 142);
    }

    #[test]
    fn srgb_levels_are_filtered_in_linear_space() {
        // Two black and two white pixels average to mid-grey, which in sRGB is
        // encoded as 188 rather than the 128 a naive byte average produces.
        let mut pixels = vec![255u8; 2 * 2 * 4];
        for index in [0, 1] {
            pixels[index * 4] = 0;
            pixels[index * 4 + 1] = 0;
            pixels[index * 4 + 2] = 0;
        }

        let (chain, levels) = build_mip_chain_8bit(&pixels, 2, 2, 4, true).expect("chain builds");

        assert_eq!(levels, 2);
        assert_eq!(&chain[16..20], &[188, 188, 188, 255]);
    }

    #[test]
    fn srgb_alpha_stays_linear() {
        // Opaque and fully transparent pixels: linear alpha averaging yields 128.
        let pixels = [
            255, 255, 255, 0, //
            255, 255, 255, 255,
        ];
        let (chain, _) = build_mip_chain_8bit(&pixels, 2, 1, 4, true).expect("chain builds");

        assert_eq!(&chain[8..12], &[255, 255, 255, 128]);
    }

    #[test]
    fn inconsistent_buffers_are_rejected() {
        assert!(build_mip_chain_8bit(&[0; 15], 2, 2, 4, false).is_none());
        assert!(build_mip_chain_8bit(&[0; 4], 1, 1, 5, false).is_none());
        assert!(build_mip_chain_8bit(&[0; 4], 0, 1, 4, false).is_none());
    }

    #[test]
    fn single_pixel_images_keep_one_level() {
        let (chain, levels) = build_mip_chain_8bit(&[1, 2, 3, 4], 1, 1, 4, true).expect("chain");

        assert_eq!(levels, 1);
        assert_eq!(chain, [1, 2, 3, 4]);
    }
}
