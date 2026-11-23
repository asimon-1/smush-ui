use super::{
    bntx_error,
    format::{
        ChannelFormat, ChannelSelector, ImageFormat, SurfaceDimension, TextureDimension, TileMode,
        TypeFormat,
    },
    model::{Texture, TextureInfo},
    swizzle::SurfaceLayout,
    zeroed_bytes,
};
use binrw::BinResult;
use png::{BitDepth, ColorType, Decoder, Encoder, Transformations};
use rusty_dds::{
    AlphaMode, D3D10ResourceDimension, Dds, DecodeContent, DxgiFormat, EncodeLayout, NewDxgiParams,
    SubresourceId,
};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

/// Whether decoded pixels expose stored channels or apply the BRTI channel selectors.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ChannelMode {
    Raw,
    Resolved,
}

/// Whether the in-process codec supports this BNTX image format.
pub fn supports_image_format(format: ImageFormat) -> bool {
    codec_format(format).is_ok()
}

/// A tightly packed, row-major RGBA8 image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl RgbaImage {
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> BinResult<Self> {
        let image = Self {
            width,
            height,
            pixels,
        };
        image.validate()?;
        Ok(image)
    }

    fn validate(&self) -> BinResult<()> {
        let expected = rgba_size(self.width, self.height)?;
        if self.pixels.len() != expected {
            return Err(bntx_error(
                0,
                format!(
                    "RGBA image has {} bytes, expected {expected}",
                    self.pixels.len()
                ),
            ));
        }
        Ok(())
    }
}

/// Compressed mip data ready to store in a BNTX texture.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedImage {
    pub image_data: Vec<u8>,
    pub mip_offsets: Vec<u64>,
    pub block_height_exponent: u8,
}

/// Decodes one texture mip through the pure-Rust BC codec.
pub fn decode_texture(
    texture: &Texture,
    mip_level: u32,
    channel_mode: ChannelMode,
) -> BinResult<RgbaImage> {
    validate_texture_profile(&texture.info)?;
    let mip_index =
        usize::try_from(mip_level).map_err(|_| bntx_error(0, "BNTX mip level is too large"))?;
    let &mip_offset = texture
        .mip_offsets
        .get(mip_index)
        .ok_or_else(|| bntx_error(0, format!("BNTX mip {mip_level} does not exist")))?;
    let base = SurfaceLayout::new(
        texture.info.format,
        texture.info.width,
        texture.info.height,
        texture.info.texture_layout.block_height_exponent(),
    )?;
    let surface = base.mip(mip_level);
    let surface_size = surface.swizzled_size()?;
    let start =
        usize::try_from(mip_offset).map_err(|_| bntx_error(0, "BNTX mip offset is too large"))?;
    let end = start
        .checked_add(surface_size)
        .filter(|&end| end <= texture.image_data.len())
        .ok_or_else(|| bntx_error(0, format!("BNTX mip {mip_level} is truncated")))?;
    if texture
        .mip_offsets
        .get(mip_index + 1)
        .is_some_and(|&next| next < end as u64)
    {
        return Err(bntx_error(
            0,
            format!("BNTX mip {mip_level} overlaps the next mip"),
        ));
    }

    let linear = surface.deswizzle(&texture.image_data[start..end])?;
    let (content, dxgi) = codec_format(texture.info.format)?;
    let mut dds = Dds::new_dxgi(NewDxgiParams {
        height: surface.height(),
        width: surface.width(),
        depth: None,
        format: dxgi,
        mipmap_levels: Some(1),
        array_layers: Some(1),
        caps2: None,
        is_cubemap: false,
        resource_dimension: D3D10ResourceDimension::Texture2D,
        alpha_mode: AlphaMode::Straight,
    })
    .map_err(|error| codec_error("create DDS surface", error))?;
    if dds.data.len() != linear.len() || dds.decode_content().ok() != Some(content) {
        return Err(bntx_error(
            0,
            "rusty_dds produced an unexpected surface layout",
        ));
    }
    dds.data.copy_from_slice(&linear);
    let decoded = dds
        .decode_rgba8(SubresourceId::mip_layer(0, 0))
        .map_err(|error| codec_error("decode texture", error))?;
    let mut image = RgbaImage::new(decoded.width, decoded.height, decoded.pixels)?;
    if channel_mode == ChannelMode::Resolved {
        resolve_channels(&mut image.pixels, texture.info.channels)?;
    }
    Ok(image)
}

