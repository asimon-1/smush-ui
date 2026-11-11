use bilge::prelude::*;
use binrw::{BinRead, BinResult, Endian, binrw};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read, Seek, Write};

use super::common::write_u32;
use super::{Color, PaneInfo, Vec2};

const TEXTBOX_HEADER_SIZE: usize = 0xA8;

#[bitsize(2)]
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, FromBits, PartialEq, Serialize)]
pub enum TextHorizontalOrigin {
    #[default]
    Center,
    Left,
    Right,
    Reserved,
}

#[bitsize(2)]
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, FromBits, PartialEq, Serialize)]
pub enum TextVerticalOrigin {
    #[default]
    Center,
    Top,
    Bottom,
    Reserved,
}

#[bitsize(8, new = pub)]
#[derive(
    Clone, Copy, DebugBits, DefaultBits, DeserializeBits, Eq, FromBits, PartialEq, SerializeBits,
)]
pub struct TextOrigin {
    pub vertical: TextVerticalOrigin,
    pub horizontal: TextHorizontalOrigin,
    pub reserved: u4,
}

#[bitsize(8)]
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, FromBits, PartialEq, Serialize)]
pub enum TextAlignment {
    Synchronous,
    Left,
    Center,
    #[default]
    Right,
    #[fallback]
    Reserved(u8),
}

#[bitsize(16, new = pub)]
#[derive(
    Clone, Copy, DebugBits, DefaultBits, DeserializeBits, Eq, FromBits, PartialEq, SerializeBits,
)]
pub struct TextPaneFlags {
    pub enable_shadow: bool,
    pub limit_glyph_count_to_length: bool,
    pub invisible_border: bool,
    pub double_border: bool,
    pub enable_per_character_transform: bool,
    pub enable_center_ceiling: bool,
    pub enable_line_transform: bool,
    pub use_default_tag_processor: bool,
    pub split_per_character_transform_by_character_width: bool,
    pub mix_shadow_alpha: bool,
    pub reverse_text: bool,
    pub per_character_transform_origin_is_center: bool,
    pub keep_font_scale: bool,
    pub per_character_transform_fix_space: bool,
    pub split_inserted_space: bool,
    pub reserved: u1,
}

#[bitsize(8)]
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, FromBits, PartialEq, Serialize)]
pub enum PerCharacterTransformLoopMode {
    #[default]
    Once,
    Loop,
    #[fallback]
    Reserved(u8),
}

#[derive(Serialize, Deserialize)]
#[binrw]
#[brw(magic = b"txt1")]
pub struct Textbox {
    #[serde(skip)]
    #[bw(map = |_: &u32| TEXTBOX_HEADER_SIZE as u32 + data.serialized_size() as u32)]
    pub size: u32,
    pub pane: PaneInfo,
    pub text_buffer_length: u16,
    pub text_restrict_length: u16,
    pub material_index: u16,
    pub font_index: u16,
    #[br(map = |bits: u8| TextOrigin::from(u8::new(bits)))]
    #[bw(map = |origin: &TextOrigin| u8::from(*origin).value())]
    pub origin: TextOrigin,
    #[br(map = |bits: u8| TextAlignment::from(u8::new(bits)))]
    #[bw(map = |alignment: &TextAlignment| u8::from(*alignment).value())]
    pub line_alignment: TextAlignment,
    #[br(map = |bits: u16| TextPaneFlags::from(u16::new(bits)))]
    #[bw(map = |flags: &TextPaneFlags| u16::from(*flags).value())]
    pub flags: TextPaneFlags,
    pub italic_tilt: f32,
    #[serde(skip)]
    #[bw(map = |_: &u32| data.layout().text_offset as u32)]
    text_offset: u32,
    pub top_color: Color,
    pub bottom_color: Color,
    pub font_size: Vec2,
    pub character_size: f32,
    pub line_size: f32,
    #[serde(skip)]
    #[bw(map = |_: &u32| data.layout().call_name_offset.unwrap_or(0) as u32)]
    textbox_name_offset: u32,
    pub shadow_offset: Vec2,
    pub shadow_scale: Vec2,
    pub shadow_top_color: Color,
    pub shadow_bottom_color: Color,
    pub shadow_italic_tilt: f32,
    #[serde(skip)]
    #[bw(map = |_: &u32| data.layout().line_transform_offset.unwrap_or(0) as u32)]
    line_transform_offset: u32,
    #[serde(skip)]
    #[bw(map = |_: &u32| data.layout().per_character_transform_offset.unwrap_or(0) as u32)]
    per_character_transform_offset: u32,
    #[br(parse_with = read_textbox_data, args(size, text_offset, textbox_name_offset, line_transform_offset, per_character_transform_offset))]
    #[bw(write_with = write_textbox_data)]
    pub data: TextboxData,
}

#[derive(Serialize, Deserialize)]
pub struct TextboxData {
    pub text: String,
    pub call_name: Option<String>,
    pub line_transforms: Option<Vec<LineTransform>>,
    pub per_character_transform: Option<PerCharacterTransform>,
}

