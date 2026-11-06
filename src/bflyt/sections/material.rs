use bilge::prelude::*;
use binrw::binrw;
use serde::{Deserialize, Serialize};

use super::{Color, Vec2};

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

/// Version-8+ texture-coordinate-generator payload.
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