/// Encodes an edited mip-zero image using an existing texture's format and layout.
pub fn encode_texture(
    texture: &Texture,
    image: &RgbaImage,
    channel_mode: ChannelMode,
) -> BinResult<EncodedImage> {
    validate_texture_profile(&texture.info)?;
    if image.width != texture.info.width || image.height != texture.info.height {
        return Err(bntx_error(
            0,
            format!(
                "PNG dimensions {}x{} do not match texture dimensions {}x{}",
                image.width, image.height, texture.info.width, texture.info.height
            ),
        ));
    }
    encode_image(
        texture.info.format,
        texture.info.channels,
        texture.info.texture_layout.block_height_exponent(),
        texture.mip_offsets.len(),
        image,
        channel_mode,
    )
}

/// Encodes a one-mip image with the canonical block height for a newly created texture.
pub fn encode_new_texture(
    format: ImageFormat,
    channels: [ChannelSelector; 4],
    image: &RgbaImage,
    channel_mode: ChannelMode,
) -> BinResult<EncodedImage> {
    let surface = SurfaceLayout::for_new_texture(format, image.width, image.height)?;
    encode_image(
        format,
        channels,
        surface.block_height_exponent(),
        1,
        image,
        channel_mode,
    )
}

/// Reads a PNG and normalizes it to tightly packed RGBA8 pixels.
pub fn read_png(bytes: &[u8]) -> BinResult<RgbaImage> {
    let mut decoder = Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(Transformations::normalize_to_color8() | Transformations::ALPHA);
    let mut reader = decoder
        .read_info()
        .map_err(|error| bntx_error(0, format!("invalid PNG: {error}")))?;
    let output_size = reader
        .output_buffer_size()
        .ok_or_else(|| bntx_error(0, "PNG dimensions exceed addressable memory"))?;
    let mut output = zeroed_bytes(output_size, "PNG decode buffer")?;
    let info = reader
        .next_frame(&mut output)
        .map_err(|error| bntx_error(0, format!("invalid PNG image data: {error}")))?;
    let output = &output[..info.buffer_size()];
    let pixels = to_rgba(output, info.color_type)?;
    RgbaImage::new(info.width, info.height, pixels)
}

/// Writes tightly packed RGBA8 pixels to a PNG byte vector.
pub fn write_png(image: &RgbaImage) -> BinResult<Vec<u8>> {
    image.validate()?;
    let mut bytes = Vec::new();
    {
        let mut encoder = Encoder::new(&mut bytes, image.width, image.height);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|error| bntx_error(0, format!("could not write PNG header: {error}")))?;
        writer
            .write_image_data(&image.pixels)
            .map_err(|error| bntx_error(0, format!("could not write PNG pixels: {error}")))?;
    }
    Ok(bytes)
}

