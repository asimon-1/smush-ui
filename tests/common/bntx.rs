use serde_json::json;
use smush_ui::bntx::{
    Bntx, ChannelMode, ChannelSelector, ImageFormat, RgbaImage, SurfaceDimension, Texture,
    TextureDimension, TextureInfo, TextureLayout, TileMode, UserData, UserDataValue,
    derive_block_height_exponent, encode_new_texture,
};

pub const BC4_UNORM: u32 = 0x1d01;
pub const BC5_UNORM: u32 = 0x1e01;
pub const BC7_UNORM: u32 = 0x2001;
pub const BC7_SRGB: u32 = 0x2006;

pub fn synthetic_bntx() -> Bntx {
    let mut bntx: Bntx = serde_json::from_value(json!({
        "version": { "major": 4, "minor": 0, "micro": 0 },
        "alignment_exponent": 12,
        "target_address_size": 64,
        "flag": 0,
        "name": "synthetic_bundle",
        "platform": "Nx",
        "textures": []
    }))
    .unwrap();
    for (index, (format, width, height, name)) in [
        (BC4_UNORM, 9, 7, "same/name"),
        (BC5_UNORM, 7, 5, "same^name"),
        (BC7_UNORM, 5, 7, "color:*?"),
        (BC7_SRGB, 7, 5, "sRGB color"),
    ]
    .into_iter()
    .enumerate()
    {
        bntx.textures
            .push(encoded_texture(format, width, height, name, index as u8));
    }
    bntx.textures[0].user_data.push(UserData {
        name: "purpose".into(),
        value: UserDataValue::String(vec!["public synthetic fixture".into()]),
    });
    let bytes = bntx.to_bytes().unwrap();
    Bntx::from_bytes(&bytes).unwrap()
}

pub fn channels_for(format: u32) -> [ChannelSelector; 4] {
    match format {
        BC4_UNORM => [
            ChannelSelector::One,
            ChannelSelector::One,
            ChannelSelector::One,
            ChannelSelector::Red,
        ],
        BC5_UNORM => [
            ChannelSelector::Red,
            ChannelSelector::Red,
            ChannelSelector::Red,
            ChannelSelector::Green,
        ],
        BC7_UNORM | BC7_SRGB => [
            ChannelSelector::Red,
            ChannelSelector::Green,
            ChannelSelector::Blue,
            ChannelSelector::Alpha,
        ],
        _ => unreachable!(),
    }
}

fn encoded_texture(format: u32, width: u32, height: u32, name: &str, seed: u8) -> Texture {
    let format = ImageFormat::from_raw(format);
    let channels = channels_for(format.to_raw());
    let image = patterned_image(format.to_raw(), width, height, seed);
    let encoded = encode_new_texture(format, channels, &image, ChannelMode::Resolved).unwrap();
    let block_height = derive_block_height_exponent(format, width, height).unwrap();
    let mut texture = Texture {
        name: name.into(),
        info: TextureInfo {
            flags: 0,
            image_dimension: TextureDimension::TwoD,
            tile_mode: TileMode::BlockLinear,
            swizzle: 0,
            sample_count: 1,
            format,
            gpu_access: 0x20,
            width,
            height,
            depth: 1,
            array_count: 1,
            texture_layout: TextureLayout::from_raw(u32::from(block_height)),
            texture_layout2: 0,
            reserved: [seed; 20],
            image_alignment: 0x200,
            channels,
            surface_dimension: SurfaceDimension::TwoD,
        },
        mip_offsets: encoded.mip_offsets,
        user_data: Vec::new(),
        image_data: encoded.image_data,
    };
    assert!(
        texture
            .info
            .texture_layout
            .set_block_height_exponent(encoded.block_height_exponent)
    );
    texture
}

fn patterned_image(format: u32, width: u32, height: u32, seed: u8) -> RgbaImage {
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            let red = ((x * 17 + y * 29 + u32::from(seed) * 11) % 256) as u8;
            let green = ((x * 31 + y * 7 + u32::from(seed) * 23) % 256) as u8;
            let blue = ((x * 5 + y * 43 + u32::from(seed) * 37) % 256) as u8;
            let alpha = ((x * 13 + y * 19 + u32::from(seed) * 41) % 256) as u8;
            match format {
                BC4_UNORM => pixels.extend_from_slice(&[255, 255, 255, alpha]),
                BC5_UNORM => pixels.extend_from_slice(&[red, red, red, green]),
                BC7_UNORM | BC7_SRGB => pixels.extend_from_slice(&[red, green, blue, alpha]),
                _ => unreachable!(),
            }
        }
    }
    RgbaImage::new(width, height, pixels).unwrap()
}
