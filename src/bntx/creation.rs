use super::{
    ChannelMode, ChannelSelector, ImageFormat, Platform, RgbaImage, SurfaceDimension, Texture,
    TextureDimension, TextureInfo, TextureLayout, TileMode, Version, bntx_error,
    derive_block_height_exponent, encode_texture, model::Bntx, validation,
};
use binrw::BinResult;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// One of the BNTX formats supported for newly encoded textures.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NewTextureFormat {
    Bc4Unorm,
    Bc5Unorm,
    Bc7Unorm,
    Bc7Srgb,
}

impl NewTextureFormat {
    pub const fn image_format(self) -> ImageFormat {
        ImageFormat::from_raw(match self {
            Self::Bc4Unorm => 0x1d01,
            Self::Bc5Unorm => 0x1e01,
            Self::Bc7Unorm => 0x2001,
            Self::Bc7Srgb => 0x2006,
        })
    }

    pub const fn channels(self) -> [ChannelSelector; 4] {
        match self {
            Self::Bc4Unorm => [
                ChannelSelector::One,
                ChannelSelector::One,
                ChannelSelector::One,
                ChannelSelector::Red,
            ],
            Self::Bc5Unorm => [
                ChannelSelector::Red,
                ChannelSelector::Red,
                ChannelSelector::Red,
                ChannelSelector::Green,
            ],
            Self::Bc7Unorm | Self::Bc7Srgb => [
                ChannelSelector::Red,
                ChannelSelector::Green,
                ChannelSelector::Blue,
                ChannelSelector::Alpha,
            ],
        }
    }
}

/// Metadata source for an added or explicitly re-profiled texture.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TextureProfile {
    Like(String),
    Format(NewTextureFormat),
}

/// Options controlling texture insertion or replacement.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AddTextureOptions {
    pub profile: Option<TextureProfile>,
    pub replace: bool,
}

#[derive(Clone, Debug)]
pub struct TextureUpsert {
    pub name: String,
    pub image: RgbaImage,
    pub profile: Option<TextureProfile>,
}

#[derive(Clone, Debug, Default)]
pub struct TexturePatch {
    pub upsert: Vec<TextureUpsert>,
    pub remove: Vec<String>,
}

pub fn apply_texture_patch(bntx: &Bntx, patch: &TexturePatch) -> BinResult<Bntx> {
    validation::validate(bntx)?;
    let mut targets = HashSet::new();
    for (index, upsert) in patch.upsert.iter().enumerate() {
        validation::validate_texture_name(&upsert.name).map_err(|error| {
            bntx_error(0, format!("upsert[{index}] {:?}: {error}", upsert.name))
        })?;
        if !targets.insert(upsert.name.as_str()) {
            return Err(bntx_error(
                0,
                format!(
                    "upsert[{index}] {:?}: duplicate BNTX patch target",
                    upsert.name
                ),
            ));
        }
        if let Some(TextureProfile::Like(template)) = &upsert.profile
            && !bntx
                .textures
                .iter()
                .any(|texture| &texture.name == template)
        {
            return Err(bntx_error(
                0,
                format!(
                    "upsert[{index}] {:?}: BNTX template texture {template:?} does not exist",
                    upsert.name
                ),
            ));
        }
    }
    for (index, name) in patch.remove.iter().enumerate() {
        validation::validate_texture_name(name)
            .map_err(|error| bntx_error(0, format!("remove[{index}] {name:?}: {error}")))?;
        if !targets.insert(name) {
            return Err(bntx_error(
                0,
                format!("remove[{index}] {name:?}: duplicate BNTX patch target"),
            ));
        }
        if !bntx.textures.iter().any(|texture| &texture.name == name) {
            return Err(bntx_error(
                0,
                format!("remove[{index}] BNTX texture {name:?} does not exist"),
            ));
        }
    }
    let additions = patch
        .upsert
        .iter()
        .filter(|entry| {
            !bntx
                .textures
                .iter()
                .any(|texture| texture.name == entry.name)
        })
        .count();
    let final_count = bntx
        .textures
        .len()
        .checked_add(additions)
        .and_then(|count| count.checked_sub(patch.remove.len()))
        .ok_or_else(|| bntx_error(0, "BNTX texture count overflow"))?;
    validation::validate_texture_count(final_count)?;

    let mut result = bntx.clone();
    for (index, upsert) in patch.upsert.iter().enumerate() {
        let texture = build_texture(bntx, &upsert.name, &upsert.image, upsert.profile.as_ref())
            .map_err(|error| {
                bntx_error(0, format!("upsert[{index}] {:?}: {error}", upsert.name))
            })?;
        if let Some(index) = result
            .textures
            .iter()
            .position(|item| item.name == upsert.name)
        {
            result.textures[index] = texture;
        } else {
            result.textures.push(texture);
        }
    }
    let removals: HashSet<_> = patch.remove.iter().collect();
    result
        .textures
        .retain(|texture| !removals.contains(&texture.name));
    validation::validate(&result)?;
    Ok(result)
}