fn encode_image(
    format: ImageFormat,
    channels: [ChannelSelector; 4],
    block_height_exponent: u8,
    mip_count: usize,
    image: &RgbaImage,
    channel_mode: ChannelMode,
) -> BinResult<EncodedImage> {
    image.validate()?;
    let mip_count =
        u32::try_from(mip_count).map_err(|_| bntx_error(0, "BNTX mip count exceeds u32"))?;
    let max_mips = u32::BITS - image.width.max(image.height).leading_zeros();
    if mip_count == 0 || mip_count > max_mips {
        return Err(bntx_error(
            0,
            format!("BNTX mip count {mip_count} exceeds the {max_mips} available levels"),
        ));
    }
    let (content, _) = codec_format(format)?;
    let mut source = image.pixels.clone();
    if channel_mode == ChannelMode::Resolved {
        unresolve_channels(&mut source, channels)?;
    }
    // rusty_dds normally emits opaque BC1 blocks; fit transparent blocks separately per mip.
    let mut bc1_pixels = (format.channel_format() == ChannelFormat::Bc1
        && source.chunks_exact(4).any(|pixel| pixel[3] < 128))
    .then(|| source.clone());
    // rusty_dds owns the temporary DDS container. This boundary accepts only
    // its validated 2D mip surfaces and copies the exact BC block prefix out.
    let dds = std::panic::catch_unwind(|| {
        Dds::encode_from_rgba8(
            &source,
            EncodeLayout::flat_2d(content, image.width, image.height).with_mips(mip_count),
        )
    })
    .map_err(|_| bntx_error(0, "rusty_dds panicked while encoding the texture"))?
    .map_err(|error| codec_error("encode texture", error))?;

    let base = SurfaceLayout::new(format, image.width, image.height, block_height_exponent)?;
    let mut image_data = Vec::new();
    let mut mip_offsets = Vec::with_capacity(mip_count as usize);
    for mip in 0..mip_count {
        let encoded = dds
            .surface(SubresourceId::mip_layer(mip, 0))
            .map_err(|error| codec_error("read encoded DDS surface", error))?;
        let surface = base.mip(mip);
        let linear_size = surface.linear_size()?;
        if (encoded.width, encoded.height) != (surface.width(), surface.height())
            || encoded.data.len() < linear_size
        {
            return Err(bntx_error(
                0,
                format!(
                    "rusty_dds returned mip {mip} as {}x{} with {} bytes; expected {}x{} with at least {} bytes",
                    encoded.width,
                    encoded.height,
                    encoded.data.len(),
                    surface.width(),
                    surface.height(),
                    linear_size
                ),
            ));
        }
        // rusty_dds sizes NPOT mip ranges by quartering the preceding DDS
        // range. The exact BC block payload is the prefix; the unused tail is
        // DDS container padding and must remain zero.
        let (blocks, padding) = encoded.data.split_at(linear_size);
        if padding.iter().any(|&byte| byte != 0) {
            return Err(bntx_error(
                0,
                format!("rusty_dds returned nonzero padding after mip {mip}"),
            ));
        }
        mip_offsets.push(image_data.len() as u64);
        if let Some(pixels) = &bc1_pixels
            && pixels.chunks_exact(4).any(|pixel| pixel[3] < 128)
        {
            let mut transparent_blocks = blocks.to_vec();
            apply_bc1_alpha(
                &mut transparent_blocks,
                surface.width(),
                surface.height(),
                pixels,
            );
            image_data.extend_from_slice(&surface.swizzle(&transparent_blocks)?);
        } else {
            image_data.extend_from_slice(&surface.swizzle(blocks)?);
        }
        if let Some(pixels) = &mut bc1_pixels
            && mip + 1 < mip_count
        {
            *pixels = downsample_rgba(pixels, surface.width(), surface.height());
        }
    }
    Ok(EncodedImage {
        image_data,
        mip_offsets,
        block_height_exponent,
    })
}

fn apply_bc1_alpha(blocks: &mut [u8], width: u32, height: u32, pixels: &[u8]) {
    let width = width as usize;
    let height = height as usize;
    let blocks_per_row = width.div_ceil(4);
    for (block_index, block) in blocks.chunks_exact_mut(8).enumerate() {
        let block_x = block_index % blocks_per_row * 4;
        let block_y = block_index / blocks_per_row * 4;
        let mut has_transparency = false;
        let mut opaque = Vec::new();
        for y in 0..4 {
            for x in 0..4 {
                if block_x + x >= width || block_y + y >= height {
                    continue;
                }
                let offset = ((block_y + y) * width + block_x + x) * 4;
                let pixel = &pixels[offset..offset + 4];
                if pixel[3] < 128 {
                    has_transparency = true;
                } else {
                    opaque.push([pixel[0], pixel[1], pixel[2]]);
                }
            }
        }
        if !has_transparency {
            continue;
        }
        let mut candidates = opaque.iter().copied().map(pack_rgb565).collect::<Vec<_>>();
        if candidates.is_empty() {
            candidates.push(0);
        }
        candidates.sort_unstable();
        candidates.dedup();
        let mut best = (u64::MAX, 0, 0);
        for &first in &candidates {
            for &second in candidates.iter().filter(|&&value| value >= first) {
                let palette = bc1_palette(first, second);
                let score = opaque
                    .iter()
                    .map(|pixel| {
                        palette
                            .iter()
                            .map(|color| color_error(*pixel, *color))
                            .min()
                            .unwrap_or(0)
                    })
                    .sum();
                best = best.min((score, first, second));
            }
        }
        let (_, first, second) = best;
        let palette = bc1_palette(first, second);
        block[..2].copy_from_slice(&first.to_le_bytes());
        block[2..4].copy_from_slice(&second.to_le_bytes());
        let mut indices = 0u32;
        for y in 0..4 {
            for x in 0..4 {
                let offset = 2 * (y * 4 + x);
                let index = if block_x + x >= width || block_y + y >= height {
                    3
                } else {
                    let pixel_offset = ((block_y + y) * width + block_x + x) * 4;
                    let pixel = &pixels[pixel_offset..pixel_offset + 4];
                    if pixel[3] < 128 {
                        3
                    } else {
                        palette
                            .iter()
                            .enumerate()
                            .min_by_key(|(_, color)| {
                                color_error([pixel[0], pixel[1], pixel[2]], **color)
                            })
                            .map(|(index, _)| index as u32)
                            .unwrap_or(0)
                    }
                };
                indices |= index << offset;
            }
        }
        block[4..8].copy_from_slice(&indices.to_le_bytes());
    }
}

