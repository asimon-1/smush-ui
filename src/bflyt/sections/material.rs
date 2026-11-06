use bilge::prelude::*;
use binrw::{BinRead, BinResult, BinWrite, Endian, Error as BinError, binrw};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Error, ErrorKind, Read, Seek, Write};

use super::{
    Color, Vec2,
    common::{
        align_up, fixed_string_to_string, lossless_vec2, string_to_fixed_string, write_u32,
        write_u32_at,
    },
};

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
    #[serde(with = "lossless_vec2")]
    pub translation: Vec2,
    #[serde(with = "lossless_vec2")]
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
    #[serde(skip)]
    flags: MaterialFlags,
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
    #[br(count = 44, if(flags.reserved_i().value() & 1 != 0))]
    pub unk1: Option<Vec<u8>>,
    #[br(calc = false)]
    pub texture_only: bool,
    #[br(calc = false)]
    pub unk2: bool,
    #[br(calc = false)]
    pub alpha_interpolation: bool,
    #[br(calc = u12::new(0))]
    pub reserved_flags: u12,
}

pub fn decode_material_payload(payload: &[u8]) -> BinResult<Material> {
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

    Ok(Material {
        reserved,
        materials,
    })
}

pub fn encode_material_payload(material: &Material) -> BinResult<Vec<u8>> {
    let material_count = u16::try_from(material.materials.len())
        .map_err(|_| material_error(0, "material count exceeds u16"))?;
    let table_end = 4 + material.materials.len() * 4;
    let mut payload = vec![0; table_end];
    payload[..2].copy_from_slice(&material_count.to_le_bytes());
    payload[2..4].copy_from_slice(&material.reserved.to_le_bytes());

    for (index, entry) in material.materials.iter().enumerate() {
        let offset = u32::try_from(payload.len() + 8)
            .map_err(|_| material_error(payload.len(), "material section is too large"))?;
        write_u32_at(&mut payload, 4 + index * 4, offset, Endian::Little);
        let record = encode_material_record(entry)?;
        payload.extend(record);
        payload.resize(align_up(payload.len(), 4), 0);
    }

    Ok(payload)
}

fn encode_material_record(material: &MaterialEntry) -> BinResult<Vec<u8>> {
    validate_material(material)?;

    let mut record = Vec::new();
    record.extend(
        string_to_fixed_string::<0x1c>(&material.name)
            .map_err(|error| material_error(0, error.to_string()))?,
    );
    write_u32(&mut record, reconstructed_flags(material), Endian::Little);
    write_u32(&mut record, material.unknown_header_value, Endian::Little);

    let mut writer = Cursor::new(record);
    writer.set_position(writer.get_ref().len() as u64);
    macro_rules! write_component {
        ($value:expr) => {
            BinWrite::write_options($value, &mut writer, Endian::Little, ())?
        };
    }

    write_component!(&material.black_color);
    write_component!(&material.white_color);
    for value in &material.texture_references {
        write_component!(value);
    }
    for value in &material.texture_transforms {
        write_component!(value);
    }
    for value in &material.texture_coordinate_generators {
        write_component!(value);
    }
    for value in &material.tev_stages {
        write_component!(value);
    }
    if let Some(value) = &material.alpha_compare {
        write_component!(value);
    }
    if let Some(value) = &material.blend_mode {
        write_component!(value);
    }
    if let Some(value) = &material.separate_blend_logic_mode {
        write_component!(value);
    }
    if let Some(value) = &material.indirect_texture_transform {
        write_component!(value);
    }
    for value in &material.projection_texture_parameters {
        write_component!(value);
    }
    if let Some(value) = &material.font_shadow_parameter {
        write_component!(value);
    }
    if let Some(value) = &material.unk1 {
        write_component!(value);
    }

    Ok(writer.into_inner())
}