/// Adds or replaces a texture using decoded RGBA8 pixels.
pub fn add_texture(
    bntx: &mut Bntx,
    name: impl Into<String>,
    image: &RgbaImage,
    options: AddTextureOptions,
) -> BinResult<()> {
    validation::validate(bntx)?;
    let name = name.into();
    validation::validate_texture_name(&name)?;
    let target_index = bntx
        .textures
        .iter()
        .position(|texture| texture.name == name);
    if target_index.is_some() && !options.replace {
        return Err(bntx_error(
            0,
            format!("BNTX texture {name:?} already exists"),
        ));
    }
    if target_index.is_none() {
        validation::validate_texture_count(bntx.textures.len() + 1)?;
    }

    let texture = build_texture(bntx, &name, image, options.profile.as_ref())?;
    if let Some(index) = target_index {
        bntx.textures[index] = texture;
    } else {
        bntx.textures.push(texture);
    }
    Ok(())
}

fn build_texture(
    bntx: &Bntx,
    name: &str,
    image: &RgbaImage,
    profile: Option<&TextureProfile>,
) -> BinResult<Texture> {
    let target_index = bntx
        .textures
        .iter()
        .position(|texture| texture.name == name);
    let target_user_data = target_index
        .map(|index| bntx.textures[index].user_data.clone())
        .unwrap_or_default();
    let info = match profile {
        Some(TextureProfile::Like(template_name)) => {
            let template = bntx
                .textures
                .iter()
                .find(|texture| texture.name == *template_name)
                .ok_or_else(|| {
                    bntx_error(
                        0,
                        format!("BNTX template texture {template_name:?} does not exist"),
                    )
                })?;
            template.info.clone()
        }
        Some(TextureProfile::Format(format)) => canonical_info(*format),
        None => match target_index {
            Some(index) => bntx.textures[index].info.clone(),
            None => canonical_info(NewTextureFormat::Bc7Unorm),
        },
    };
    let texture = encode_added_texture(name.to_owned(), info, target_user_data, image)?;
    validation::validate_texture(&texture)?;
    Ok(texture)
}

/// Creates a canonical version-4 NX container from ordered decoded images.
pub fn create_bntx(
    name: impl Into<String>,
    images: impl IntoIterator<Item = (String, RgbaImage)>,
    format: NewTextureFormat,
) -> BinResult<Bntx> {
    let mut textures = Vec::new();
    for (name, image) in images {
        textures.push(encode_added_texture(
            name,
            canonical_info(format),
            Vec::new(),
            &image,
        )?);
    }
    let bntx = Bntx {
        version: Version {
            major: 4,
            minor: 0,
            micro: 0,
        },
        alignment_exponent: 12,
        target_address_size: 64,
        flag: 0,
        name: name.into(),
        platform: Platform::Nx,
        textures,
        source_string_order: Vec::new(),
    };
    validation::validate(&bntx)?;
    Ok(bntx)
}

fn encode_added_texture(
    name: String,
    mut info: TextureInfo,
    user_data: Vec<super::UserData>,
    image: &RgbaImage,
) -> BinResult<Texture> {
    info.width = image.width;
    info.height = image.height;
    let block_height = derive_block_height_exponent(info.format, image.width, image.height)?;
    if !info.texture_layout.set_block_height_exponent(block_height) {
        return Err(bntx_error(0, "BNTX block height exceeds texture layout"));
    }
    let mut texture = Texture {
        name,
        info,
        mip_offsets: vec![0],
        user_data,
        image_data: Vec::new(),
    };
    let encoded = encode_texture(&texture, image, ChannelMode::Resolved)?;
    texture.image_data = encoded.image_data;
    texture.mip_offsets = encoded.mip_offsets;
    Ok(texture)
}

fn canonical_info(format: NewTextureFormat) -> TextureInfo {
    TextureInfo {
        flags: 1,
        image_dimension: TextureDimension::TwoD,
        tile_mode: TileMode::BlockLinear,
        swizzle: 0,
        sample_count: 1,
        format: format.image_format(),
        gpu_access: 0x20,
        width: 1,
        height: 1,
        depth: 1,
        array_count: 1,
        texture_layout: TextureLayout::from_raw(0),
        texture_layout2: 0x0001_0007,
        reserved: [0; 20],
        image_alignment: 0x200,
        channels: format.channels(),
        surface_dimension: SurfaceDimension::TwoD,
    }
}
