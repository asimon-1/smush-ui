use binrw::binrw;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[binrw]
#[brw(magic = b"grp1")]
pub struct Group {
    #[serde(skip)]
    #[bw(map = |_: &u32| 0x24 + 24 * children.len() as u32)]
    size: u32,
    /// NUL-terminated group name in a fixed 24-byte buffer.
    pub name: [u8; 24],
    #[bw(map = |_: &u16| children.len() as u16)]
    #[serde(skip)]
    child_count: u16,
    pub padding: [u8; 2],
    #[br(count = child_count)]
    /// NUL-terminated child names, each in a fixed 24-byte buffer.
    pub children: Vec<[u8; 24]>,
}

impl Group { pub fn new(name: [u8; 24], padding: [u8; 2], children: Vec<[u8; 24]>) -> Self { Self { size: 0, name, child_count: 0, padding, children } } }
