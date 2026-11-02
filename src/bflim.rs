use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bflim {
    pub header: BflimHeader,
    pub image: BflimImageInfo,
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BflimHeader {
    pub byte_order: ByteOrder,
    pub header_size: u16,
    pub version: u32,
    pub file_size: u32,
    pub block_count: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ByteOrder {
    #[default]
    BigEndian,
    LittleEndian,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BflimImageInfo {
    pub width: u16,
    pub height: u16,
    pub alignment: u16,
    pub format: u8,
    pub tile_mode: u8,
    pub swizzle: u16,
    pub image_size: u32,
}
