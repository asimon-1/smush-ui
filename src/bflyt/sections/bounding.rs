use binrw::binrw;

use super::PaneInfo;

#[binrw]
#[brw(magic = b"bnd1")]
pub struct Bounding {
    #[bw(calc = 0x54)]
    pub size: u32,
    pub pane: PaneInfo,
}
