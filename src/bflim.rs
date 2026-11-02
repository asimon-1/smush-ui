use serde::{Deserialize, Serialize};

use binrw::{BinRead, BinWrite};

use crate::binary::FileHeader;

pub const MAGIC: [u8; 4] = *b"FLIM";
pub const IMAGE_SECTION_MAGIC: [u8; 4] = *b"imag";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bflim {
    pub image_data: Vec<u8>,
    pub footer: BflimFooter,
}

#[derive(Clone, Debug, Default, PartialEq, BinRead, BinWrite, Serialize, Deserialize)]
#[serde(default)]
pub struct BflimFooter {
    pub header: FileHeader,
    pub image: BflimImageInfo,
    pub image_data_size: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BinRead, BinWrite, Serialize, Deserialize)]
#[serde(default)]
pub struct BflimImageInfo {
    pub magic: [u8; 4],
    pub section_size: u32,
    pub width: u16,
    pub height: u16,
    pub alignment: u16,
    pub format: u8,
    pub tile_mode_and_swizzle: u8,
}

impl Default for BflimImageInfo {
    fn default() -> Self {
        Self {
            magic: IMAGE_SECTION_MAGIC,
            section_size: 0x10,
            width: 0,
            height: 0,
            alignment: 0,
            format: 0,
            tile_mode_and_swizzle: 0,
        }
    }
}

impl BflimImageInfo {
    pub fn tile_mode(&self) -> u8 {
        self.tile_mode_and_swizzle & 0x1F
    }

    pub fn swizzle(&self) -> u8 {
        self.tile_mode_and_swizzle >> 5
    }
}