#[derive(Serialize, Deserialize)]
pub struct LineTransform {
    pub offset: f32,
    pub width: f32,
}

#[derive(Serialize, Deserialize)]
pub struct PerCharacterTransform {
    pub curve_time_offset: f32,
    pub curve_time_width: f32,
    pub loop_mode: PerCharacterTransformLoopMode,
    pub rotation_origin: TextVerticalOrigin,
    pub rotation_origin_offset: f32,
    pub fixed_space_width: f32,
    pub fixed_space_origin: TextHorizontalOrigin,
    /// Opaque animation-info payload, if the transform contains one.
    pub animation_data: Option<Vec<u8>>,
}

#[derive(Default)]
struct TextboxDataLayout {
    text_offset: usize,
    call_name_offset: Option<usize>,
    line_transform_offset: Option<usize>,
    per_character_transform_offset: Option<usize>,
    size: usize,
}

impl TextboxData {
    fn layout(&self) -> TextboxDataLayout {
        let mut offset = TEXTBOX_HEADER_SIZE;
        let text_offset = offset;
        offset += align(self.text_size(), 4);

        let call_name_offset = self.call_name.as_ref().map(|name| {
            let result = offset;
            offset += align(name.len() + 1, 4);
            result
        });
        let line_transform_offset = self.line_transforms.as_ref().map(|transforms| {
            offset = align(offset, 4);
            let result = offset;
            offset += 1 + transforms.len() * 8;
            result
        });
        let per_character_transform_offset =
            self.per_character_transform.as_ref().map(|transform| {
                offset = align(offset, 4);
                let result = offset;
                offset += transform.serialized_size();
                result
            });

        TextboxDataLayout {
            text_offset,
            call_name_offset,
            line_transform_offset,
            per_character_transform_offset,
            size: offset - TEXTBOX_HEADER_SIZE,
        }
    }

    fn text_size(&self) -> usize {
        (self.text.encode_utf16().count() + 1) * 2
    }

    fn serialized_size(&self) -> usize {
        self.layout().size
    }
}

impl PerCharacterTransform {
    fn serialized_size(&self) -> usize {
        32 + self.animation_data.as_ref().map_or(0, Vec::len)
    }
}

fn align(size: usize, alignment: usize) -> usize {
    (size + alignment - 1) & !(alignment - 1)
}

fn read_textbox_data<R: Read + Seek>(
    reader: &mut R,
    endian: Endian,
    (size, text_offset, name_offset, line_transform_offset, per_character_transform_offset): (
        u32,
        u32,
        u32,
        u32,
        u32,
    ),
) -> BinResult<TextboxData> {
    let mut bytes = vec![0; (size as usize).saturating_sub(TEXTBOX_HEADER_SIZE)];
    reader.read_exact(&mut bytes)?;
    let payload = |offset: u32| -> Option<&[u8]> {
        (offset >= TEXTBOX_HEADER_SIZE as u32)
            .then(|| bytes.get((offset as usize) - TEXTBOX_HEADER_SIZE..))
            .flatten()
    };
    let decode_utf16 = |offset: u32| -> String {
        let Some(bytes) = payload(offset) else {
            return String::new();
        };
        let units = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| match endian {
                Endian::Little => u16::from_le_bytes(*pair),
                Endian::Big => u16::from_be_bytes(*pair),
            })
            .take_while(|unit| *unit != 0)
            .collect::<Vec<_>>();
        String::from_utf16_lossy(&units)
    };
    let read_name = |offset: u32| -> Option<String> {
        let bytes = payload(offset)?;
        let end = bytes.iter().position(|byte| *byte == 0)?;
        Some(String::from_utf8_lossy(&bytes[..end]).into_owned())
    };

    Ok(TextboxData {
        text: decode_utf16(text_offset),
        call_name: (name_offset != 0).then(|| read_name(name_offset)).flatten(),
        line_transforms: (line_transform_offset != 0)
            .then(|| {
                payload(line_transform_offset).map(|bytes| read_line_transforms(bytes, endian))
            })
            .flatten()
            .transpose()?,
        per_character_transform: (per_character_transform_offset != 0)
            .then(|| {
                payload(per_character_transform_offset)
                    .map(|bytes| read_per_character_transform(bytes, endian))
            })
            .flatten()
            .transpose()?,
    })
}

fn read_line_transforms(bytes: &[u8], endian: Endian) -> BinResult<Vec<LineTransform>> {
    let mut reader = Cursor::new(bytes);
    let count = u8::read_options(&mut reader, endian, ())? as usize;
    let offsets = (0..count)
        .map(|_| f32::read_options(&mut reader, endian, ()))
        .collect::<BinResult<Vec<_>>>()?;
    let widths = (0..count)
        .map(|_| f32::read_options(&mut reader, endian, ()))
        .collect::<BinResult<Vec<_>>>()?;
    Ok(offsets
        .into_iter()
        .zip(widths)
        .map(|(offset, width)| LineTransform { offset, width })
        .collect())
}

