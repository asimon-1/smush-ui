use std::io::Cursor;

use bilge::prelude::*;
use binrw::{BinRead, BinWrite, BinWriterExt};
use smush_ui::bflyt::sections::material::{
    AlphaCompare, BlendMode, FontShadowParameter, IndirectTextureTransform, MaterialFlags,
    ProjectionTextureParameter, TevStage, TextureCoordinateGenerator, TextureFilterFlags,
    TextureReference, TextureTransform, TextureWrapFlags, TextureWrapMode,
};
use smush_ui::bflyt::sections::{Color, Vec2};

fn encoded_len<T: BinWrite>(value: &T) -> usize
where
    for<'a> T::Args<'a>: Default,
{
    let mut output = Cursor::new(Vec::new());
    output
        .write_le(value)
        .expect("fixed component should write");
    output.into_inner().len()
}

fn color() -> Color {
    Color {
        red: 1,
        green: 2,
        blue: 3,
        alpha: 4,
    }
}

fn vec2() -> Vec2 {
    Vec2 { x: 1.5, y: -2.0 }
}

#[test]
fn material_flags_decode_counts_and_preserve_reserved_bits() {
    let raw = 3
        | (1 << 2)
        | (2 << 4)
        | (5 << 6)
        | (1 << 9)
        | (1 << 10)
        | (1 << 11)
        | (1 << 12)
        | (1 << 13)
        | (1 << 14)
        | (2 << 15)
        | (1 << 17)
        | (1 << 18)
        | (0x1a5 << 19);
    let flags = MaterialFlags::from(u32::new(raw));

    assert_eq!(flags.texture_reference_count().value(), 3);
    assert_eq!(flags.texture_transform_count().value(), 1);
    assert_eq!(flags.texture_coordinate_generator_count().value(), 2);
    assert_eq!(flags.tev_stage_count().value(), 5);
    assert!(flags.has_alpha_compare());
    assert!(flags.has_blend_mode());
    assert!(flags.texture_only());
    assert!(flags.has_separate_blend_logic_mode());
    assert!(flags.unk1());
    assert!(flags.has_indirect_texture_transform());
    assert_eq!(flags.projection_texture_parameter_count().value(), 2);
    assert!(flags.has_font_shadow_parameter());
    assert!(flags.alpha_interpolation());
    assert_eq!(u32::from(flags).value(), raw);
}

#[test]
fn texture_reference_round_trips_packed_flags() {
    let bytes = [0xfe, 0xff, 0b1010_1001, 0b0101_0110];
    let mut input = Cursor::new(bytes);
    let reference = TextureReference::read_le(&mut input).expect("texture ref should parse");

    assert_eq!(reference.texture_index, -2);
    assert_eq!(reference.wrap_flags.u(), TextureWrapMode::Repeat);
    assert_eq!(reference.wrap_flags.v(), TextureWrapMode::Mirror);
    assert_eq!(reference.wrap_flags.reserved_i().value(), 0b1010);
    assert_eq!(reference.filter_flags.reserved_i().value(), 0b0101);

    let mut output = Cursor::new(Vec::new());
    output
        .write_le(&reference)
        .expect("texture ref should write");
    assert_eq!(output.into_inner(), bytes);
}

#[test]
fn version_8_component_models_have_fixed_serialized_sizes() {
    assert_eq!(
        encoded_len(&TextureReference {
            texture_index: -1,
            wrap_flags: TextureWrapFlags::default(),
            filter_flags: TextureFilterFlags::default(),
        }),
        4
    );
    assert_eq!(
        encoded_len(&TextureTransform {
            translation: vec2(),
            rotation: 0.25,
            scale: vec2(),
        }),
        20
    );
    assert_eq!(
        encoded_len(&TextureCoordinateGenerator {
            matrix: 1,
            source: 2,
            unknown: [3; 14],
        }),
        16
    );
    assert_eq!(
        encoded_len(&TevStage {
            color_mode: 1,
            alpha_mode: 2,
            reserved: 3,
        }),
        4
    );
    assert_eq!(
        encoded_len(&AlphaCompare {
            compare_mode: 1,
            reserved: [2; 3],
            reference: 0.5,
        }),
        8
    );
    assert_eq!(
        encoded_len(&BlendMode {
            operation: 1,
            source_factor: 2,
            destination_factor: 3,
            logic_operation: 4,
        }),
        4
    );
    assert_eq!(
        encoded_len(&IndirectTextureTransform {
            rotation: 0.0,
            scale: vec2(),
        }),
        12
    );
    assert_eq!(
        encoded_len(&ProjectionTextureParameter {
            translation: vec2(),
            scale: vec2(),
            mode: 1,
            reserved: [2; 3],
        }),
        20
    );
    assert_eq!(
        encoded_len(&FontShadowParameter {
            black_color: color(),
            white_color: color(),
        }),
        8
    );
}
