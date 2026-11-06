use bilge::prelude::*;
use binrw::{BinRead, BinResult, Endian, Error as BinError, binrw};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Error, ErrorKind};

use super::{Color, Vec2, common::fixed_string_to_string};

#[bitsize(32, new = pub)]
#[derive(
    Clone, Copy, DebugBits, DefaultBits, DeserializeBits, Eq, FromBits, PartialEq, SerializeBits,
)]
pub struct MaterialFlags {
    pub texture_reference_count: u2,
    pub texture_transform_count: u2,
    pub texture_coordinate_generator_count: u2,
    pub tev_stage_count: u3,
    pub has_alpha_compare: bool,
    pub has_blend_mode: bool,
    pub texture_only: bool,
    pub has_separate_blend_logic_mode: bool,
    pub unk1: bool,
    pub has_indirect_texture_transform: bool,
    pub projection_texture_parameter_count: u2,
    pub has_font_shadow_parameter: bool,
    pub alpha_interpolation: bool,
    pub reserved: u13,
}

#[bitsize(2)]
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, FromBits, PartialEq, Serialize)]
pub enum TextureWrapMode {
    #[default]
    Clamp,
    Repeat,
    Mirror,
    Reserved,
}

#[bitsize(2)]
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, FromBits, PartialEq, Serialize)]
pub enum TextureFilterMode {
    #[default]
    Nearest,
    Linear,
    Reserved2,
    Reserved3,
}

#[bitsize(8, new = pub)]
#[derive(
    Clone, Copy, DebugBits, DefaultBits, DeserializeBits, Eq, FromBits, PartialEq, SerializeBits,
)]
pub struct TextureWrapFlags {
    pub u: TextureWrapMode,
    pub v: TextureWrapMode,
    pub reserved: u4,
}

#[bitsize(8, new = pub)]
#[derive(
    Clone, Copy, DebugBits, DefaultBits, DeserializeBits, Eq, FromBits, PartialEq, SerializeBits,
)]
pub struct TextureFilterFlags {
    pub minification: TextureFilterMode,
    pub magnification: TextureFilterMode,
    pub reserved: u4,
}

#[derive(Serialize, Deserialize)]
#[binrw]
pub struct TextureReference {
    pub texture_index: i16,
    #[br(map = |bits: u8| TextureWrapFlags::from(u8::new(bits)))]
    #[bw(map = |flags: &TextureWrapFlags| u8::from(*flags).value())]
    pub wrap_flags: TextureWrapFlags,
    #[br(map = |bits: u8| TextureFilterFlags::from(u8::new(bits)))]
    #[bw(map = |flags: &TextureFilterFlags| u8::from(*flags).value())]
    pub filter_flags: TextureFilterFlags,
}

#[derive(Serialize, Deserialize)]
#[binrw]
pub struct TextureTransform {
    pub translation: Vec2,
    pub rotation: f32,
    pub scale: Vec2,
}

#[derive(Serialize, Deserialize)]
#[binrw]
pub struct TextureCoordinateGenerator {
    pub matrix: u8,
    pub source: u8,
    pub unknown: [u8; 14],
}

/// One TEV stage. Color and alpha modes are deliberately separate raw values.
#[derive(Serialize, Deserialize)]
#[binrw]
pub struct TevStage {
    pub color_mode: u8,
    pub alpha_mode: u8,
    pub reserved: u16,
}

#[derive(Serialize, Deserialize)]
#[binrw]
pub struct AlphaCompare {
    pub compare_mode: u8,
    pub reserved: [u8; 3],
    pub reference: f32,
}

#[derive(Serialize, Deserialize)]
#[binrw]
pub struct BlendMode {
    pub operation: u8,
    pub source_factor: u8,
    pub destination_factor: u8,
    pub logic_operation: u8,
}

#[derive(Serialize, Deserialize)]
#[binrw]
pub struct IndirectTextureTransform {
    pub rotation: f32,
    pub scale: Vec2,
}

#[derive(Serialize, Deserialize)]
#[binrw]
pub struct ProjectionTextureParameter {
    pub translation: Vec2,
    pub scale: Vec2,
    pub mode: u8,
    pub reserved: [u8; 3],
}

#[derive(Serialize, Deserialize)]
#[binrw]
pub struct FontShadowParameter {
    pub black_color: Color,
    pub white_color: Color,
}