fn read_per_character_transform(bytes: &[u8], endian: Endian) -> BinResult<PerCharacterTransform> {
    let mut reader = Cursor::new(bytes);
    let curve_time_offset = f32::read_options(&mut reader, endian, ())?;
    let curve_time_width = f32::read_options(&mut reader, endian, ())?;
    let loop_mode =
        PerCharacterTransformLoopMode::from(u8::new(u8::read_options(&mut reader, endian, ())?));
    let rotation_origin =
        TextVerticalOrigin::from(u2::new(u8::read_options(&mut reader, endian, ())?));
    let has_animation_data = u8::read_options(&mut reader, endian, ())? != 0;
    let _padding = u8::read_options(&mut reader, endian, ())?;
    let rotation_origin_offset = f32::read_options(&mut reader, endian, ())?;
    let fixed_space_width = f32::read_options(&mut reader, endian, ())?;
    let fixed_space_origin =
        TextHorizontalOrigin::from(u2::new(u8::read_options(&mut reader, endian, ())?));
    let mut padding = [0; 11];
    reader.read_exact(&mut padding)?;
    let animation_start = reader.position() as usize;

    Ok(PerCharacterTransform {
        curve_time_offset,
        curve_time_width,
        loop_mode,
        rotation_origin,
        rotation_origin_offset,
        fixed_space_width,
        fixed_space_origin,
        animation_data: has_animation_data.then(|| bytes[animation_start..].to_vec()),
    })
}

fn write_textbox_data<W: Write + Seek>(
    data: &TextboxData,
    writer: &mut W,
    endian: Endian,
    _: (),
) -> BinResult<()> {
    let mut bytes = Vec::with_capacity(data.serialized_size());
    for unit in data.text.encode_utf16().chain(std::iter::once(0)) {
        write_u16(&mut bytes, unit, endian);
    }
    bytes.resize(align(bytes.len(), 4), 0);
    if let Some(name) = &data.call_name {
        bytes.extend(name.bytes());
        bytes.push(0);
        bytes.resize(align(bytes.len(), 4), 0);
    }
    if let Some(transforms) = &data.line_transforms {
        bytes.resize(align(bytes.len(), 4), 0);
        bytes.push(transforms.len() as u8);
        for transform in transforms {
            write_u32(&mut bytes, transform.offset.to_bits(), endian);
        }
        for transform in transforms {
            write_u32(&mut bytes, transform.width.to_bits(), endian);
        }
    }
    if let Some(transform) = &data.per_character_transform {
        bytes.resize(align(bytes.len(), 4), 0);
        write_u32(&mut bytes, transform.curve_time_offset.to_bits(), endian);
        write_u32(&mut bytes, transform.curve_time_width.to_bits(), endian);
        bytes.push(u8::from(transform.loop_mode).value());
        bytes.push(u2::from(transform.rotation_origin).value());
        bytes.push(transform.animation_data.is_some() as u8);
        bytes.push(0);
        write_u32(
            &mut bytes,
            transform.rotation_origin_offset.to_bits(),
            endian,
        );
        write_u32(&mut bytes, transform.fixed_space_width.to_bits(), endian);
        bytes.push(u2::from(transform.fixed_space_origin).value());
        bytes.extend([0; 11]);
        if let Some(animation_data) = &transform.animation_data {
            bytes.extend(animation_data);
        }
    }
    writer.write_all(&bytes)?;
    Ok(())
}

fn write_u16(bytes: &mut Vec<u8>, value: u16, endian: Endian) {
    bytes.extend(if endian == Endian::Little {
        value.to_le_bytes()
    } else {
        value.to_be_bytes()
    });
}

impl Textbox {
    pub fn new(pane: PaneInfo) -> Self {
        Self {
            size: 0,
            pane,
            text_buffer_length: 0,
            text_restrict_length: 0,
            material_index: 0,
            font_index: 0,
            origin: TextOrigin::default(),
            line_alignment: TextAlignment::default(),
            flags: TextPaneFlags::default(),
            italic_tilt: 0.0,
            text_offset: 0,
            top_color: Color {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 0,
            },
            bottom_color: Color {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 0,
            },
            font_size: Vec2 { x: 0.0, y: 0.0 },
            character_size: 0.0,
            line_size: 0.0,
            textbox_name_offset: 0,
            shadow_offset: Vec2 { x: 0.0, y: 0.0 },
            shadow_scale: Vec2 { x: 0.0, y: 0.0 },
            shadow_top_color: Color {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 0,
            },
            shadow_bottom_color: Color {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 0,
            },
            shadow_italic_tilt: 0.0,
            line_transform_offset: 0,
            per_character_transform_offset: 0,
            data: TextboxData {
                text: String::new(),
                call_name: None,
                line_transforms: None,
                per_character_transform: None,
            },
        }
    }
}
