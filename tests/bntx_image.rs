use serde_jsonc2::jsonc;
use smush_ui::bntx::{
    Bntx, ChannelMode, ChannelSelector, ImageFormat, RgbaImage, SurfaceDimension, SurfaceLayout,
    Texture, TextureDimension, TextureInfo, TextureLayout, TileMode, decode_texture,
    derive_block_height_exponent, encode_new_texture, encode_texture, read_png, write_png,
};

const BC4_UNORM: u32 = 0x1d01;
const BC5_UNORM: u32 = 0x1e01;
const BC7_UNORM: u32 = 0x2001;
const BC7_SRGB: u32 = 0x2006;

fn channels_for(format: u32) -> [ChannelSelector; 4] {
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

fn texture_model(format: u32, width: u32, height: u32, mip_count: usize) -> Texture {
    let format = ImageFormat::from_raw(format);
    let block_height = derive_block_height_exponent(format, width, height).unwrap();
    Texture {
        name: "synthetic".into(),
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
            image_alignment: 0x200,
            channels: channels_for(format.to_raw()),
            surface_dimension: SurfaceDimension::TwoD,
        },
        mip_offsets: vec![0; mip_count],
        user_data: Vec::new(),
        image_data: Vec::new(),
    }
}

fn encoded_texture(format: u32, width: u32, height: u32, pattern: u8) -> (Texture, RgbaImage) {
    let source = patterned_image(format, width, height, pattern);
    let encoded = encode_new_texture(
        ImageFormat::from_raw(format),
        channels_for(format),
        &source,
        ChannelMode::Resolved,
    )
    .unwrap();
    let mut texture = texture_model(format, width, height, 1);
    assert!(
        texture
            .info
            .texture_layout
            .set_block_height_exponent(encoded.block_height_exponent)
    );
    texture.mip_offsets = encoded.mip_offsets;
    texture.image_data = encoded.image_data;
    (texture, source)
}

fn container_with(texture: Texture) -> Bntx {
    let mut container: Bntx = serde_jsonc2::from_value(jsonc!({
        "version": { "major": 4, "minor": 0, "micro": 0 },
        "alignment_exponent": 12,
        "target_address_size": 64,
        "flag": 0,
        "name": "synthetic_images",
        "platform": "Nx",
        "textures": []
    }))
    .unwrap();
    container.textures.push(texture);
    container
}

#[test]
fn block_linear_surfaces_round_trip_across_observed_geometries() {
    for (format, width, height, exponent) in [
        (BC4_UNORM, 1, 1, 0),
        (BC4_UNORM, 72, 64, 1),
        (BC5_UNORM, 7, 5, 0),
        (BC7_UNORM, 36, 32, 0),
        (BC7_SRGB, 256, 256, 3),
        (BC7_UNORM, 1024, 1024, 4),
    ] {
        let layout = SurfaceLayout::new(ImageFormat::from_raw(format), width, height, exponent)
            .expect("surface layout should be supported");
        let linear = (0..layout.linear_size().unwrap())
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        let swizzled = layout.swizzle(&linear).expect("surface should swizzle");
        assert_eq!(swizzled.len(), layout.swizzled_size().unwrap());
        assert_eq!(
            layout.deswizzle(&swizzled).unwrap(),
            linear,
            "{format:#x} {width}x{height} exponent {exponent}"
        );
    }
}

#[test]
fn block_linear_surface_matches_known_tegra_gob_offsets() {
    let layout = SurfaceLayout::new(ImageFormat::from_raw(BC7_UNORM), 20, 36, 0).unwrap();
    let mut linear = Vec::with_capacity(layout.linear_size().unwrap());
    for block in 1..=45 {
        linear.extend_from_slice(&[block; 16]);
    }

    let swizzled = layout.swizzle(&linear).unwrap();
    for (offset, block) in [
        (0, 1),
        (32, 2),
        (256, 3),
        (288, 4),
        (512, 5),
        (16, 6),
        (48, 7),
        (272, 8),
        (528, 10),
        (64, 11),
        (128, 21),
        (1024, 41),
        (1536, 45),
    ] {
        assert_eq!(&swizzled[offset..offset + 16], &[block; 16]);
    }
}

