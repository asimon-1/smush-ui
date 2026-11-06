use binrw::binrw;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[binrw]
#[brw(magic = b"mat1")]
pub struct Material {
    #[serde(skip)]
    #[bw(map = |_: &u32| 8 + data.len() as u32)]
    size: u32,
    #[br(count = size - 8)]
    pub data: Vec<u8>,
}

impl Material {
    pub fn new(data: Vec<u8>) -> Self { Self { size: 0, data } }
}
