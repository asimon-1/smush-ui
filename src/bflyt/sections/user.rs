use binrw::binrw;
use serde::{Deserialize, Serialize};

/// `usd1` user-data section, represented as its raw payload.
#[derive(Serialize, Deserialize)]
#[binrw]
#[brw(magic = b"usd1")]
pub struct User {
    #[serde(skip)]
    #[bw(map = |_: &u32| 8 + data.len() as u32)]
    size: u32,
    #[br(count = size - 8)]
    pub data: Vec<u8>,
}

impl User { pub fn new(data: Vec<u8>) -> Self { Self { size: 0, data } } }
