use serde::{Deserialize, Serialize};

pub const MAGIC: [u8; 4] = *b"FLYT";
pub type BflytHeader = crate::binary::FileHeader;

pub mod wire {
    use binrw::{BinRead, BinWrite};

    #[derive(Clone, Copy, Debug, Default, PartialEq, BinRead, BinWrite)]
    pub struct LayoutSectionPrefix {
        pub is_drawn_from_middle: u8,
        pub padding: [u8; 3],
        pub width: f32,
        pub height: f32,
        pub max_parts_height: f32,
        pub max_parts_width: f32,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, BinRead, BinWrite)]
    pub struct PaneData {
        pub flags: u8,
        pub origins: u8,
        pub alpha: u8,
        pub part_scaling: u8,
        pub name: [u8; 24],
        pub user_data: [u8; 8],
        pub position: [f32; 3],
        pub rotation: [f32; 3],
        pub scale: [f32; 2],
        pub size: [f32; 2],
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, BinRead, BinWrite)]
    pub struct MaterialInfoPrefix {
        pub corner_colors: [u8; 16],
        pub material_index: u16,
        pub texture_coordinate_set_count: u16,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bflyt {
    pub version: u32,
    pub layout: LayoutInfo,
    pub textures: Vec<String>,
    pub fonts: Vec<String>,
    pub materials: Vec<Material>,
    pub groups: Vec<PaneGroup>,
    pub root: PaneNode,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutInfo {
    pub width: f32,
    pub height: f32,
    pub is_drawn_from_middle: bool,
    pub max_parts_height: f32,
    pub max_parts_width: f32,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaneNode {
    pub name: String,
    pub kind: PaneKind,
    pub visible: bool,
    pub alpha: u8,
    pub position: Vec3,
    pub rotation: Vec3,
    pub scale: Vec2,
    pub size: Vec2,
    pub origin: PaneAnchor,
    pub parent_origin: PaneAnchor,
    pub part_scaling: u8,
    pub transmit_alpha: bool,
    pub user_data: [u8; 8],
    pub children: Vec<PaneNode>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PaneKind {
    #[default]
    Container,
    Bounding,
    Picture {
        material: Option<String>,
        #[serde(default)]
        colors: [Color; 4],
        #[serde(default)]
        texture_coordinates: Vec<TextureCoordinateSet>,
    },
    Text {
        text: String,
        font: Option<String>,
        material: Option<String>,
        font_size: Vec2,
        character_spacing: f32,
        line_spacing: f32,
        color_top: Color,
        color_bottom: Color,
        horizontal_alignment: HorizontalAlignment,
        vertical_alignment: VerticalAlignment,
    },
    Window {
        material: Option<String>,
        frame_count: u8,
    },
    Parts {
        layout: String,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HorizontalOrigin {
    #[default]
    Center,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerticalOrigin {
    #[default]
    Center,
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaneAnchor {
    pub x: HorizontalOrigin,
    pub y: VerticalOrigin,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextureCoordinateSet {
    pub top_left: Vec2,
    pub top_right: Vec2,
    pub bottom_left: Vec2,
    pub bottom_right: Vec2,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Material {
    pub name: String,
    pub textures: Vec<TextureBinding>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextureBinding {
    pub texture: String,
    pub coordinate_set: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaneGroup {
    pub name: String,
    pub panes: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HorizontalAlignment {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerticalAlignment {
    #[default]
    Top,
    Center,
    Bottom,
}
