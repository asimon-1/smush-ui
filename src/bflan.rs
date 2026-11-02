use serde::{Deserialize, Serialize};

use crate::binary::FileHeader;

pub const MAGIC: [u8; 4] = *b"FLAN";

pub mod wire {
    use binrw::{BinRead, BinWrite};

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BinRead, BinWrite)]
    pub struct Pat1DataPrefix {
        pub order: u16,
        pub group_count: u16,
        pub animation_name_offset: u32,
        pub group_names_offset: u32,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BinRead, BinWrite)]
    pub struct Pat1VersionExtension {
        pub unknown: u32,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BinRead, BinWrite)]
    pub struct Pat1FrameMetadata {
        pub start_frame: i16,
        pub end_frame: i16,
        pub child_binding: u8,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BinRead, BinWrite)]
    pub struct Pah1Share {
        pub source_pane: [u8; 25],
        pub target_group: [u8; 25],
        pub padding: [u8; 2],
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BinRead, BinWrite)]
    pub struct Pai1DataPrefix {
        pub frame_size: u16,
        pub looping: u8,
        pub padding: u8,
        pub texture_count: u16,
        pub entry_count: u16,
        pub entry_offset_table: u32,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BinRead, BinWrite)]
    pub struct PaiEntryHeader {
        pub name: [u8; 28],
        pub tag_count: u8,
        pub target: u8,
        pub padding: u16,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BinRead, BinWrite)]
    pub struct PaiTagHeader {
        pub tag: [u8; 4],
        pub entry_count: u8,
        pub padding: [u8; 3],
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BinRead, BinWrite)]
    pub struct AnimationCurveHeader {
        pub id: u8,
        pub target: u8,
        pub curve_type: u8,
        pub reserved: u8,
        pub key_count: u16,
        pub padding: u16,
        pub key_data_offset: u32,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, BinRead, BinWrite)]
    pub struct StepKeyframe {
        pub frame: f32,
        pub value: u16,
        pub padding: u16,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, BinRead, BinWrite)]
    pub struct HermiteKeyframe {
        pub frame: f32,
        pub value: f32,
        pub slope: f32,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bflan {
    pub header: FileHeader,
    pub tag: Option<Pat1>,
    pub share: Option<Pah1>,
    pub info: Option<Pai1>,
}

pub type BflanHeader = FileHeader;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pat1 {
    pub order: u16,
    pub name: String,
    pub start_frame: i16,
    pub end_frame: i16,
    pub child_binding: bool,
    pub groups: Vec<String>,
    pub unknown_data: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pah1 {
    pub shares: Vec<AnimationShare>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnimationShare {
    pub source_pane: String,
    pub target_group: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pai1 {
    pub frame_size: u16,
    pub looping: bool,
    pub textures: Vec<String>,
    pub entries: Vec<PaiEntry>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaiEntry {
    pub name: String,
    pub target: AnimationEntryTarget,
    pub padding: u16,
    pub tags: Vec<PaiTag>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "raw", rename_all = "snake_case")]
pub enum AnimationEntryTarget {
    #[default]
    Pane,
    Material,
    Unknown(u8),
}

impl From<u8> for AnimationEntryTarget {
    fn from(raw: u8) -> Self {
        match raw {
            0 => Self::Pane,
            1 => Self::Material,
            value => Self::Unknown(value),
        }
    }
}

impl From<AnimationEntryTarget> for u8 {
    fn from(target: AnimationEntryTarget) -> Self {
        match target {
            AnimationEntryTarget::Pane => 0,
            AnimationEntryTarget::Material => 1,
            AnimationEntryTarget::Unknown(value) => value,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaiTag {
    pub tag: String,
    pub unknown_prefix: Option<u32>,
    pub curves: Vec<AnimationCurve>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnimationCurve {
    pub id: u8,
    pub target: u8,
    pub curve_type: CurveType,
    pub reserved: u8,
    pub padding: u16,
    pub key_data_offset: u32,
    pub keyframes: Vec<Keyframe>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CurveType {
    Constant,
    #[default]
    Step,
    Hermite,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Keyframe {
    Step { frame: f32, value: u16 },
    Hermite { frame: f32, value: f32, slope: f32 },
}
