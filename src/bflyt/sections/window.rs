use binrw::binrw;
use serde::{Deserialize, Serialize};

use super::PaneInfo;

#[derive(Serialize, Deserialize)]
#[binrw]
#[brw(magic = b"wnd1")]
pub struct Window {
    #[serde(skip)]
    #[bw(map = |_: &u32| 0x70 + data.len() as u32)]
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

impl Window {
    pub fn new(pane: PaneInfo) -> Self {
        Self {
            size: 0,
            pane,
            rectangle: [0.0; 4],
            frame_data_count: 0,
            flags: 0,
            padding: [0; 2],
            material_info_offset: 0,
            unknown_offset: 0,
            data: Vec::new(),
        }
    }
}
