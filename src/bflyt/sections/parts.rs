use binrw::binrw;
use serde::{Deserialize, Serialize};

use super::{PaneInfo, Vec2};

#[derive(Serialize, Deserialize)]
#[binrw]
pub struct PartsProperty {
    pub name: [u8; 24],
    pub usage_flags: u8,
    pub basic_usage_flags: u8,
    pub material_usage_flags: u8,
    pub system_ext_user_data_override_flags: u8,
    pub property_offset: u32,
    pub ext_user_data_offset: u32,
    pub pane_basic_info_offset: u32,
}

#[derive(Serialize, Deserialize)]
#[binrw]
#[brw(magic = b"prt1")]
pub struct Parts {
    #[serde(skip)]
    #[bw(map = |_: &u32| 0x60 + 40 * properties.len() as u32 + data.len() as u32)]
    size: u32,
    pub pane: PaneInfo,
    #[bw(map = |_: &u32| properties.len() as u32)]
    #[serde(skip)]
    property_count: u32,
    pub magnify: Vec2,
    #[br(count = property_count)]
    pub properties: Vec<PartsProperty>,
    /// Includes the part name, alignment bytes, and pointed-to subsection data.
    #[br(count = size - 0x60 - 40 * property_count)]
    pub data: Vec<u8>,
}

impl Parts { pub fn new(pane: PaneInfo, magnify: Vec2, properties: Vec<PartsProperty>, data: Vec<u8>) -> Self { Self { size: 0, pane, property_count: 0, magnify, properties, data } } }
