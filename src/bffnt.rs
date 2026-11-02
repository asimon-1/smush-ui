use serde::{Deserialize, Serialize};

use crate::binary::ByteOrder;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bffnt {
    pub header: BffntHeader,
    pub info: FontInfo,
    pub glyph_sheet: GlyphSheet,
    pub widths: Vec<GlyphWidthRange>,
    pub character_maps: Vec<CharacterMap>,
    pub kerning: Vec<KerningPair>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BffntHeader {
    pub byte_order: ByteOrder,
    pub header_size: u16,
    pub version: u32,
    pub file_size: u32,
    pub section_count: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FontInfo {
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
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GlyphSheet {
    pub cell_width: u8,
    pub cell_height: u8,
    pub sheet_count: u8,
    pub sheet_size: u32,
    pub baseline_position: u16,
    pub texture_format: u16,
    pub cells_per_row: u16,
    pub cells_per_column: u16,
    pub texture_width: u16,
    pub texture_height: u16,
    pub texture_data: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GlyphWidthRange {
    pub first_glyph: u16,
    pub last_glyph: u16,
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
    pub mapping: CharacterMapping,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CharacterMapping {
    Sequential { base_index: u16 },
    Direct { glyph_indices: Vec<u16> },
    Sparse { entries: Vec<CharacterMapEntry> },
}

impl Default for CharacterMapping {
    fn default() -> Self {
        Self::Sequential { base_index: 0 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CharacterMapEntry {
    pub character_code: u16,
    pub glyph_index: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KerningPair {
    pub first_character: u16,
    pub second_character: u16,
    pub horizontal_offset: i16,
}
