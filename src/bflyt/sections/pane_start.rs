use binrw::binrw;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[binrw]
#[brw(magic = b"pas1")]
/// Empty marker that begins the children of a pane.
pub struct PaneStart {
    #[serde(skip)]
    #[br(assert(size == 8))]
    #[bw(map = |_: &u32| 8)]
    pub size: u32,
}

impl PaneStart {
    pub fn new() -> Self {
        Self { size: 0 }
    }
}
