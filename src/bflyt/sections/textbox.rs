use binrw::{BinResult, Endian, binrw};
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, Write};

use super::{Color, PaneInfo, Vec2};

#[derive(Serialize, Deserialize)]
#[binrw]
#[brw(magic = b"txt1")]
pub struct Textbox {
    #[serde(skip)]
    #[bw(map = |_: &u32| 0xA0 + data.serialized_size() as u32)]
    pub size: u32,
    pub pane: PaneInfo,
    pub text_buffer_length: u16,
    pub text_restrict_length: u16,
    pub material_index: u16,
    pub font_index: u16,
    pub origin: u8,
    pub line_alignment: u8,
    pub active_shadows: u8,
    pub padding: u8,
    pub italic_tilt: f32,
    #[serde(skip)]
    #[bw(map = |_: &u32| 0xA0)]
    text_offset: u32,
    pub top_color: Color,
    pub bottom_color: Color,
    pub font_size: Vec2,
    pub character_size: f32,
    pub line_size: f32,
    #[serde(skip)]
    #[bw(map = |_: &u32| data.name_offset() as u32)]
    textbox_name_offset: u32,
    pub shadow_offset: Vec2,
    pub shadow_scale: Vec2,
    pub shadow_top_color: Color,
    pub shadow_bottom_color: Color,
    pub shadow_italic_tilt: f32,
    /// Variable data beginning at `0xA0`.
    #[br(parse_with = read_textbox_data, args(size, text_offset, textbox_name_offset))]
    #[bw(write_with = write_textbox_data)]
    pub data: TextboxData,
}

#[derive(Serialize, Deserialize)]
pub struct TextboxData {
    pub text: String,
    pub call_name: Option<String>,
    pub per_character_transform: Option<Vec<u8>>,
}

impl TextboxData {
    fn text_size(&self) -> usize {
        (self.text.encode_utf16().count() + 1) * 2
    }
    fn serialized_size(&self) -> usize {
        let text_size = align(self.text_size(), 4);
        let name_size = self
            .call_name
            .as_ref()
            .map_or(0, |name| align(name.len() + 1, 4));
        text_size + name_size + self.per_character_transform.as_ref().map_or(0, Vec::len)
    }
    fn name_offset(&self) -> usize {
        self.call_name
            .as_ref()
            .map_or(0, |_| 0xA0 + align(self.text_size(), 4))
    }
}

fn align(size: usize, alignment: usize) -> usize {
    (size + alignment - 1) & !(alignment - 1)
}

fn read_textbox_data<R: Read + Seek>(
    reader: &mut R,
    endian: Endian,
    (size, text_offset, name_offset): (u32, u32, u32),
) -> BinResult<TextboxData> {
    let mut bytes = vec![0; (size - 0xA0) as usize];
    reader.read_exact(&mut bytes)?;
    let decode_utf16 = |offset: u32| -> String {
        let mut units = Vec::new();
        let mut index = (offset - 0xA0) as usize;
        while index + 1 < bytes.len() {
            let unit = if endian == Endian::Little {
                u16::from_le_bytes([bytes[index], bytes[index + 1]])
            } else {
                u16::from_be_bytes([bytes[index], bytes[index + 1]])
            };
            if unit == 0 {
                break;
            }
            units.push(unit);
            index += 2;
        }
        String::from_utf16_lossy(&units)
    };
    let read_name = |offset: u32| -> Option<String> {
        if offset == 0 {
            None
        } else {
            let start = (offset - 0xA0) as usize;
            let end = bytes[start..].iter().position(|byte| *byte == 0)? + start;
            Some(String::from_utf8_lossy(&bytes[start..end]).into_owned())
        }
    };
    Ok(TextboxData {
        text: decode_utf16(text_offset),
        call_name: read_name(name_offset),
        per_character_transform: None,
    })
}

fn write_textbox_data<W: Write + Seek>(
    data: &TextboxData,
    writer: &mut W,
    endian: Endian,
    _: (),
) -> BinResult<()> {
    let mut bytes = Vec::new();
    for unit in data.text.encode_utf16().chain(std::iter::once(0)) {
        bytes.extend_from_slice(&if endian == Endian::Little {
            unit.to_le_bytes()
        } else {
            unit.to_be_bytes()
        });
    }
    bytes.resize(align(bytes.len(), 4), 0);
    if let Some(name) = &data.call_name {
        bytes.extend(name.bytes());
        bytes.push(0);
        bytes.resize(align(bytes.len(), 4), 0);
    }
    if let Some(transform) = &data.per_character_transform {
        bytes.extend(transform);
    }
    writer.write_all(&bytes)?;
    Ok(())
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
            origin: 0,
            line_alignment: 0,
            active_shadows: 0,
            padding: 0,
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
            data: TextboxData {
                text: String::new(),
                call_name: None,
                per_character_transform: None,
            },
        }
    }
}