#[test]
fn derives_new_and_mip_block_heights_at_gob_boundaries() {
    let format = ImageFormat::from_raw(BC7_UNORM);
    for (height, expected) in [
        (1, 0),
        (32, 0),
        (33, 1),
        (64, 1),
        (65, 2),
        (129, 3),
        (257, 4),
    ] {
        assert_eq!(
            derive_block_height_exponent(format, 7, height).unwrap(),
            expected,
            "height {height}"
        );
    }

    let base = SurfaceLayout::new(format, 1024, 1024, 4).unwrap();
    assert_eq!(base.mip(0).block_height_exponent(), 4);
    assert_eq!(base.mip(1).block_height_exponent(), 4);
    assert_eq!(base.mip(2).block_height_exponent(), 3);
    assert_eq!(base.mip(3).block_height_exponent(), 2);
    assert_eq!(base.mip(4).block_height_exponent(), 1);
    assert_eq!(base.mip(5).block_height_exponent(), 0);
}

#[test]
fn decodes_all_supported_formats_and_applies_channel_selectors() {
    for (format, expected_dimensions) in [
        (BC7_UNORM, (13, 11)),
        (BC7_SRGB, (15, 9)),
        (BC4_UNORM, (9, 7)),
        (BC5_UNORM, (7, 5)),
    ] {
        let pattern = if matches!(format, BC7_UNORM | BC7_SRGB) {
            1
        } else {
            2
        };
        let (texture, source) = encoded_texture(
            format,
            expected_dimensions.0,
            expected_dimensions.1,
            pattern,
        );
        let raw = decode_texture(&texture, 0, ChannelMode::Raw)
            .unwrap_or_else(|error| panic!("decode {format:#x}: {error}"));
        let resolved = decode_texture(&texture, 0, ChannelMode::Resolved)
            .unwrap_or_else(|error| panic!("resolve {format:#x}: {error}"));
        assert_eq!((raw.width, raw.height), expected_dimensions);
        assert_eq!(
            raw.pixels.len(),
            raw.width as usize * raw.height as usize * 4
        );

        for (raw, resolved) in raw
            .pixels
            .chunks_exact(4)
            .zip(resolved.pixels.chunks_exact(4))
        {
            match format {
                BC4_UNORM => assert_eq!(resolved, &[255, 255, 255, raw[0]]),
                BC5_UNORM => assert_eq!(resolved, &[raw[0], raw[0], raw[0], raw[1]]),
                BC7_UNORM | BC7_SRGB => assert_eq!(resolved, raw),
                _ => unreachable!(),
            }
        }
        assert_lossy_match(format, &source, &resolved);

        let png = write_png(&resolved).expect("decoded image should write as PNG");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(read_png(&png).unwrap(), resolved);
    }
}

#[test]
fn encodes_supported_formats_deterministically_and_round_trips_through_bntx() {
    for (format, width, height) in [
        (BC4_UNORM, 5, 7),
        (BC5_UNORM, 7, 5),
        (BC7_UNORM, 5, 7),
        (BC7_SRGB, 7, 5),
    ] {
        let channels = channels_for(format);
        for pattern in 0..3 {
            let source = patterned_image(format, width, height, pattern);
            let channel_mode = if format == BC7_UNORM && pattern == 2 {
                ChannelMode::Raw
            } else {
                ChannelMode::Resolved
            };
            let encoded = encode_new_texture(
                ImageFormat::from_raw(format),
                channels,
                &source,
                channel_mode,
            )
            .unwrap_or_else(|error| panic!("encode {format:#x}: {error}"));
            let repeated = encode_new_texture(
                ImageFormat::from_raw(format),
                channels,
                &source,
                channel_mode,
            )
            .unwrap();
            assert_eq!(encoded, repeated, "encoding must be deterministic");

            let layout =
                SurfaceLayout::for_new_texture(ImageFormat::from_raw(format), width, height)
                    .unwrap();
            assert_eq!(encoded.image_data.len(), layout.swizzled_size().unwrap());
            assert_eq!(encoded.mip_offsets, [0]);

            let mut texture = texture_model(format, width, height, 1);
            texture.name = format!("encoded_{format:x}_{pattern}");
            assert!(
                texture
                    .info
                    .texture_layout
                    .set_block_height_exponent(encoded.block_height_exponent)
            );
            texture.mip_offsets = encoded.mip_offsets;
            texture.image_data = encoded.image_data;
            let container = container_with(texture);

            let bytes = container
                .to_bytes()
                .expect("encoded texture should serialize");
            let reparsed = Bntx::from_bytes(&bytes).expect("encoded texture should reparse");
            let decoded = decode_texture(&reparsed.textures[0], 0, channel_mode)
                .expect("encoded texture should decode");
            assert_lossy_match(format, &source, &decoded);
        }
    }
}

