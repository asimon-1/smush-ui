use serde::{Deserialize, Serialize};

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
    pub origin: LayoutOrigin,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutOrigin {
    #[default]
    Center,
    TopLeft,
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
    pub origin: PaneOrigin,
    pub parent_origin: PaneOrigin,
    pub transmit_alpha: bool,
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
pub enum PaneOrigin {
    #[default]
    Center,
    Left,
    Right,
    Top,
    Bottom,
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
