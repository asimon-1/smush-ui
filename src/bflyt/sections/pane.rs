use binrw::binrw;
use serde::{Deserialize, Serialize};

use super::PaneInfo;

#[derive(Serialize, Deserialize)]
#[binrw]
#[brw(magic = b"pan1")]
pub struct Pane {
    #[serde(skip)]
    #[bw(map = |_: &u32| 0x54)]
    size: u32,
    pub pane: PaneInfo,
}

impl Pane {
    pub fn new(pane: PaneInfo) -> Self {
        Self { size: 0, pane }
    }
}
