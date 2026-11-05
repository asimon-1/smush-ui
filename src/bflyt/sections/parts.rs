use binrw::binrw;

use super::{PaneInfo, Vec2};

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

#[binrw]
#[brw(magic = b"prt1")]
pub struct Parts {
    #[bw(calc = 0x60 + 40 * properties.len() as u32 + data.len() as u32)]
    pub size: u32,
    pub pane: PaneInfo,
    #[bw(calc = properties.len() as u32)]
    pub property_count: u32,
    pub magnify: Vec2,
    #[br(count = property_count)]
    pub properties: Vec<PartsProperty>,
    /// Includes the part name, alignment bytes, and pointed-to subsection data.
    #[br(count = size - 0x60 - 40 * property_count)]
    pub data: Vec<u8>,
}
