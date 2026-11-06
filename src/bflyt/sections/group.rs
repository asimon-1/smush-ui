use binrw::binrw;
use serde::{Deserialize, Serialize};

use super::common::{fixed_name_to_string, string_to_fixed_name};

#[derive(Serialize, Deserialize)]
#[binrw]
#[brw(magic = b"grp1")]
pub struct Group {
    #[serde(skip)]
    #[bw(map = |_: &u32| 0x24 + 24 * children.len() as u32)]
    size: u32,
    /// NUL-terminated group name in a fixed 24-byte buffer.
    #[br(try_map = fixed_name_to_string)]
    #[bw(try_map = string_to_fixed_name)]
    pub name: String,
    #[bw(map = |_: &u16| children.len() as u16)]
    #[serde(skip)]
    child_count: u16,
    pub padding: [u8; 2],
    #[br(count = child_count, try_map = |children: Vec<[u8; 24]>| children.into_iter().map(fixed_name_to_string).collect::<Result<Vec<_>, _>>())]
    #[bw(try_map = |children: &Vec<String>| children.iter().map(string_to_fixed_name).collect::<Result<Vec<_>, _>>())]
    /// NUL-terminated child names, each in a fixed 24-byte buffer.
    pub children: Vec<String>,
}

impl Group {
    pub fn new(name: String, padding: [u8; 2], children: Vec<String>) -> Self {
        Self {
            size: 0,
            name,
            child_count: 0,
            padding,
            children,
        }
    }
}
