use binrw::binrw;
use serde::{Deserialize, Serialize};
use std::io::{Error, ErrorKind};

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

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PaneFlags {
    pub is_visible: bool,
    pub transmits_alpha_to_children: bool,
    /// Unspecified flag bits, retained when reading and writing.
    pub reserved: u8,
}

impl PaneFlags {
    fn from_bits(bits: u8) -> Self {
        Self {
            is_visible: bits & 0x01 != 0,
            transmits_alpha_to_children: bits & 0x02 != 0,
            reserved: bits & !0x03,
        }
    }

    fn bits(self) -> u8 {
        self.reserved & !0x03
            | u8::from(self.is_visible)
            | (u8::from(self.transmits_alpha_to_children) << 1)
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum HorizontalOrigin {
    #[default]
    Center,
    Left,
    Right,
    Reserved(u8),
}

impl HorizontalOrigin {
    fn from_bits(bits: u8) -> Self {
        match bits & 0x03 {
            0 => Self::Center,
            1 => Self::Left,
            2 => Self::Right,
            value => Self::Reserved(value),
        }
    }

    fn bits(self) -> u8 {
        match self {
            Self::Center => 0,
            Self::Left => 1,
            Self::Right => 2,
            Self::Reserved(value) => value & 0x03,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum VerticalOrigin {
    #[default]
    Center,
    Top,
    Bottom,
    Reserved(u8),
}

impl VerticalOrigin {
    fn from_bits(bits: u8) -> Self {
        match bits & 0x03 {
            0 => Self::Center,
            1 => Self::Top,
            2 => Self::Bottom,
            value => Self::Reserved(value),
        }
    }

    fn bits(self) -> u8 {
        match self {
            Self::Center => 0,
            Self::Top => 1,
            Self::Bottom => 2,
            Self::Reserved(value) => value & 0x03,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PaneOrigin {
    pub x: HorizontalOrigin,
    pub y: VerticalOrigin,
    pub parent_x: HorizontalOrigin,
    pub parent_y: VerticalOrigin,
}

impl PaneOrigin {
    fn from_bits(bits: u8) -> Self {
        Self {
            x: HorizontalOrigin::from_bits(bits),
            y: VerticalOrigin::from_bits(bits >> 2),
            parent_x: HorizontalOrigin::from_bits(bits >> 4),
            parent_y: VerticalOrigin::from_bits(bits >> 6),
        }
    }

    fn bits(self) -> u8 {
        self.x.bits()
            | (self.y.bits() << 2)
            | (self.parent_x.bits() << 4)
            | (self.parent_y.bits() << 6)
    }
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
    #[br(map = PaneFlags::from_bits)]
    #[bw(map = |flags: &PaneFlags| flags.bits())]
    pub flags: PaneFlags,
    /// Origin and parent-origin bitfield.
    #[br(map = PaneOrigin::from_bits)]
    #[bw(map = |origin: &PaneOrigin| origin.bits())]
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
