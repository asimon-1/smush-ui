use super::{
    bntx_error,
    format::{
        ChannelFormat, ChannelSelector, ImageFormat, SurfaceDimension, TextureDimension, TileMode,
        TypeFormat,
    },
    model::{Texture, TextureInfo},
    swizzle::SurfaceLayout,
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
    let mut output = vec![0; output_size];
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
        // container padding and must remain zero.
        let (blocks, padding) = encoded.data.split_at(linear_size);
        if padding.iter().any(|&byte| byte != 0) {
            return Err(bntx_error(
                0,
                format!("rusty_dds returned nonzero padding after mip {mip}"),
            ));
        }
        mip_offsets.push(image_data.len() as u64);
        image_data.extend_from_slice(&surface.swizzle(blocks)?);
    }
    Ok(EncodedImage {
        image_data,
        mip_offsets,
        block_height_exponent,
    })
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
    let mut rgba = Vec::with_capacity(match color {
        ColorType::Grayscale => bytes.len() * 4,
        ColorType::GrayscaleAlpha => bytes.len() / 2 * 4,
        ColorType::Rgb => bytes.len() / 3 * 4,
        ColorType::Rgba => bytes.len(),
        ColorType::Indexed => {
            return Err(bntx_error(0, "PNG palette was not expanded by the decoder"));
        }
    });
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
