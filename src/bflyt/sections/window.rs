use bilge::prelude::*;
use binrw::{BinRead, BinResult, Endian, binrw};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read, Seek, Write};

use super::common::{write_u16, write_u32};
use super::{Color, PaneInfo, Vec2};

#[bitsize(2)]
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, FromBits, PartialEq, Serialize)]
pub enum WindowFrameMode {
    #[default]
    FullFrame,
    HorizontalFrame,
    HorizontalFrameOnly,
    Reserved,
}

#[bitsize(8, new = pub)]
#[derive(
    Clone, Copy, DebugBits, DefaultBits, DeserializeBits, Eq, FromBits, PartialEq, SerializeBits,
)]
pub struct WindowFlags {
    pub use_top_left_frame_material_for_all: bool,
    pub tint_frames_with_content_colors: bool,
    pub frame_mode: WindowFrameMode,
    pub hide_content: bool,
    pub reserved: u3,
}

#[derive(Serialize, Deserialize)]
#[binrw]
#[brw(magic = b"wnd1")]
pub struct Window {
    #[serde(skip)]
    #[bw(map = |_: &u32| 0x70 + data.serialized_size() as u32)]
    pub size: u32,
    pub pane: PaneInfo,
    pub content_expansion_left: i16,
    pub content_expansion_right: i16,
    pub content_expansion_top: i16,
    pub content_expansion_bottom: i16,
    pub frame_thickness_left: i16,
    pub frame_thickness_right: i16,
    pub frame_thickness_top: i16,
    pub frame_thickness_bottom: i16,
    #[serde(skip)]
    #[bw(map = |_: &u8| data.frames.len() as u8)]
    frame_count: u8,
    #[br(map = |bits: u8| WindowFlags::from(u8::new(bits)))]
    #[bw(map = |flags: &WindowFlags| u8::from(*flags).value())]
    pub flags: WindowFlags,
    #[serde(skip)]
    pub padding: [u8; 2],
    #[serde(skip)]
    #[bw(map = |_: &u32| 0x70)]
    content_offset: u32,
    #[serde(skip)]
    #[bw(map = |_: &u32| 0x70 + data.content_size() as u32)]
    frame_table_offset: u32,
    #[br(parse_with = read_window_data, args(size, frame_count, content_offset, frame_table_offset))]
    #[bw(write_with = write_window_data)]
    #[serde(flatten)]
    pub data: WindowData,
}

#[derive(Serialize, Deserialize)]
pub struct WindowData {
    pub content: WindowContent,
    pub frames: Vec<WindowFrame>,
}
#[derive(Serialize, Deserialize)]
pub struct WindowContent {
    pub top_left_color: Color,
    pub top_right_color: Color,
    pub bottom_left_color: Color,
    pub bottom_right_color: Color,
    pub material_index: u16,
    pub texture_coordinates: Vec<[Vec2; 4]>,
}
#[derive(Serialize, Deserialize)]
pub struct WindowFrame {
    pub material_index: u16,
    #[serde(rename = "texture_flip")]
    pub texture_flip: TextureFlip,
}

/// Texture-coordinate transform mode.
#[bitsize(8)]
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, FromBits, PartialEq, Serialize)]
pub enum TextureFlip {
    #[default]
    None,
    Horizontal,
    Vertical,
    Rotate90,
    Rotate180,
    Rotate270,
    /// An unrecognized transform mode, retained when reading and writing.
    #[fallback]
    Reserved(u8),
}

impl WindowData {
    fn content_size(&self) -> usize {
        20 + self.content.texture_coordinates.len() * 32
    }
    fn serialized_size(&self) -> usize {
        self.content_size() + self.frames.len() * 8
    }
}

fn read_window_data<R: Read + Seek>(
    reader: &mut R,
    endian: Endian,
    (size, frame_count, content_offset, frame_offset): (u32, u8, u32, u32),
) -> BinResult<WindowData> {
    let mut payload = vec![0; (size - 0x70) as usize];
    reader.read_exact(&mut payload)?;
    let content = read_window_content(&payload[(content_offset - 0x70) as usize..], endian)?;
    let mut frame_table = Cursor::new(&payload[(frame_offset - 0x70) as usize..]);
    let offsets = (0..frame_count)
        .map(|_| u32::read_options(&mut frame_table, endian, ()))
        .collect::<BinResult<Vec<_>>>()?;
    let frames = offsets
        .into_iter()
        .map(|offset| read_window_frame(&payload[(offset - 0x70) as usize..], endian))
        .collect::<BinResult<_>>()?;
    Ok(WindowData { content, frames })
}