fn bc1_palette(first: u16, second: u16) -> [[u8; 3]; 3] {
    let a = unpack_rgb565(first);
    let b = unpack_rgb565(second);
    let middle =
        std::array::from_fn(|channel| ((u16::from(a[channel]) + u16::from(b[channel])) / 2) as u8);
    [a, b, middle]
}

fn color_error(pixel: [u8; 3], color: [u8; 3]) -> u64 {
    (0..3)
        .map(|channel| {
            let difference = i64::from(pixel[channel]) - i64::from(color[channel]);
            (difference * difference) as u64
        })
        .sum()
}

fn pack_rgb565(color: [u8; 3]) -> u16 {
    (u16::from(color[0]) * 31 / 255) << 11
        | (u16::from(color[1]) * 63 / 255) << 5
        | (u16::from(color[2]) * 31 / 255)
}

fn unpack_rgb565(color: u16) -> [u8; 3] {
    let red = (color >> 11) & 31;
    let green = (color >> 5) & 63;
    let blue = color & 31;
    [
        ((red << 3) | (red >> 2)) as u8,
        ((green << 2) | (green >> 4)) as u8,
        ((blue << 3) | (blue >> 2)) as u8,
    ]
}

fn downsample_rgba(pixels: &[u8], width: u32, height: u32) -> Vec<u8> {
    // Use the same rounded 2x2 box filter as rusty_dds's generated mip chain.
    let width = width as usize;
    let height = height as usize;
    let next_width = (width / 2).max(1);
    let next_height = (height / 2).max(1);
    let mut next = Vec::with_capacity(next_width * next_height * 4);
    for y in 0..next_height {
        for x in 0..next_width {
            for channel in 0..4 {
                let mut sum = 0u32;
                for dy in 0..2 {
                    for dx in 0..2 {
                        let source_x = (x * 2 + dx).min(width - 1);
                        let source_y = (y * 2 + dy).min(height - 1);
                        sum += u32::from(pixels[(source_y * width + source_x) * 4 + channel]);
                    }
                }
                next.push(((sum + 2) / 4) as u8);
            }
        }
    }
    next
}

pub(crate) fn validate_texture_profile(info: &TextureInfo) -> BinResult<()> {
    if info.image_dimension != TextureDimension::TwoD
        || info.surface_dimension != SurfaceDimension::TwoD
        || info.tile_mode != TileMode::BlockLinear
        || info.swizzle != 0
        || info.sample_count != 1
        || info.depth != 1
        || info.array_count != 1
        || info.image_alignment != 0x200
    {
        return Err(bntx_error(
            0,
            "PNG conversion supports only 0x200-aligned unswizzled single-sample 2D NX textures",
        ));
    }
    codec_format(info.format)?;
    Ok(())
}

fn codec_format(format: ImageFormat) -> BinResult<(DecodeContent, DxgiFormat)> {
    match (format.channel_format(), format.type_format()) {
        (ChannelFormat::Bc1, TypeFormat::Unorm) => Ok((DecodeContent::Bc1, DxgiFormat::BC1_UNorm)),
        (ChannelFormat::Bc1, TypeFormat::Srgb) => {
            Ok((DecodeContent::Bc1, DxgiFormat::BC1_UNorm_sRGB))
        }
        (ChannelFormat::Bc4, TypeFormat::Unorm) => {
            Ok((DecodeContent::Bc4UNorm, DxgiFormat::BC4_UNorm))
        }
        (ChannelFormat::Bc5, TypeFormat::Unorm) => {
            Ok((DecodeContent::Bc5UNorm, DxgiFormat::BC5_UNorm))
        }
        (ChannelFormat::Bc7U, TypeFormat::Unorm) => Ok((DecodeContent::Bc7, DxgiFormat::BC7_UNorm)),
        (ChannelFormat::Bc7U, TypeFormat::Srgb) => {
            Ok((DecodeContent::Bc7, DxgiFormat::BC7_UNorm_sRGB))
        }
        _ => Err(bntx_error(
            0,
            format!("unsupported BNTX image format {:#010x}", format.to_raw()),
        )),
    }
}