fn validate_material(material: &MaterialEntry) -> BinResult<()> {
    if material.texture_references.len() > 3
        || material.texture_transforms.len() > 3
        || material.texture_coordinate_generators.len() > 3
        || material.projection_texture_parameters.len() > 3
    {
        return invalid_material_payload(0, "material component count exceeds 3");
    }
    if material.tev_stages.len() > 7 {
        return invalid_material_payload(0, "material TEV stage count exceeds 7");
    }
    if material.unk1.as_ref().is_some_and(|data| data.len() != 44) {
        return invalid_material_payload(0, "unknown bit-19 data must be 44 bytes");
    }
    if material.name.len() > 0x1c {
        return invalid_material_payload(0, "material name exceeds 28 bytes");
    }
    Ok(())
}

fn reconstructed_flags(material: &MaterialEntry) -> u32 {
    const DERIVED_FLAG_MASK: u32 = 0x000b_d7ff;

    (((u32::from(material.reserved_flags).value() << 20)
        | ((material.texture_only as u32) << 11)
        | ((material.unk2 as u32) << 13)
        | ((material.alpha_interpolation as u32) << 18))
        & !DERIVED_FLAG_MASK)
        | material.texture_references.len() as u32
        | ((material.texture_transforms.len() as u32) << 2)
        | ((material.texture_coordinate_generators.len() as u32) << 4)
        | ((material.tev_stages.len() as u32) << 6)
        | ((material.alpha_compare.is_some() as u32) << 9)
        | ((material.blend_mode.is_some() as u32) << 10)
        | ((material.separate_blend_logic_mode.is_some() as u32) << 12)
        | ((material.indirect_texture_transform.is_some() as u32) << 14)
        | ((material.projection_texture_parameters.len() as u32) << 15)
        | ((material.font_shadow_parameter.is_some() as u32) << 17)
        | ((material.unk1.is_some() as u32) << 19)
}

fn decode_material_record(record: &[u8], section_offset: usize) -> BinResult<MaterialEntry> {
    if record.len() < 44 {
        return invalid_material_payload(section_offset, "material record header is truncated");
    }

    let mut reader = Cursor::new(record);
    let mut material =
        MaterialEntry::read_options(&mut reader, Endian::Little, ()).map_err(|_| {
            material_error(
                section_offset + reader.position() as usize,
                "material record is truncated",
            )
        })?;
    material.texture_only = material.flags.texture_only();
    material.unk2 = material.flags.unk1();
    material.alpha_interpolation = material.flags.alpha_interpolation();
    material.reserved_flags = u12::new(material.flags.reserved_i().value() >> 1);

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
pub struct Material {
    #[serde(skip)]
    reserved: u16,
    pub materials: Vec<MaterialEntry>,
}

impl Material {
    pub fn new(materials: Vec<MaterialEntry>) -> Self {
        Self {
            reserved: 0,
            materials,
        }
    }
}

impl BinRead for Material {
    type Args<'a> = ();

    fn read_options<R: Read + Seek>(
        reader: &mut R,
        _: Endian,
        _: Self::Args<'_>,
    ) -> BinResult<Self> {
        let section_start = reader.stream_position()?;
        let mut magic = [0; 4];
        reader.read_exact(&mut magic)?;
        if &magic != b"mat1" {
            return Err(BinError::AssertFail {
                pos: section_start,
                message: format!("expected mat1, found {:?}", String::from_utf8_lossy(&magic)),
            });
        }

        let size = u32::read_options(reader, Endian::Little, ())?;
        if size < 8 {
            return Err(material_error(
                section_start as usize,
                "material section is smaller than its header",
            ));
        }
        let mut payload = vec![0; (size - 8) as usize];
        reader.read_exact(&mut payload)?;
        let decoded = decode_material_payload(&payload)?;
        Ok(Self {
            reserved: decoded.reserved,
            materials: decoded.materials,
        })
    }
}

impl BinWrite for Material {
    type Args<'a> = ();

    fn write_options<W: Write + Seek>(
        &self,
        writer: &mut W,
        _: Endian,
        _: Self::Args<'_>,
    ) -> BinResult<()> {
        let payload = encode_material_payload(self)?;
        let size = u32::try_from(payload.len() + 8)
            .map_err(|_| material_error(0, "material section is too large"))?;
        writer.write_all(b"mat1")?;
        writer.write_all(&size.to_le_bytes())?;
        writer.write_all(&payload)?;
        Ok(())
    }
}
