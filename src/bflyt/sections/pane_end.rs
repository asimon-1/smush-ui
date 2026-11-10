use binrw::binrw;
use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
#[binrw]
#[brw(magic = b"pae1")]
/// Empty marker that ends the children of a pane.
pub struct PaneEnd {
    #[serde(skip)]
    #[br(assert(size == 8))]
    #[bw(map = |_: &u32| 8)]
    pub size: u32,
}

impl PaneEnd {
    pub fn new() -> Self {
        Self { size: 0 }
    }
}
