use binrw::binrw;

use super::{Color, PaneInfo, Vec2};

#[binrw]
#[brw(magic = b"txt1")]
pub struct Textbox {
    #[bw(calc = 0xA0 + data.len() as u32)]
    pub size: u32,
    pub pane: PaneInfo,
    pub text_buffer_length: u16,
    pub text_restrict_length: u16,
    pub material_index: u16,
    pub font_index: u16,
    pub origin: u8,
    pub line_alignment: u8,
    pub active_shadows: u8,
    pub padding: u8,
    pub italic_tilt: f32,
    pub text_offset: u32,
    pub top_color: Color,
    pub bottom_color: Color,
    pub font_size: Vec2,
    pub character_size: f32,
    pub line_size: f32,
    pub textbox_name_offset: u32,
    pub shadow_offset: Vec2,
    pub shadow_scale: Vec2,
    pub shadow_top_color: Color,
    pub shadow_bottom_color: Color,
    pub shadow_italic_tilt: f32,
    /// Variable text, name, and optional transform data beginning at 0xA0.
    #[br(count = size - 0xA0)]
    pub data: Vec<u8>,
}