fn resolve_channels(pixels: &mut [u8], channels: [ChannelSelector; 4]) -> BinResult<()> {
    for pixel in pixels.chunks_exact_mut(4) {
        let source = [pixel[0], pixel[1], pixel[2], pixel[3]];
        for (output, selector) in pixel.iter_mut().zip(channels) {
            *output = select_channel(source, selector)?;
        }
    }
    Ok(())
}

fn unresolve_channels(pixels: &mut [u8], channels: [ChannelSelector; 4]) -> BinResult<()> {
    for pixel in pixels.chunks_exact_mut(4) {
        let resolved = [pixel[0], pixel[1], pixel[2], pixel[3]];
        let mut stored = [0, 0, 0, 255];
        let mut assigned = [false; 4];
        for (output_index, selector) in channels.into_iter().enumerate() {
            let stored_index = match selector {
                ChannelSelector::Red => Some(0),
                ChannelSelector::Green => Some(1),
                ChannelSelector::Blue => Some(2),
                ChannelSelector::Alpha => Some(3),
                ChannelSelector::Zero | ChannelSelector::One => None,
                ChannelSelector::Unknown(value) => {
                    return Err(bntx_error(
                        0,
                        format!("unsupported BNTX channel selector {value}"),
                    ));
                }
            };
            if let Some(stored_index) = stored_index
                && !assigned[stored_index]
            {
                stored[stored_index] = resolved[output_index];
                assigned[stored_index] = true;
            }
        }
        pixel.copy_from_slice(&stored);
    }
    Ok(())
}

fn select_channel(source: [u8; 4], selector: ChannelSelector) -> BinResult<u8> {
    match selector {
        ChannelSelector::Zero => Ok(0),
        ChannelSelector::One => Ok(255),
        ChannelSelector::Red => Ok(source[0]),
        ChannelSelector::Green => Ok(source[1]),
        ChannelSelector::Blue => Ok(source[2]),
        ChannelSelector::Alpha => Ok(source[3]),
        ChannelSelector::Unknown(value) => Err(bntx_error(
            0,
            format!("unsupported BNTX channel selector {value}"),
        )),
    }
}

fn to_rgba(bytes: &[u8], color: ColorType) -> BinResult<Vec<u8>> {
    let capacity = match color {
        ColorType::Grayscale => bytes.len().checked_mul(4),
        ColorType::GrayscaleAlpha => (bytes.len() / 2).checked_mul(4),
        ColorType::Rgb => (bytes.len() / 3).checked_mul(4),
        ColorType::Rgba => Some(bytes.len()),
        ColorType::Indexed => {
            return Err(bntx_error(0, "PNG palette was not expanded by the decoder"));
        }
    }
    .ok_or_else(|| bntx_error(0, "PNG RGBA conversion size overflows"))?;
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(capacity)
        .map_err(|_| bntx_error(0, "PNG RGBA conversion is too large"))?;
    match color {
        ColorType::Grayscale => {
            for &value in bytes {
                rgba.extend_from_slice(&[value, value, value, 255]);
            }
        }
        ColorType::GrayscaleAlpha => {
            for value in bytes.chunks_exact(2) {
                rgba.extend_from_slice(&[value[0], value[0], value[0], value[1]]);
            }
        }
        ColorType::Rgb => {
            for value in bytes.chunks_exact(3) {
                rgba.extend_from_slice(&[value[0], value[1], value[2], 255]);
            }
        }
        ColorType::Rgba => rgba.extend_from_slice(bytes),
        ColorType::Indexed => unreachable!(),
    }
    Ok(rgba)
}

fn rgba_size(width: u32, height: u32) -> BinResult<usize> {
    usize::try_from(u128::from(width) * u128::from(height) * 4)
        .map_err(|_| bntx_error(0, "RGBA image size exceeds addressable memory"))
}

fn codec_error(operation: &str, error: rusty_dds::Error) -> binrw::Error {
    bntx_error(0, format!("could not {operation}: {error}"))
}
