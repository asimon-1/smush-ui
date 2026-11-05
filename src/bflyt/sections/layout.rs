use binrw::{NullString, binrw};

#[binrw]
#[brw(magic = b"lyt1")]
pub struct Layout {
    /// Total section size, including the 8-byte section header.
    #[bw(calc = 0x1C + name.0.len() as u32 + 1)]
    pub size: u32,
    /// Nonzero when the layout is drawn from its middle.
    pub is_drawn_from_middle: u8,
    padding: [u8; 3],
    pub width: f32,
    pub height: f32,
    pub max_parts_height: f32,
    pub max_parts_width: f32,
    pub name: NullString,
}
