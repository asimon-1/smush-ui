use binrw::binrw;
use serde::{Deserialize, Serialize};

use super::common::{fixed_string_to_string, string_to_fixed_string};

#[derive(Serialize, Deserialize)]
#[binrw]
#[brw(magic = b"grp1")]
pub struct Group {
    #[serde(skip)]
    #[bw(map = |_: &u32| 0x24 + 24 * members.len() as u32)]
    size: u32,
    /// NUL-terminated group name in a fixed 24-byte buffer.
    #[br(try_map = fixed_string_to_string::<24>)]
    #[bw(try_map = string_to_fixed_string::<24>)]
    pub name: String,
    #[bw(map = |_: &u32| members.len() as u32)]
    #[serde(skip)]
    child_count: u32,
    #[br(count = child_count, try_map = |members: Vec<[u8; 24]>| members.into_iter().map(fixed_string_to_string::<24>).collect::<Result<Vec<_>, _>>())]
    #[bw(try_map = |members: &Vec<String>| members.iter().map(string_to_fixed_string::<24>).collect::<Result<Vec<_>, _>>())]
    /// Pane names belonging to this group; distinct from nested subgroup `children` in JSON.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<String>,
}

impl Group {
    pub fn new(name: String, members: Vec<String>) -> Self {
        Self {
            size: 0,
            name,
            child_count: 0,
            members,
        }
    }
}