#[test]
fn encodes_and_decodes_padded_non_power_of_two_mip_chains() {
    let mut texture = texture_model(BC7_UNORM, 17, 9, 4);
    assert!(texture.info.texture_layout.set_block_height_exponent(0));
    let source = patterned_image(BC7_UNORM, 17, 9, 2);
    let encoded = encode_texture(&texture, &source, ChannelMode::Resolved).unwrap();
    assert_eq!(encoded.mip_offsets, [0, 1024, 1536, 2048]);
    assert_eq!(encoded.image_data.len(), 2560);
    texture.image_data = encoded.image_data;
    texture.mip_offsets = encoded.mip_offsets;
    for (level, dimensions) in [(0, (17, 9)), (1, (8, 4)), (2, (4, 2)), (3, (2, 1))] {
        let decoded = decode_texture(&texture, level, ChannelMode::Resolved).unwrap();
        assert_eq!((decoded.width, decoded.height), dimensions);
    }

    texture.info.image_alignment = 0x1000;
    assert!(encode_texture(&texture, &source, ChannelMode::Resolved).is_err());
}

#[test]
fn rejects_invalid_png_surface_and_texture_inputs() {
    assert!(read_png(b"not a png").is_err());
    assert!(
        SurfaceLayout::new(ImageFormat::from_raw(BC7_UNORM), u32::MAX, u32::MAX, 0)
            .unwrap()
            .linear_size()
            .is_err()
    );

    let (mut texture, _) = encoded_texture(BC7_UNORM, 8, 8, 2);
    texture.image_data.pop();
    assert!(decode_texture(&texture, 0, ChannelMode::Raw).is_err());

    let (texture, _) = encoded_texture(BC7_UNORM, 8, 8, 2);
    let wrong_size = RgbaImage::new(1, 1, vec![0; 4]).unwrap();
    assert!(encode_texture(&texture, &wrong_size, ChannelMode::Raw).is_err());

    let source = RgbaImage::new(4, 4, vec![0; 64]).unwrap();
    assert!(
        encode_new_texture(
            ImageFormat::from_raw(0xdead_beef),
            [ChannelSelector::Red; 4],
            &source,
            ChannelMode::Raw,
        )
        .is_err()
    );
    assert!(RgbaImage::new(2, 2, vec![0; 15]).is_err());
}

fn patterned_image(format: u32, width: u32, height: u32, pattern: u8) -> RgbaImage {
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            let red = match pattern {
                0 => 32,
                1 => 220,
                _ => ((x * 31 + y * 17) % 256) as u8,
            };
            let green = match pattern {
                0 => 96,
                1 => 180,
                _ => ((x * 11 + y * 47) % 256) as u8,
            };
            let blue = match pattern {
                0 => 160,
                1 => 80,
                _ => ((x * 53 + y * 7) % 256) as u8,
            };
            let alpha = if pattern == 0 {
                255
            } else if pattern == 1 {
                96
            } else {
                ((x * 19 + y * 23) % 256) as u8
            };
            match format {
                BC4_UNORM => pixels.extend_from_slice(&[255, 255, 255, red]),
                BC5_UNORM => pixels.extend_from_slice(&[red, red, red, green]),
                BC7_UNORM | BC7_SRGB => pixels.extend_from_slice(&[red, green, blue, alpha]),
                _ => unreachable!(),
            }
        }
    }
    RgbaImage::new(width, height, pixels).unwrap()
}

fn assert_lossy_match(format: u32, expected: &RgbaImage, actual: &RgbaImage) {
    assert_eq!(
        (actual.width, actual.height),
        (expected.width, expected.height)
    );
    let differences = expected
        .pixels
        .iter()
        .zip(&actual.pixels)
        .map(|(&expected, &actual)| expected.abs_diff(actual))
        .collect::<Vec<_>>();
    let maximum = differences.iter().copied().max().unwrap_or(0);
    let mean = differences
        .iter()
        .map(|&value| u64::from(value))
        .sum::<u64>() as f64
        / differences.len() as f64;
    assert!(
        maximum <= 96 && mean <= 20.0,
        "{format:#x} error too large: max {maximum}, mean {mean:.2}"
    );
}
