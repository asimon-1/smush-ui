use binrw::binrw;

use super::PaneInfo;

#[binrw]
#[brw(magic = b"wnd1")]
pub struct Window {
    #[bw(calc = 0x70 + data.len() as u32)]
    pub size: u32,
    pub pane: PaneInfo,
    pub rectangle: [f32; 4],
    pub frame_data_count: u8,
    pub flags: u8,
    pub padding: [u8; 2],
    /// Section-relative offset to material info.
    pub material_info_offset: u32,
    /// Section-relative offset with an unknown target.
    pub unknown_offset: u32,
    #[br(count = size - 0x70)]
    pub data: Vec<u8>,
}
