use binrw::binrw;

use super::{MaterialInfo, PaneInfo};

#[binrw]
#[brw(magic = b"pic1")]
pub struct Picture {
    #[bw(calc = 0x68 + 32 * material.texture_coordinates.len() as u32)]
    pub size: u32,
    pub pane: PaneInfo,
    pub material: MaterialInfo,
}
