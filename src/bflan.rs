use serde::{Deserialize, Serialize};

use crate::bflim::ByteOrder;
use crate::bflyt::Color;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bflan {
    pub header: BflanHeader,
    pub animations: Vec<AnimationTag>,
    pub groups: Vec<AnimationGroup>,
    pub shares: Vec<AnimationShare>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BflanHeader {
    pub byte_order: ByteOrder,
    pub header_size: u16,
    pub version: u32,
    pub file_size: u32,
    pub section_count: u16,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnimationTag {
    pub name: String,
    pub start_frame: i16,
    pub end_frame: i16,
    pub order: u16,
    pub child_binding: bool,
    pub groups: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnimationGroup {
    pub name: String,
    pub tracks: Vec<AnimationTrack>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnimationTrack {
    pub pane_name: String,
    pub target: AnimationTarget,
    pub curve: CurveType,
    pub keyframes: Vec<Keyframe>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "property", content = "custom", rename_all = "snake_case")]
pub enum AnimationTarget {
    #[default]
    TranslationX,
    TranslationY,
    TranslationZ,
    RotationX,
    RotationY,
    RotationZ,
    ScaleX,
    ScaleY,
    Width,
    Height,
    Alpha,
    Visibility,
    Color,
    TextureIndex,
    Custom(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CurveType {
    Constant,
    #[default]
    Step,
    Hermite,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Keyframe {
    pub frame: f32,
    pub value: AnimationValue,
    pub incoming_slope: Option<f32>,
    pub outgoing_slope: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum AnimationValue {
    Float(f32),
    Integer(i32),
    Boolean(bool),
    Color(Color),
    Vector2 { x: f32, y: f32 },
}

impl Default for AnimationValue {
    fn default() -> Self {
        Self::Float(0.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnimationShare {
    pub source_pane: String,
    pub target_group: String,
}
