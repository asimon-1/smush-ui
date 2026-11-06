use binrw::binrw;
use serde::{Deserialize, Serialize};

use super::{Color, PaneInfo, Vec2};

#[derive(Serialize, Deserialize)]
#[binrw]
#[brw(magic = b"txt1")]
pub struct Textbox {
    #[serde(skip)]
    #[bw(map = |_: &u32| 0xA0 + data.len() as u32)]
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

impl Textbox {
    pub fn new(pane: PaneInfo) -> Self {
        Self {
            size: 0,
            pane,
            text_buffer_length: 0,
            text_restrict_length: 0,
            material_index: 0,
            font_index: 0,
            origin: 0,
            line_alignment: 0,
            active_shadows: 0,
            padding: 0,
            italic_tilt: 0.0,
            text_offset: 0,
            top_color: Color { red: 0, green: 0, blue: 0, alpha: 0 },
            bottom_color: Color { red: 0, green: 0, blue: 0, alpha: 0 },
            font_size: Vec2 { x: 0.0, y: 0.0 },
            character_size: 0.0,
            line_size: 0.0,
            textbox_name_offset: 0,
            shadow_offset: Vec2 { x: 0.0, y: 0.0 },
            shadow_scale: Vec2 { x: 0.0, y: 0.0 },
            shadow_top_color: Color { red: 0, green: 0, blue: 0, alpha: 0 },
            shadow_bottom_color: Color { red: 0, green: 0, blue: 0, alpha: 0 },
            shadow_italic_tilt: 0.0,
            data: Vec::new(),
        }
    }
}
