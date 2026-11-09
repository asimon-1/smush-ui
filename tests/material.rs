use std::{fs, io::Cursor};

use bilge::prelude::*;
use binrw::{BinRead, BinWrite, BinWriterExt};
use smush_ui::bflyt::sections::material::{
    AlphaCompare, BlendMode, FontShadowParameter, IndirectTextureTransform, MaterialFlags,
    ProjectionTextureParameter, TevStage, TextureCoordinateGenerator, TextureFilterFlags,
    TextureReference, TextureTransform, TextureWrapFlags, TextureWrapMode, decode_material_payload,
    encode_material_payload,
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

fn material_record(name: &str, flags: u32, variable_data: Vec<u8>) -> Vec<u8> {
    let mut record = vec![0; 44];
    record[..name.len()].copy_from_slice(name.as_bytes());
    record[28..32].copy_from_slice(&flags.to_le_bytes());
    record[32..36].copy_from_slice(&0x1234_5678u32.to_le_bytes());
    record.extend(variable_data);
    record
}

fn material_section(records: Vec<Vec<u8>>) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&(records.len() as u16).to_le_bytes());
    payload.extend_from_slice(&0x9876u16.to_le_bytes());
    payload.resize(4 + records.len() * 4, 0);

    for (index, record) in records.iter().enumerate() {
        let offset = (payload.len() + 8) as u32;
        payload[4 + index * 4..8 + index * 4].copy_from_slice(&offset.to_le_bytes());
        payload.extend(record);
    }
    payload
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
fn component_models_have_fixed_serialized_sizes() {
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

#[test]
fn projection_parameters_preserve_non_finite_float_bits_in_json() {
    let parameter = ProjectionTextureParameter {
        translation: Vec2 {
            x: 0.5,
            y: f32::from_bits(0xffff_ffff),
        },
        scale: Vec2 {
            x: f32::INFINITY,
            y: -1.0,
        },
        mode: 1,
        reserved: [2; 3],
    };

    let json = serde_json::to_value(&parameter).expect("parameter should serialize");
    assert_eq!(json["translation"]["x"], 0.5);
    assert_eq!(json["translation"]["y"]["bits"], 0xffff_ffffu32);
    let restored: ProjectionTextureParameter =
        serde_json::from_value(json).expect("parameter should deserialize");
    assert_eq!(restored.translation.y.to_bits(), 0xffff_ffff);
    assert_eq!(restored.scale.x.to_bits(), f32::INFINITY.to_bits());
}

#[test]
fn decodes_empty_minimal_and_multiple_materials() {
    let empty =
        decode_material_payload(&[0, 0, 0, 0]).expect("empty material section should decode");
    assert!(empty.materials.is_empty());

    let payload = material_section(vec![
        material_record("first", 0, Vec::new()),
        material_record("second", 0, Vec::new()),
    ]);
    let decoded =
        decode_material_payload(&payload).expect("minimal material records should decode");
    assert_eq!(decoded.materials.len(), 2);
    assert_eq!(decoded.materials[0].name, "first");
    assert_eq!(decoded.materials[1].name, "second");
    assert_eq!(decoded.materials[0].unknown_header_value, 0x1234_5678);
}

#[test]
fn decodes_all_flag_controlled_components() {
    let flags = 2
        | (1 << 2)
        | (1 << 4)
        | (2 << 6)
        | (1 << 9)
        | (1 << 10)
        | (1 << 11)
        | (1 << 12)
        | (1 << 13)
        | (1 << 14)
        | (2 << 15)
        | (1 << 17)
        | (1 << 18)
        | (1 << 19);
    let variable_data = vec![0; 8 + 20 + 16 + 8 + 8 + 4 + 4 + 12 + 40 + 8 + 44];
    let payload = material_section(vec![material_record("complete", flags, variable_data)]);
    let material = decode_material_payload(&payload)
        .expect("all material components should decode")
        .materials
        .pop()
        .unwrap();

    assert_eq!(material.texture_references.len(), 2);
    assert_eq!(material.texture_transforms.len(), 1);
    assert_eq!(material.texture_coordinate_generators.len(), 1);
    assert_eq!(material.tev_stages.len(), 2);
    assert!(material.alpha_compare.is_some());
    assert!(material.blend_mode.is_some());
    assert!(material.separate_blend_logic_mode.is_some());
    assert!(material.indirect_texture_transform.is_some());
    assert_eq!(material.projection_texture_parameters.len(), 2);
    assert!(material.font_shadow_parameter.is_some());
    assert!(material.texture_only);
    assert!(material.alpha_interpolation);
    assert!(material.unk2);
    assert_eq!(material.reserved_flags.value(), 0);
    assert!(material.unk1.is_some());
}

#[test]
fn rejects_malformed_material_tables_and_records() {
    assert!(decode_material_payload(&[1, 0, 0, 0]).is_err());

    let mut offset_into_table = vec![1, 0, 0, 0, 8, 0, 0, 0];
    offset_into_table.extend(material_record("bad", 0, Vec::new()));
    assert!(decode_material_payload(&offset_into_table).is_err());

    let mut descending_offsets = vec![2, 0, 0, 0];
    descending_offsets.extend_from_slice(&28u32.to_le_bytes());
    descending_offsets.extend_from_slice(&24u32.to_le_bytes());
    descending_offsets.resize(20, 0);
    assert!(decode_material_payload(&descending_offsets).is_err());

    let out_of_range = vec![1, 0, 0, 0, 100, 0, 0, 0];
    assert!(decode_material_payload(&out_of_range).is_err());

    let truncated_optional = material_section(vec![material_record("short", 1 << 9, Vec::new())]);
    assert!(decode_material_payload(&truncated_optional).is_err());

    let trailing_data = material_section(vec![material_record("extra", 0, vec![0])]);
    assert!(decode_material_payload(&trailing_data).is_err());
}

#[test]
fn encodes_material_payloads_with_reconstructed_flags() {
    let flags = 2
        | (1 << 2)
        | (1 << 4)
        | (2 << 6)
        | (1 << 9)
        | (1 << 10)
        | (1 << 11)
        | (1 << 12)
        | (1 << 13)
        | (1 << 14)
        | (2 << 15)
        | (1 << 17)
        | (1 << 18)
        | (1 << 19);
    let payload = material_section(vec![material_record(
        "complete",
        flags,
        vec![0; 8 + 20 + 16 + 8 + 8 + 4 + 4 + 12 + 40 + 8 + 44],
    )]);
    let decoded = decode_material_payload(&payload).expect("material should decode");
    assert_eq!(encode_material_payload(&decoded).unwrap(), payload);
}

#[test]
fn rejects_material_component_counts_that_do_not_fit_in_flags() {
    let payload = material_section(vec![material_record("limited", 0, Vec::new())]);
    let mut decoded = decode_material_payload(&payload).expect("material should decode");
    for _ in 0..4 {
        decoded.materials[0]
            .texture_references
            .push(TextureReference {
                texture_index: 0,
                wrap_flags: TextureWrapFlags::default(),
                filter_flags: TextureFilterFlags::default(),
            });
    }

    assert!(encode_material_payload(&decoded).is_err());
}

#[test]
fn checked_in_material_payloads_round_trip_byte_for_byte() {
    let fixture_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/test_assets");
    for entry in fs::read_dir(fixture_dir).expect("fixture directory should be readable") {
        let path = entry.expect("fixture entry should be readable").path();
        if path
            .extension()
            .is_none_or(|extension| extension != "bflyt")
        {
            continue;
        }

        let bytes = fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let section_count = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
        let mut offset = 20;
        for _ in 0..section_count {
            let size =
                u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
            if &bytes[offset..offset + 4] == b"mat1" {
                let payload = &bytes[offset + 8..offset + size];
                let decoded = decode_material_payload(payload)
                    .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
                assert_eq!(
                    encode_material_payload(&decoded).unwrap(),
                    payload,
                    "{} material payload should round trip",
                    path.display()
                );
            }
            offset += size;
        }
    }
}
