use serde::{Deserialize, Serialize};

use crate::binary::FileHeader;

pub const MAGIC: [u8; 4] = *b"FFNT";
pub type BffntHeader = FileHeader;

pub mod wire {
    use binrw::{BinRead, BinWrite};

    use crate::binary::SectionHeader;

    #[derive(Clone, Copy, Debug, Default, PartialEq, BinRead, BinWrite)]
    pub struct FinfSection {
        pub header: SectionHeader,
        pub data: FinfData,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, BinRead, BinWrite)]
    pub struct FinfData {
        pub font_type: u8,
        pub height: u8,
        pub width: u8,
        pub ascent: u8,
        pub line_feed: u16,
        pub invalid_character_index: u16,
        pub default_left_distance: u8,
        pub default_cell_width: u8,
        pub default_character_width: u8,
        pub character_code: u8,
        pub glyph_sheet_offset: u32,
        pub width_section_offset: u32,
        pub character_map_offset: u32,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, BinRead, BinWrite)]
    pub struct TglpSectionPrefix {
        pub header: SectionHeader,
        pub cell_width: u8,
        pub cell_height: u8,
        pub sheet_count: u8,
        pub max_character_width: u8,
        pub sheet_size: u32,
        pub baseline_position: u16,
        pub texture_format: u16,
        pub cells_per_row: u16,
        pub cells_per_column: u16,
        pub texture_width: u16,
        pub texture_height: u16,
        pub texture_data_offset: u32,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BinRead, BinWrite)]
    pub struct CwdhData {
        pub first_glyph: u16,
        pub last_glyph: u16,
        pub next_section_offset: u32,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BinRead, BinWrite)]
    pub struct CwdhSection {
        pub header: SectionHeader,
        pub data: CwdhData,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BinRead, BinWrite)]
    pub struct CmapData {
        pub first_code: u16,
        pub last_code: u16,
        pub mapping_type: u16,
        pub padding: u16,
        pub next_section_offset: u32,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BinRead, BinWrite)]
    pub struct CmapSection {
        pub header: SectionHeader,
        pub data: CmapData,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bffnt {
    pub header: FileHeader,
    pub info: FontInfo,
    pub glyph_sheet: GlyphSheet,
    pub widths: Vec<GlyphWidthRange>,
    pub character_maps: Vec<CharacterMap>,
    pub kerning_data: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FontInfo {
    pub section_size: u32,
    pub font_type: u8,
    pub height: u8,
    pub width: u8,
    pub ascent: u8,
    pub line_feed: u16,
    pub invalid_character_index: u16,
    pub default_left_distance: u8,
    pub default_cell_width: u8,
    pub default_character_width: u8,
    pub character_code: u8,
    pub glyph_sheet_offset: u32,
    pub width_section_offset: u32,
    pub character_map_offset: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GlyphSheet {
    pub cell_width: u8,
    pub cell_height: u8,
    pub sheet_count: u8,
    pub max_character_width: u8,
    pub sheet_size: u32,
    pub baseline_position: u16,
    pub texture_format: u16,
    pub cells_per_row: u16,
    pub cells_per_column: u16,
    pub texture_width: u16,
    pub texture_height: u16,
    pub texture_data_offset: u32,
    pub texture_data: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GlyphWidthRange {
    pub first_glyph: u16,
    pub last_glyph: u16,
    pub next_section_offset: u32,
    pub entries: Vec<GlyphWidth>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GlyphWidth {
    pub left_distance: u8,
    pub cell_width: u8,
    pub character_width: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CharacterMap {
    pub first_code: u16,
    pub last_code: u16,
    pub padding: u16,
    pub next_section_offset: u32,
    pub mapping: CharacterMapping,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CharacterMapping {
    Direct { base_index: u16 },
    Table { glyph_indices: Vec<u16> },
    Scan { entries: Vec<CharacterMapEntry> },
}

impl Default for CharacterMapping {
    fn default() -> Self {
        Self::Direct { base_index: 0 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CharacterMapEntry {
    pub character_code: u16,
    pub glyph_index: u16,
}
