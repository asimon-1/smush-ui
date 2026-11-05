use binrw::binrw;

use super::PaneInfo;

#[binrw]
#[brw(magic = b"pan1")]
pub struct Pane {
    #[bw(calc = 0x54)]
    pub size: u32,
    pub pane: PaneInfo,
}
