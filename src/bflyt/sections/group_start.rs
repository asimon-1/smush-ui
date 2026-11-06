use binrw::binrw;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[binrw]
#[brw(magic = b"grs1")]
/// Empty marker which follows a `grp1` section.
pub struct GroupStart {
    #[serde(skip)]
    #[br(assert(size == 8))]
    #[bw(map = |_: &u32| 8)]
    pub size: u32,
}

impl GroupStart {
    pub fn new() -> Self {
        Self { size: 0 }
    }
}
