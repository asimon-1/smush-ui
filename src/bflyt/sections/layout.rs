use binrw::{NullString, binrw};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[binrw]
#[brw(magic = b"lyt1")]
pub struct Layout {
    /// Total section size, including the 8-byte section header.
    #[serde(skip)]
    #[bw(map = |_: &u32| 0x1C + name.len() as u32 + 1)]
    pub size: u32,
    /// Whether the layout is drawn from its middle rather than its origin.
    #[br(map = |value: u8| value != 0)]
    #[bw(map = |value: &bool| u8::from(*value))]
    pub is_drawn_from_middle: bool,
    #[serde(skip)]
    padding: [u8; 3],
    pub width: f32,
    pub height: f32,
    pub max_parts_height: f32,
    pub max_parts_width: f32,
    #[br(try_map = |name: NullString| String::try_from(name))]
    #[bw(map = |name: &String| NullString::from(name.as_str()))]
    pub name: String,
}
