use binrw::binrw;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[binrw]
#[brw(magic = b"gre1")]
/// Empty marker which closes a group.
pub struct GroupEnd {
    #[serde(skip)]
    #[br(assert(size == 8))]
    #[bw(map = |_: &u32| 8)]
    pub size: u32,
}

impl GroupEnd {
    pub fn new() -> Self {
        Self { size: 0 }
    }
}
