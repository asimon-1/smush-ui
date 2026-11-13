use super::{
    bntx_error,
    format::{Platform, SurfaceDimension, TextureDimension, TileMode},
    model::{Bntx, Texture, UserDataValue},
};
use binrw::BinResult;
use std::collections::HashSet;

const SUPPORTED_ALIGNMENT_EXPONENT: u8 = 12;
const SUPPORTED_ADDRESS_SIZE: u8 = 64;

pub(crate) fn validate(bntx: &Bntx) -> BinResult<()> {
    if (bntx.version.major, bntx.version.minor, bntx.version.micro) != (4, 0, 0) {
        return Err(bntx_error(0, "writer supports only BNTX version 4.0.0"));
    }
    if bntx.platform != Platform::Nx
        || bntx.alignment_exponent != SUPPORTED_ALIGNMENT_EXPONENT
        || bntx.target_address_size != SUPPORTED_ADDRESS_SIZE
        || bntx.flag != 0
    {
        return Err(bntx_error(0, "unsupported BNTX writer profile"));
    }
    validate_name(&bntx.name, "container name", true)?;
    validate_texture_count(bntx.textures.len())?;
    let mut texture_names = HashSet::with_capacity(bntx.textures.len());
    for texture in &bntx.textures {
        validate_texture_name(&texture.name)?;
        if !texture_names.insert(texture.name.as_str()) {
            return Err(bntx_error(
                0,
                format!("duplicate BNTX texture name {:?}", texture.name),
            ));
        }
        validate_texture(texture)?;
    }
    Ok(())
}

pub(crate) fn validate_texture_name(name: &str) -> BinResult<()> {
    validate_name(name, "texture name", false)
}

pub(crate) fn validate_texture_count(count: usize) -> BinResult<()> {
    if count > usize::from(u16::MAX) {
        return Err(bntx_error(0, "BNTX texture count exceeds 65535"));
    }
    Ok(())
}

pub(crate) fn validate_texture(texture: &Texture) -> BinResult<()> {
    let info = &texture.info;
    if info.image_dimension != TextureDimension::TwoD
        || info.tile_mode != TileMode::BlockLinear
        || info.swizzle != 0
        || info.sample_count != 1
        || info.surface_dimension != SurfaceDimension::TwoD
        || info.depth != 1
        || info.array_count != 1
    {
        return Err(bntx_error(
            0,
            format!("unsupported BNTX texture profile for {:?}", texture.name),
        ));
    }
    if info.width == 0 || info.height == 0 || info.texture_layout.block_height_exponent() > 5 {
        return Err(bntx_error(
            0,
            format!(
                "invalid BNTX texture dimensions or layout for {:?}",
                texture.name
            ),
        ));
    }
    if info.image_alignment == 0 || !info.image_alignment.is_power_of_two() {
        return Err(bntx_error(
            0,
            format!("invalid BNTX image alignment for {:?}", texture.name),
        ));
    }
    if texture.image_data.is_empty() || texture.image_data.len() > u32::MAX as usize {
        return Err(bntx_error(
            0,
            format!("invalid BNTX image payload for {:?}", texture.name),
        ));
    }
    if texture.mip_offsets.is_empty()
        || texture.mip_offsets.len() > usize::from(u16::MAX)
        || texture.mip_offsets[0] != 0
        || texture
            .mip_offsets
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || texture
            .mip_offsets
            .iter()
            .any(|&offset| offset >= texture.image_data.len() as u64)
    {
        return Err(bntx_error(
            0,
            format!("invalid BNTX mip offsets for {:?}", texture.name),
        ));
    }
    if texture.user_data.len() > u32::MAX as usize {
        return Err(bntx_error(0, "BNTX user-data count exceeds u32"));
    }
    let mut names = HashSet::with_capacity(texture.user_data.len());
    for user in &texture.user_data {
        validate_name(&user.name, "user-data name", false)?;
        if !names.insert(user.name.as_str()) {
            return Err(bntx_error(
                0,
                format!("duplicate BNTX user-data name {:?}", user.name),
            ));
        }
        validate_user_value(&user.value)?;
    }
    Ok(())
}

fn validate_user_value(value: &UserDataValue) -> BinResult<()> {
    let count = match value {
        UserDataValue::Int32(values) => values.len(),
        UserDataValue::Single(values) => values.len(),
        UserDataValue::String(values) => {
            for value in values {
                validate_name(value, "string user-data value", true)?;
            }
            values.len()
        }
        UserDataValue::Byte(values) => values.len(),
        UserDataValue::WString(values) => {
            for value in values {
                if value.encode_utf16().count() > usize::from(u16::MAX) {
                    return Err(bntx_error(0, "BNTX wide string exceeds 65535 code units"));
                }
            }
            values.len()
        }
    };
    if count > u32::MAX as usize {
        return Err(bntx_error(0, "BNTX user-data value count exceeds u32"));
    }
    Ok(())
}

fn validate_name(value: &str, label: &str, allow_empty: bool) -> BinResult<()> {
    if (!allow_empty && value.is_empty()) || value.as_bytes().contains(&0) {
        return Err(bntx_error(0, format!("invalid BNTX {label}")));
    }
    if value.len() > usize::from(u16::MAX) {
        return Err(bntx_error(0, format!("BNTX {label} exceeds 65535 bytes")));
    }
    Ok(())
}
