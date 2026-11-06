use binrw::binrw;
use serde::{Deserialize, Serialize};

use super::PaneInfo;

#[derive(Serialize, Deserialize)]
#[binrw]
#[brw(magic = b"bnd1")]
pub struct Bounding {
    #[serde(skip)]
    #[bw(map = |_: &u32| 0x54)]
    size: u32,
    pub pane: PaneInfo,
}

impl Bounding { pub fn new(pane: PaneInfo) -> Self { Self { size: 0, pane } } }
