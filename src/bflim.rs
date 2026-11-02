use serde::{Deserialize, Serialize};

use crate::binary::FileHeader;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bflim {
    pub image_data: Vec<u8>,
    pub footer: BflimFooter,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BflimFooter {
    pub header: FileHeader,
    pub image: BflimImageInfo,
    pub image_data_size: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BflimImageInfo {
    pub section_size: u32,
    pub width: u16,
    pub height: u16,
    pub alignment: u16,
    pub format: u8,
    pub tile_mode_and_swizzle: u8,
}

impl BflimImageInfo {
    pub fn tile_mode(&self) -> u8 {
        self.tile_mode_and_swizzle & 0x1F
    }

    pub fn swizzle(&self) -> u8 {
        self.tile_mode_and_swizzle >> 5
    }
}