#[derive(BinRead, Serialize, Deserialize)]
pub struct MaterialEntry {
    #[br(try_map = fixed_string_to_string::<0x1c>)]
    pub name: String,
    #[br(map = |raw: u32| MaterialFlags::from(u32::new(raw)))]
    pub flags: MaterialFlags,
    pub unknown_header_value: u32,
    pub black_color: Color,
    pub white_color: Color,
    #[br(count = flags.texture_reference_count().value())]
    pub texture_references: Vec<TextureReference>,
    #[br(count = flags.texture_transform_count().value())]
    pub texture_transforms: Vec<TextureTransform>,
    #[br(count = flags.texture_coordinate_generator_count().value())]
    pub texture_coordinate_generators: Vec<TextureCoordinateGenerator>,
    #[br(count = flags.tev_stage_count().value())]
    pub tev_stages: Vec<TevStage>,
    #[br(if(flags.has_alpha_compare()))]
    pub alpha_compare: Option<AlphaCompare>,
    #[br(if(flags.has_blend_mode()))]
    pub blend_mode: Option<BlendMode>,
    #[br(if(flags.has_separate_blend_logic_mode()))]
    pub separate_blend_logic_mode: Option<BlendMode>,
    #[br(if(flags.has_indirect_texture_transform()))]
    pub indirect_texture_transform: Option<IndirectTextureTransform>,
    #[br(count = flags.projection_texture_parameter_count().value())]
    pub projection_texture_parameters: Vec<ProjectionTextureParameter>,
    #[br(if(flags.has_font_shadow_parameter()))]
    pub font_shadow_parameter: Option<FontShadowParameter>,
}

#[derive(Serialize, Deserialize)]
pub struct DecodedMaterialSection {
    pub reserved: u16,
    pub materials: Vec<MaterialEntry>,
}

pub fn decode_material_payload(payload: &[u8]) -> BinResult<DecodedMaterialSection> {
    if payload.len() < 4 {
        return invalid_material_payload(0, "material count table is truncated");
    }

    let mut header = Cursor::new(payload);
    let material_count = u16::read_options(&mut header, Endian::Little, ())
        .map_err(|_| material_error(0, "material count table is truncated"))?
        as usize;
    let reserved = u16::read_options(&mut header, Endian::Little, ())
        .map_err(|_| material_error(2, "material count table is truncated"))?;
    let table_end = 4usize
        .checked_add(
            material_count
                .checked_mul(4)
                .ok_or_else(|| material_error(0, "material offset table is too large"))?,
        )
        .ok_or_else(|| material_error(0, "material offset table is too large"))?;
    if table_end > payload.len() {
        return invalid_material_payload(0, "material offset table is truncated");
    }

    let mut offsets = Vec::with_capacity(material_count);
    let mut previous_offset = 8 + table_end;
    for index in 0..material_count {
        let offset = u32::read_options(&mut header, Endian::Little, ())
            .map_err(|_| material_error(4 + index * 4, "material offset table is truncated"))?
            as usize;
        if offset < 8 + table_end {
            return invalid_material_payload(
                4 + index * 4,
                "material offset points into the offset table",
            );
        }
        if offset > payload.len() + 8 {
            return invalid_material_payload(
                4 + index * 4,
                "material offset is outside the section",
            );
        }
        if offset < previous_offset {
            return invalid_material_payload(4 + index * 4, "material offsets are not monotonic");
        }
        previous_offset = offset;
        offsets.push(offset);
    }

    let mut materials = Vec::with_capacity(material_count);
    for (index, &section_offset) in offsets.iter().enumerate() {
        let record_end = offsets.get(index + 1).copied().unwrap_or(payload.len() + 8);
        let record_start = section_offset - 8;
        let record_end = record_end - 8;
        materials.push(decode_material_record(
            &payload[record_start..record_end],
            section_offset,
        )?);
    }

    Ok(DecodedMaterialSection {
        reserved,
        materials,
    })
}

fn decode_material_record(record: &[u8], section_offset: usize) -> BinResult<MaterialEntry> {
    if record.len() < 44 {
        return invalid_material_payload(section_offset, "material record header is truncated");
    }

    let mut reader = Cursor::new(record);
    let material = MaterialEntry::read_options(&mut reader, Endian::Little, ()).map_err(|_| {
        material_error(
            section_offset + reader.position() as usize,
            "material record is truncated",
        )
    })?;

    if reader.position() != record.len() as u64 {
        return invalid_material_payload(
            section_offset + reader.position() as usize,
            "material record has unrecognized trailing bytes",
        );
    }

    Ok(material)
}

fn invalid_material_payload<T>(position: usize, message: impl Into<String>) -> BinResult<T> {
    Err(material_error(position, message))
}

fn material_error(position: usize, message: impl Into<String>) -> BinError {
    BinError::Custom {
        pos: position as u64,
        err: Box::new(Error::new(ErrorKind::InvalidData, message.into())),
    }
}

#[derive(Serialize, Deserialize)]
#[binrw]
#[brw(magic = b"mat1")]
pub struct Material {
    #[serde(skip)]
    #[bw(map = |_: &u32| 8 + data.len() as u32)]
    size: u32,
    #[br(count = size - 8)]
    pub data: Vec<u8>,
}

impl Material {
    pub fn new(data: Vec<u8>) -> Self {
        Self { size: 0, data }
    }
}
