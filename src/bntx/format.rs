use bilge::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TypeFormat {
    Unorm,
    Snorm,
    UInt,
    SInt,
    Float,
    Srgb,
    Depth,
    UScaled,
    SScaled,
    UFloat,
    Unknown(u8),
}

impl TypeFormat {
    pub const fn from_raw(raw: u8) -> Self {
        match raw {
            0x01 => Self::Unorm,
            0x02 => Self::Snorm,
            0x03 => Self::UInt,
            0x04 => Self::SInt,
            0x05 => Self::Float,
            0x06 => Self::Srgb,
            0x07 => Self::Depth,
            0x08 => Self::UScaled,
            0x09 => Self::SScaled,
            0x0a => Self::UFloat,
            value => Self::Unknown(value),
        }
    }

    pub const fn to_raw(self) -> u8 {
        match self {
            Self::Unorm => 0x01,
            Self::Snorm => 0x02,
            Self::UInt => 0x03,
            Self::SInt => 0x04,
            Self::Float => 0x05,
            Self::Srgb => 0x06,
            Self::Depth => 0x07,
            Self::UScaled => 0x08,
            Self::SScaled => 0x09,
            Self::UFloat => 0x0a,
            Self::Unknown(value) => value,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ChannelFormat {
    None,
    R8,
    R4G4B4A4,
    R5G5B5A1,
    A1B5G5R5,
    R5G6B5,
    B5G6R5,
    R8G8,
    R16,
    R8G8B8A8,
    B8G8R8A8,
    R9G9B9E5F,
    R10G10B10A2,
    R11G11B10F,
    R16G16,
    D24S8,
    R32,
    R16G16B16A16,
    D32FS8,
    R32G32,
    R32G32B32,
    R32G32B32A32,
    Bc1,
    Bc2,
    Bc3,
    Bc4,
    Bc5,
    Bc6H,
    Bc7U,
    Astc4x4,
    Astc5x4,
    Astc5x5,
    Astc6x5,
    Astc6x6,
    Astc8x5,
    Astc8x6,
    Astc8x8,
    Astc10x5,
    Astc10x6,
    Astc10x8,
    Astc10x10,
    Astc12x10,
    Astc12x12,
    B5G5R5A1Alternate,
    Unknown(u16),
}

impl ChannelFormat {
    pub const fn from_raw(raw: u16) -> Self {
        match raw {
            0x01 => Self::None,
            0x02 => Self::R8,
            0x03 => Self::R4G4B4A4,
            0x05 => Self::R5G5B5A1,
            0x06 => Self::A1B5G5R5,
            0x07 => Self::R5G6B5,
            0x08 => Self::B5G6R5,
            0x09 => Self::R8G8,
            0x0a => Self::R16,
            0x0b => Self::R8G8B8A8,
            0x0c => Self::B8G8R8A8,
            0x0d => Self::R9G9B9E5F,
            0x0e => Self::R10G10B10A2,
            0x0f => Self::R11G11B10F,
            0x12 => Self::R16G16,
            0x13 => Self::D24S8,
            0x14 => Self::R32,
            0x15 => Self::R16G16B16A16,
            0x16 => Self::D32FS8,
            0x17 => Self::R32G32,
            0x18 => Self::R32G32B32,
            0x19 => Self::R32G32B32A32,
            0x1a => Self::Bc1,
            0x1b => Self::Bc2,
            0x1c => Self::Bc3,
            0x1d => Self::Bc4,
            0x1e => Self::Bc5,
            0x1f => Self::Bc6H,
            0x20 => Self::Bc7U,
            0x2d => Self::Astc4x4,
            0x2e => Self::Astc5x4,
            0x2f => Self::Astc5x5,
            0x30 => Self::Astc6x5,
            0x31 => Self::Astc6x6,
            0x32 => Self::Astc8x5,
            0x33 => Self::Astc8x6,
            0x34 => Self::Astc8x8,
            0x35 => Self::Astc10x5,
            0x36 => Self::Astc10x6,
            0x37 => Self::Astc10x8,
            0x38 => Self::Astc10x10,
            0x39 => Self::Astc12x10,
            0x3a => Self::Astc12x12,
            0x3b => Self::B5G5R5A1Alternate,
            value => Self::Unknown(value),
        }
    }

    pub const fn to_raw(self) -> u16 {
        match self {
            Self::None => 0x01,
            Self::R8 => 0x02,
            Self::R4G4B4A4 => 0x03,
            Self::R5G5B5A1 => 0x05,
            Self::A1B5G5R5 => 0x06,
            Self::R5G6B5 => 0x07,
            Self::B5G6R5 => 0x08,
            Self::R8G8 => 0x09,
            Self::R16 => 0x0a,
            Self::R8G8B8A8 => 0x0b,
            Self::B8G8R8A8 => 0x0c,
            Self::R9G9B9E5F => 0x0d,
            Self::R10G10B10A2 => 0x0e,
            Self::R11G11B10F => 0x0f,
            Self::R16G16 => 0x12,
            Self::D24S8 => 0x13,
            Self::R32 => 0x14,
            Self::R16G16B16A16 => 0x15,
            Self::D32FS8 => 0x16,
            Self::R32G32 => 0x17,
            Self::R32G32B32 => 0x18,
            Self::R32G32B32A32 => 0x19,
            Self::Bc1 => 0x1a,
            Self::Bc2 => 0x1b,
            Self::Bc3 => 0x1c,
            Self::Bc4 => 0x1d,
            Self::Bc5 => 0x1e,
            Self::Bc6H => 0x1f,
            Self::Bc7U => 0x20,
            Self::Astc4x4 => 0x2d,
            Self::Astc5x4 => 0x2e,
            Self::Astc5x5 => 0x2f,
            Self::Astc6x5 => 0x30,
            Self::Astc6x6 => 0x31,
            Self::Astc8x5 => 0x32,
            Self::Astc8x6 => 0x33,
            Self::Astc8x8 => 0x34,
            Self::Astc10x5 => 0x35,
            Self::Astc10x6 => 0x36,
            Self::Astc10x8 => 0x37,
            Self::Astc10x10 => 0x38,
            Self::Astc12x10 => 0x39,
            Self::Astc12x12 => 0x3a,
            Self::B5G5R5A1Alternate => 0x3b,
            Self::Unknown(value) => value,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ImageFormat {
    raw: u32,
}

impl ImageFormat {
    pub const fn from_raw(raw: u32) -> Self {
        Self { raw }
    }

    pub const fn to_raw(self) -> u32 {
        self.raw
    }

    pub const fn channel_format(self) -> ChannelFormat {
        ChannelFormat::from_raw((self.raw >> 8) as u16)
    }

    pub const fn type_format(self) -> TypeFormat {
        TypeFormat::from_raw(self.raw as u8)
    }

    pub const fn from_parts(channel: ChannelFormat, ty: TypeFormat) -> Self {
        Self {
            raw: ((channel.to_raw() as u32) << 8) | ty.to_raw() as u32,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ChannelSelector {
    Zero,
    One,
    Red,
    Green,
    Blue,
    Alpha,
    Unknown(u8),
}

impl ChannelSelector {
    pub const fn from_raw(raw: u8) -> Self {
        match raw {
            0 => Self::Zero,
            1 => Self::One,
            2 => Self::Red,
            3 => Self::Green,
            4 => Self::Blue,
            5 => Self::Alpha,
            value => Self::Unknown(value),
        }
    }

    pub const fn to_raw(self) -> u8 {
        match self {
            Self::Zero => 0,
            Self::One => 1,
            Self::Red => 2,
            Self::Green => 3,
            Self::Blue => 4,
            Self::Alpha => 5,
            Self::Unknown(value) => value,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Platform {
    Nx,
    Ounce,
    Pc,
    Gen,
    Unknown([u8; 4]),
}

impl Platform {
    pub const fn from_raw(raw: [u8; 4]) -> Self {
        match raw {
            [b'N', b'X', b' ', b' '] => Self::Nx,
            [b'O', b'u', b'n', b'c'] => Self::Ounce,
            [b'P', b'C', b' ', b' '] => Self::Pc,
            [b'G', b'e', b'n', b' '] => Self::Gen,
            other => Self::Unknown(other),
        }
    }

    pub const fn to_raw(self) -> [u8; 4] {
        match self {
            Self::Nx => *b"NX  ",
            Self::Ounce => *b"Ounc",
            Self::Pc => *b"PC  ",
            Self::Gen => *b"Gen ",
            Self::Unknown(raw) => raw,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TextureDimension {
    OneD,
    TwoD,
    ThreeD,
    Unknown(u8),
}

impl TextureDimension {
    pub const fn from_raw(raw: u8) -> Self {
        match raw {
            1 => Self::OneD,
            2 => Self::TwoD,
            3 => Self::ThreeD,
            value => Self::Unknown(value),
        }
    }

    pub const fn to_raw(self) -> u8 {
        match self {
            Self::OneD => 1,
            Self::TwoD => 2,
            Self::ThreeD => 3,
            Self::Unknown(value) => value,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SurfaceDimension {
    OneD,
    TwoD,
    ThreeD,
    Cube,
    OneDArray,
    TwoDArray,
    TwoDMultisample,
    TwoDMultisampleArray,
    CubeArray,
    Unknown(u32),
}

impl SurfaceDimension {
    pub const fn from_raw(raw: u32) -> Self {
        match raw {
            0 => Self::OneD,
            1 => Self::TwoD,
            2 => Self::ThreeD,
            3 => Self::Cube,
            4 => Self::OneDArray,
            5 => Self::TwoDArray,
            6 => Self::TwoDMultisample,
            7 => Self::TwoDMultisampleArray,
            8 => Self::CubeArray,
            value => Self::Unknown(value),
        }
    }

    pub const fn to_raw(self) -> u32 {
        match self {
            Self::OneD => 0,
            Self::TwoD => 1,
            Self::ThreeD => 2,
            Self::Cube => 3,
            Self::OneDArray => 4,
            Self::TwoDArray => 5,
            Self::TwoDMultisample => 6,
            Self::TwoDMultisampleArray => 7,
            Self::CubeArray => 8,
            Self::Unknown(value) => value,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TileMode {
    BlockLinear,
    Linear,
    Unknown(u16),
}

impl TileMode {
    pub const fn from_raw(raw: u16) -> Self {
        match raw {
            0 => Self::BlockLinear,
            1 => Self::Linear,
            value => Self::Unknown(value),
        }
    }

    pub const fn to_raw(self) -> u16 {
        match self {
            Self::BlockLinear => 0,
            Self::Linear => 1,
            Self::Unknown(value) => value,
        }
    }
}

#[bitsize(32, new = pub)]
#[derive(
    Clone, Copy, DebugBits, DefaultBits, DeserializeBits, Eq, FromBits, PartialEq, SerializeBits,
)]
pub struct TextureLayout {
    pub block_height_log2: u3,
    pub unknown: u29,
}

impl TextureLayout {
    pub fn from_raw(raw: u32) -> Self {
        Self::from(u32::new(raw))
    }

    pub fn to_raw(self) -> u32 {
        u32::from(self).value()
    }

    pub fn block_height_exponent(self) -> u8 {
        self.block_height_log2().value()
    }

    pub fn set_block_height_exponent(&mut self, exponent: u8) -> bool {
        if exponent > 7 {
            return false;
        }
        self.set_block_height_log2(u3::new(exponent));
        true
    }
}