fn read_window_content(bytes: &[u8], endian: Endian) -> BinResult<WindowContent> {
    let mut reader = Cursor::new(bytes);
    let colors = [
        Color::read_options(&mut reader, endian, ())?,
        Color::read_options(&mut reader, endian, ())?,
        Color::read_options(&mut reader, endian, ())?,
        Color::read_options(&mut reader, endian, ())?,
    ];
    let material_index = u16::read_options(&mut reader, endian, ())?;
    let coordinate_count = u8::read_options(&mut reader, endian, ())? as usize;
    let _padding = u8::read_options(&mut reader, endian, ())?;
    let texture_coordinates = (0..coordinate_count)
        .map(|_| <[Vec2; 4]>::read_options(&mut reader, endian, ()))
        .collect::<BinResult<_>>()?;
    let [
        top_left_color,
        top_right_color,
        bottom_left_color,
        bottom_right_color,
    ] = colors;
    Ok(WindowContent {
        top_left_color,
        top_right_color,
        bottom_left_color,
        bottom_right_color,
        material_index,
        texture_coordinates,
    })
}

fn read_window_frame(bytes: &[u8], endian: Endian) -> BinResult<WindowFrame> {
    let mut reader = Cursor::new(bytes);
    Ok(WindowFrame {
        material_index: u16::read_options(&mut reader, endian, ())?,
        texture_flip: TextureFlip::from(u8::new(u8::read_options(&mut reader, endian, ())?)),
    })
}

fn write_window_data<W: Write + Seek>(
    data: &WindowData,
    writer: &mut W,
    endian: Endian,
    _: (),
) -> BinResult<()> {
    let mut bytes = Vec::with_capacity(data.serialized_size());
    data.content.write_to(&mut bytes, endian);
    let frame_data_start = 0x70 + data.content_size() + data.frames.len() * 4;
    for (index, _) in data.frames.iter().enumerate() {
        write_u32(&mut bytes, (frame_data_start + index * 4) as u32, endian);
    }
    for frame in &data.frames {
        frame.write_to(&mut bytes, endian);
    }
    writer.write_all(&bytes)?;
    Ok(())
}

impl WindowContent {
    fn write_to(&self, bytes: &mut Vec<u8>, endian: Endian) {
        for color in [
            &self.top_left_color,
            &self.top_right_color,
            &self.bottom_left_color,
            &self.bottom_right_color,
        ] {
            bytes.extend([color.red, color.green, color.blue, color.alpha]);
        }
        write_u16(bytes, self.material_index, endian);
        bytes.push(self.texture_coordinates.len() as u8);
        bytes.push(0);
        for coordinates in &self.texture_coordinates {
            for coordinate in coordinates {
                write_u32(bytes, coordinate.x.to_bits(), endian);
                write_u32(bytes, coordinate.y.to_bits(), endian);
            }
        }
    }
}

impl WindowFrame {
    fn write_to(&self, bytes: &mut Vec<u8>, endian: Endian) {
        write_u16(bytes, self.material_index, endian);
        bytes.extend([u8::from(self.texture_flip).value(), 0]);
    }
}

impl Window {
    pub fn new(pane: PaneInfo) -> Self {
        Self {
            size: 0,
            pane,
            content_expansion_left: 0,
            content_expansion_right: 0,
            content_expansion_top: 0,
            content_expansion_bottom: 0,
            frame_thickness_left: 0,
            frame_thickness_right: 0,
            frame_thickness_top: 0,
            frame_thickness_bottom: 0,
            frame_count: 0,
            flags: WindowFlags::default(),
            padding: [0; 2],
            content_offset: 0,
            frame_table_offset: 0,
            data: WindowData {
                content: WindowContent {
                    top_left_color: Color {
                        red: 0,
                        green: 0,
                        blue: 0,
                        alpha: 0,
                    },
                    top_right_color: Color {
                        red: 0,
                        green: 0,
                        blue: 0,
                        alpha: 0,
                    },
                    bottom_left_color: Color {
                        red: 0,
                        green: 0,
                        blue: 0,
                        alpha: 0,
                    },
                    bottom_right_color: Color {
                        red: 0,
                        green: 0,
                        blue: 0,
                        alpha: 0,
                    },
                    material_index: 0,
                    texture_coordinates: Vec::new(),
                },
                frames: Vec::new(),
            },
        }
    }
}
