use bilge::prelude::*;
use binrw::{BinRead, BinResult, Endian, Error as BinError, NullString, binrw};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Error, ErrorKind};

pub(crate) fn align_up(size: usize, alignment: usize) -> usize {
    debug_assert!(alignment.is_power_of_two());
    (size + alignment - 1) & !(alignment - 1)
}

pub(crate) fn read_null_string_at(
    bytes: &[u8],
    offset: usize,
    endian: Endian,
) -> BinResult<String> {
    let mut reader = Cursor::new(&bytes[offset..]);
    NullString::read_options(&mut reader, endian, ())?
        .try_into()
        .map_err(|error| BinError::Custom {
            pos: offset as u64,
            err: Box::new(error),
        })
}

pub(crate) fn write_u16(bytes: &mut Vec<u8>, value: u16, endian: Endian) {
    bytes.extend_from_slice(&if endian == Endian::Little {
        value.to_le_bytes()
    } else {
        value.to_be_bytes()
    });
}

pub(crate) fn write_u32(bytes: &mut Vec<u8>, value: u32, endian: Endian) {
    bytes.extend_from_slice(&if endian == Endian::Little {
        value.to_le_bytes()
    } else {
        value.to_be_bytes()
    });
}

pub(crate) fn write_u32_at(bytes: &mut [u8], offset: usize, value: u32, endian: Endian) {
    let encoded = if endian == Endian::Little {
        value.to_le_bytes()
    } else {
        value.to_be_bytes()
    };
    bytes[offset..offset + 4].copy_from_slice(&encoded);
}

pub(crate) fn fixed_string_to_string<const N: usize>(
    bytes: [u8; N],
) -> Result<String, std::str::Utf8Error> {
    let length = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..length]).map(str::to_owned)
}

pub(crate) fn string_to_fixed_string<const N: usize>(name: &String) -> Result<[u8; N], Error> {
    if name.len() > N {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("fixed BFLYT strings cannot exceed {N} bytes"),
        ));
    }
    let mut bytes = [0; N];
    bytes[..name.len()].copy_from_slice(name.as_bytes());
    Ok(bytes)
}

#[bitsize(8, new = pub)]
#[derive(
    Clone, Copy, DebugBits, DefaultBits, DeserializeBits, Eq, FromBits, PartialEq, SerializeBits,
)]
pub struct PaneFlags {
    pub is_visible: bool,
    pub transmits_alpha_to_children: bool,
    /// Unspecified flag bits, retained when reading and writing.
    pub reserved: u6,
}

#[bitsize(2)]
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, FromBits, PartialEq, Serialize)]
pub enum HorizontalOrigin {
    #[default]
    Center,
    Left,
    Right,
    Reserved,
}

#[bitsize(2)]
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, FromBits, PartialEq, Serialize)]
pub enum VerticalOrigin {
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
pub struct PaneOrigin {
    pub x: HorizontalOrigin,
    pub y: VerticalOrigin,
    pub parent_x: HorizontalOrigin,
    pub parent_y: VerticalOrigin,
}

#[derive(Serialize, Deserialize)]
#[binrw]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

#[derive(Serialize, Deserialize)]
#[binrw]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

#[derive(Serialize, Deserialize)]
#[binrw]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// The 76-byte pane payload at section offsets `0x8..0x54`.
#[derive(Serialize, Deserialize)]
#[binrw]
pub struct PaneInfo {
    /// Bit 0: visible. Bit 1: transmit alpha to children.
    #[br(map = |bits: u8| PaneFlags::from(u8::new(bits)))]
    #[bw(map = |flags: &PaneFlags| u8::from(*flags).value())]
    pub flags: PaneFlags,
    /// Origin and parent-origin bitfield.
    #[br(map = |bits: u8| PaneOrigin::from(u8::new(bits)))]
    #[bw(map = |origin: &PaneOrigin| u8::from(*origin).value())]
    pub origin: PaneOrigin,
    /// Opacity value.
    pub alpha: u8,
    /// Part scaling mode.
    pub part_scaling: u8,
    /// NUL-terminated pane name in a fixed 24-byte buffer.
    #[br(try_map = fixed_string_to_string::<24>)]
    #[bw(try_map = string_to_fixed_string::<24>)]
    pub name: String,
    /// NUL-terminated user name in a fixed 8-byte buffer.
    #[br(try_map = fixed_string_to_string::<8>)]
    #[bw(try_map = string_to_fixed_string::<8>)]
    pub user_name: String,
    pub position: Vec3,
    pub rotation: Vec3,
    pub scale: Vec2,
    pub size: Vec2,
}

#[derive(Serialize, Deserialize)]
#[binrw]
pub struct MaterialInfo {
    pub top_left_color: Color,
    pub top_right_color: Color,
    pub bottom_left_color: Color,
    pub bottom_right_color: Color,
    pub material_index: u16,
    #[serde(skip)]
    #[bw(map = |_: &u16| texture_coordinates.len() as u16)]
    texture_coordinate_count: u16,
    #[br(count = texture_coordinate_count)]
    pub texture_coordinates: Vec<[Vec2; 4]>,
}

impl MaterialInfo {
    pub fn new(
        top_left_color: Color,
        top_right_color: Color,
        bottom_left_color: Color,
        bottom_right_color: Color,
        material_index: u16,
        texture_coordinates: Vec<[Vec2; 4]>,
    ) -> Self {
        Self {
            top_left_color,
            top_right_color,
            bottom_left_color,
            bottom_right_color,
            material_index,
            texture_coordinate_count: 0,
            texture_coordinates,
        }
    }
}
