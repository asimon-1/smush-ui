use binrw::binrw;
use serde::{Deserialize, Serialize};

/// `cnt1` control-data section, represented as its raw payload.
#[derive(Serialize, Deserialize)]
#[binrw]
#[brw(magic = b"cnt1")]
pub struct Control {
    #[serde(skip)]
    #[bw(map = |_: &u32| 8 + data.len() as u32)]
    size: u32,
    #[br(count = size - 8)]
    pub data: Vec<u8>,
}

impl Control {
    pub fn new(data: Vec<u8>) -> Self {
        Self { size: 0, data }
    }
}
