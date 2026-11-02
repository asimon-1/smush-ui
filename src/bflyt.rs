use std::fmt;

use serde::{Deserialize, Serialize};

pub const MAGIC: [u8; 4] = *b"FLYT";
pub type BflytHeader = crate::binary::FileHeader;

#[derive(Clone, Debug, PartialEq)]
pub enum BflytError {
    Invalid(&'static str),
}

impl fmt::Display for BflytError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => write!(formatter, "invalid BFLYT: {message}"),
        }
    }
}

impl std::error::Error for BflytError {}

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
    pub unknown_sections: Vec<UnknownBflytSection>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UnknownBflytSection {
    pub magic: String,
    pub data: Vec<u8>,
}

impl Bflyt {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BflytError> {
        if bytes.len() < 0x14 || bytes[..4] != MAGIC {
            return Err(BflytError::Invalid("missing FLYT signature or header"));
        }

        let little_endian = match &bytes[4..6] {
            [0xFE, 0xFF] => false,
            [0xFF, 0xFE] => true,
            _ => return Err(BflytError::Invalid("invalid byte-order mark")),
        };
        let reader = BflytReader {
            bytes,
            little_endian,
        };
        let header_size = reader.u16(6)? as usize;
        let version = reader.u32(8)?;
        let file_size = reader.u32(12)? as usize;
        let section_count = reader.u16(16)? as usize;
        if header_size < 0x14 || header_size > file_size || file_size > bytes.len() {
            return Err(BflytError::Invalid("header or file size is out of bounds"));
        }

        let mut sections = Vec::with_capacity(section_count);
        let mut position = header_size;
        for _ in 0..section_count {
            let magic: [u8; 4] = reader.slice(position, 4, file_size)?.try_into().unwrap();
            let size = reader.u32(position + 4)? as usize;
            if size < 8 {
                return Err(BflytError::Invalid("section is smaller than its header"));
            }
            let end = position
                .checked_add(size)
                .filter(|end| *end <= file_size)
                .ok_or(BflytError::Invalid("section is out of bounds"))?;
            sections.push(BflytSection {
                magic,
                start: position,
                end,
            });
            position = end;
        }
        if position != file_size {
            return Err(BflytError::Invalid("section count does not reach file end"));
        }

        let mut layout = Bflyt::default();
        layout.version = version;

        for section in &sections {
            match &section.magic {
                b"lyt1" => parse_layout(&reader, section, &mut layout.layout)?,
                b"txl1" => layout.textures = parse_resource_list(&reader, section)?,
                b"fnl1" => layout.fonts = parse_resource_list(&reader, section)?,
                b"mat1" => layout.materials = parse_materials(&reader, section)?,
                b"grp1" => layout.groups.push(parse_group(&reader, section)?),
                _ => {}
            }
        }

        let mut panes = Vec::<TemporaryPane>::new();
        let mut root_index = None;
        let mut pane_stack = Vec::<usize>::new();
        let mut current_pane = None;

        for section in &sections {
            match &section.magic {
                b"pan1" | b"bnd1" | b"pic1" | b"txt1" | b"wnd1" | b"prt1" => {
                    let pane = parse_pane(&reader, section, version, &layout)?;
                    let index = panes.len();
                    panes.push(TemporaryPane {
                        pane,
                        children: Vec::new(),
                    });
                    if let Some(parent) = pane_stack.last().copied() {
                        panes[parent].children.push(index);
                    } else if root_index.replace(index).is_some() {
                        return Err(BflytError::Invalid("multiple root panes"));
                    }
                    current_pane = Some(index);
                }
                b"pas1" => {
                    pane_stack.push(
                        current_pane.ok_or(BflytError::Invalid("pas1 has no preceding pane"))?,
                    );
                }
                b"pae1" => {
                    let parent = pane_stack
                        .pop()
                        .ok_or(BflytError::Invalid("pae1 has no matching pas1"))?;
                    current_pane = Some(parent);
                }
                b"usd1" => {
                    if let Some(index) = current_pane {
                        panes[index].pane.user_data_sections.push(
                            reader
                                .slice(
                                    section.start + 8,
                                    section.end - section.start - 8,
                                    section.end,
                                )?
                                .to_vec(),
                        );
                    } else {
                        layout.unknown_sections.push(raw_section(&reader, section)?);
                    }
                }
                b"lyt1" | b"txl1" | b"fnl1" | b"mat1" | b"grp1" | b"grs1" | b"gre1" => {}
                _ => layout.unknown_sections.push(raw_section(&reader, section)?),
            }
        }
        if !pane_stack.is_empty() {
            return Err(BflytError::Invalid("pane start has no matching pane end"));
        }
        if let Some(root_index) = root_index {
            layout.root = materialize_pane(root_index, &mut panes);
        }

        Ok(layout)
    }
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
    pub user_data_sections: Vec<Vec<u8>>,
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

#[derive(Clone, Copy)]
struct BflytSection {
    magic: [u8; 4],
    start: usize,
    end: usize,
}

struct BflytReader<'a> {
    bytes: &'a [u8],
    little_endian: bool,
}

impl BflytReader<'_> {
    fn slice(&self, start: usize, length: usize, limit: usize) -> Result<&[u8], BflytError> {
        let end = start
            .checked_add(length)
            .filter(|end| *end <= limit && limit <= self.bytes.len())
            .ok_or(BflytError::Invalid("field is out of bounds"))?;
        Ok(&self.bytes[start..end])
    }

    fn u8(&self, offset: usize) -> Result<u8, BflytError> {
        Ok(self.slice(offset, 1, self.bytes.len())?[0])
    }

    fn u16(&self, offset: usize) -> Result<u16, BflytError> {
        let bytes = self.slice(offset, 2, self.bytes.len())?;
        let value: [u8; 2] = bytes.try_into().unwrap();
        Ok(if self.little_endian {
            u16::from_le_bytes(value)
        } else {
            u16::from_be_bytes(value)
        })
    }

    fn u32(&self, offset: usize) -> Result<u32, BflytError> {
        let bytes = self.slice(offset, 4, self.bytes.len())?;
        let value: [u8; 4] = bytes.try_into().unwrap();
        Ok(if self.little_endian {
            u32::from_le_bytes(value)
        } else {
            u32::from_be_bytes(value)
        })
    }

    fn f32(&self, offset: usize) -> Result<f32, BflytError> {
        Ok(f32::from_bits(self.u32(offset)?))
    }

    fn fixed_string(
        &self,
        offset: usize,
        length: usize,
        limit: usize,
    ) -> Result<String, BflytError> {
        let bytes = self.slice(offset, length, limit)?;
        let end = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len());
        Ok(String::from_utf8_lossy(&bytes[..end]).into_owned())
    }

    fn c_string(&self, offset: usize, limit: usize) -> Result<String, BflytError> {
        if offset >= limit {
            return Err(BflytError::Invalid("string offset is out of bounds"));
        }
        self.fixed_string(offset, limit - offset, limit)
    }

    fn utf16_string(&self, offset: usize, limit: usize) -> Result<String, BflytError> {
        if offset >= limit {
            return Err(BflytError::Invalid("text offset is out of bounds"));
        }
        let available = self.slice(offset, limit - offset, limit)?;
        let mut units = Vec::new();
        for pair in available.chunks_exact(2) {
            let unit = if self.little_endian {
                u16::from_le_bytes([pair[0], pair[1]])
            } else {
                u16::from_be_bytes([pair[0], pair[1]])
            };
            if unit == 0 {
                break;
            }
            units.push(unit);
        }
        Ok(String::from_utf16_lossy(&units))
    }
}

struct TemporaryPane {
    pane: PaneNode,
    children: Vec<usize>,
}

fn parse_layout(
    reader: &BflytReader<'_>,
    section: &BflytSection,
    layout: &mut LayoutInfo,
) -> Result<(), BflytError> {
    if section.end - section.start < 0x1C {
        return Err(BflytError::Invalid("lyt1 section is too small"));
    }
    layout.is_drawn_from_middle = reader.u8(section.start + 8)? != 0;
    layout.width = reader.f32(section.start + 12)?;
    layout.height = reader.f32(section.start + 16)?;
    layout.max_parts_height = reader.f32(section.start + 20)?;
    layout.max_parts_width = reader.f32(section.start + 24)?;
    layout.name = reader.c_string(section.start + 28, section.end)?;
    Ok(())
}

fn parse_resource_list(
    reader: &BflytReader<'_>,
    section: &BflytSection,
) -> Result<Vec<String>, BflytError> {
    if section.end - section.start < 10 {
        return Err(BflytError::Invalid("resource list is too small"));
    }
    let count = reader.u16(section.start + 8)? as usize;
    let table_start = section.start + 12;
    let table_size = count
        .checked_mul(4)
        .ok_or(BflytError::Invalid("resource list count overflows"))?;
    reader.slice(table_start, table_size, section.end)?;
    (0..count)
        .map(|index| {
            let offset = reader.u32(table_start + index * 4)? as usize;
            let position = table_start
                .checked_add(offset)
                .filter(|position| *position < section.end)
                .ok_or(BflytError::Invalid("resource name offset is out of bounds"))?;
            reader.c_string(position, section.end)
        })
        .collect()
}

fn parse_materials(
    reader: &BflytReader<'_>,
    section: &BflytSection,
) -> Result<Vec<Material>, BflytError> {
    if section.end - section.start < 12 {
        return Err(BflytError::Invalid("mat1 section is too small"));
    }
    let count = reader.u16(section.start + 8)? as usize;
    let table_start = section.start + 12;
    let table_size = count
        .checked_mul(4)
        .ok_or(BflytError::Invalid("material count overflows"))?;
    reader.slice(table_start, table_size, section.end)?;
    (0..count)
        .map(|index| {
            let offset = reader.u32(table_start + index * 4)? as usize;
            let position = section
                .start
                .checked_add(offset)
                .filter(|position| *position < section.end)
                .ok_or(BflytError::Invalid("material offset is out of bounds"))?;
            Ok(Material {
                name: reader.fixed_string(position, 28, section.end)?,
                ..Material::default()
            })
        })
        .collect()
}

fn parse_group(reader: &BflytReader<'_>, section: &BflytSection) -> Result<PaneGroup, BflytError> {
    if section.end - section.start < 36 {
        return Err(BflytError::Invalid("grp1 section is too small"));
    }
    let name = reader.fixed_string(section.start + 8, 24, section.end)?;
    let count = reader.u16(section.start + 32)? as usize;
    let children_start = section.start + 36;
    let children_size = count
        .checked_mul(24)
        .ok_or(BflytError::Invalid("group child count overflows"))?;
    reader.slice(children_start, children_size, section.end)?;
    let panes = (0..count)
        .map(|index| reader.fixed_string(children_start + index * 24, 24, section.end))
        .collect::<Result<_, _>>()?;
    Ok(PaneGroup { name, panes })
}

fn parse_pane(
    reader: &BflytReader<'_>,
    section: &BflytSection,
    version: u32,
    layout: &Bflyt,
) -> Result<PaneNode, BflytError> {
    if section.end - section.start < 84 {
        return Err(BflytError::Invalid("pane section is too small"));
    }
    let start = section.start;
    let flags = reader.u8(start + 8)?;
    let origins = reader.u8(start + 9)?;
    let user_data = reader
        .slice(start + 36, 8, section.end)?
        .try_into()
        .unwrap();
    let mut pane = PaneNode {
        name: reader.fixed_string(start + 12, 24, section.end)?,
        visible: flags & 1 != 0,
        transmit_alpha: flags & 2 != 0,
        alpha: reader.u8(start + 10)?,
        part_scaling: reader.u8(start + 11)?,
        user_data,
        position: Vec3 {
            x: reader.f32(start + 44)?,
            y: reader.f32(start + 48)?,
            z: reader.f32(start + 52)?,
        },
        rotation: Vec3 {
            x: reader.f32(start + 56)?,
            y: reader.f32(start + 60)?,
            z: reader.f32(start + 64)?,
        },
        scale: Vec2 {
            x: reader.f32(start + 68)?,
            y: reader.f32(start + 72)?,
        },
        size: Vec2 {
            x: reader.f32(start + 76)?,
            y: reader.f32(start + 80)?,
        },
        origin: PaneAnchor {
            x: horizontal_origin(origins & 0x03),
            y: vertical_origin((origins >> 2) & 0x03),
        },
        parent_origin: PaneAnchor {
            x: horizontal_origin((origins >> 4) & 0x03),
            y: vertical_origin((origins >> 6) & 0x03),
        },
        ..PaneNode::default()
    };

    pane.kind = match &section.magic {
        b"pan1" => PaneKind::Container,
        b"bnd1" => PaneKind::Bounding,
        b"pic1" => {
            if section.end - start < 104 {
                return Err(BflytError::Invalid("pic1 section is too small"));
            }
            let material_index = reader.u16(start + 100)? as usize;
            let coordinate_count = reader.u8(start + 102)? as usize;
            let coordinate_bytes = coordinate_count
                .checked_mul(32)
                .ok_or(BflytError::Invalid("picture coordinate count overflows"))?;
            reader.slice(start + 104, coordinate_bytes, section.end)?;
            let texture_coordinates = (0..coordinate_count)
                .map(|index| {
                    let offset = start + 104 + index * 32;
                    Ok(TextureCoordinateSet {
                        top_left: Vec2 {
                            x: reader.f32(offset)?,
                            y: reader.f32(offset + 4)?,
                        },
                        top_right: Vec2 {
                            x: reader.f32(offset + 8)?,
                            y: reader.f32(offset + 12)?,
                        },
                        bottom_left: Vec2 {
                            x: reader.f32(offset + 16)?,
                            y: reader.f32(offset + 20)?,
                        },
                        bottom_right: Vec2 {
                            x: reader.f32(offset + 24)?,
                            y: reader.f32(offset + 28)?,
                        },
                    })
                })
                .collect::<Result<_, BflytError>>()?;
            PaneKind::Picture {
                material: layout
                    .materials
                    .get(material_index)
                    .map(|material| material.name.clone()),
                colors: [
                    read_color(reader, start + 84)?,
                    read_color(reader, start + 88)?,
                    read_color(reader, start + 92)?,
                    read_color(reader, start + 96)?,
                ],
                texture_coordinates,
            }
        }
        b"txt1" => {
            if section.end - start < 0xA4 {
                return Err(BflytError::Invalid("txt1 section is too small"));
            }
            let text_offset = reader.u32(start + 100)? as usize;
            let text = if text_offset == 0 {
                String::new()
            } else {
                let position = start
                    .checked_add(text_offset)
                    .filter(|position| *position < section.end)
                    .ok_or(BflytError::Invalid("text offset is out of bounds"))?;
                reader.utf16_string(position, section.end)?
            };
            let material_index = reader.u16(start + 88)? as usize;
            let font_index = reader.u16(start + 90)? as usize;
            let alignment = reader.u8(start + 93)?;
            PaneKind::Text {
                text,
                font: layout.fonts.get(font_index).cloned(),
                material: layout
                    .materials
                    .get(material_index)
                    .map(|material| material.name.clone()),
                font_size: Vec2 {
                    x: reader.f32(start + 112)?,
                    y: reader.f32(start + 116)?,
                },
                character_spacing: reader.f32(start + 120)?,
                line_spacing: reader.f32(start + 124)?,
                color_top: read_color(reader, start + 104)?,
                color_bottom: read_color(reader, start + 108)?,
                horizontal_alignment: horizontal_alignment(alignment & 0x03),
                vertical_alignment: vertical_alignment((alignment >> 2) & 0x03),
            }
        }
        b"wnd1" => {
            if section.end - start < 0x70 {
                return Err(BflytError::Invalid("wnd1 section is too small"));
            }
            let frame_count = reader.u8(start + 100)?;
            let content_offset = reader.u32(start + 104)? as usize;
            let material = if content_offset == 0 {
                None
            } else {
                let info = start
                    .checked_add(content_offset)
                    .filter(|position| position + 18 <= section.end)
                    .ok_or(BflytError::Invalid(
                        "window content offset is out of bounds",
                    ))?;
                let index = reader.u16(info + 16)? as usize;
                layout
                    .materials
                    .get(index)
                    .map(|material| material.name.clone())
            };
            PaneKind::Window {
                material,
                frame_count,
            }
        }
        b"prt1" => {
            if section.end - start < 96 {
                return Err(BflytError::Invalid("prt1 section is too small"));
            }
            let property_count = reader.u32(start + 84)? as usize;
            let property_size = if version >> 24 >= 8 { 40 } else { 36 };
            let layout_offset = start
                .checked_add(96)
                .and_then(|offset| offset.checked_add(property_count.checked_mul(property_size)?))
                .filter(|offset| *offset < section.end)
                .ok_or(BflytError::Invalid("part property table is out of bounds"))?;
            PaneKind::Parts {
                layout: reader.c_string(layout_offset, section.end)?,
            }
        }
        _ => return Err(BflytError::Invalid("unsupported pane section")),
    };
    Ok(pane)
}

fn read_color(reader: &BflytReader<'_>, offset: usize) -> Result<Color, BflytError> {
    let color = reader.slice(offset, 4, reader.bytes.len())?;
    Ok(Color {
        r: color[0],
        g: color[1],
        b: color[2],
        a: color[3],
    })
}

fn horizontal_origin(value: u8) -> HorizontalOrigin {
    match value {
        1 => HorizontalOrigin::Left,
        2 => HorizontalOrigin::Right,
        _ => HorizontalOrigin::Center,
    }
}

fn vertical_origin(value: u8) -> VerticalOrigin {
    match value {
        1 => VerticalOrigin::Top,
        2 => VerticalOrigin::Bottom,
        _ => VerticalOrigin::Center,
    }
}

fn horizontal_alignment(value: u8) -> HorizontalAlignment {
    match value {
        1 => HorizontalAlignment::Center,
        2 => HorizontalAlignment::Right,
        _ => HorizontalAlignment::Left,
    }
}

fn vertical_alignment(value: u8) -> VerticalAlignment {
    match value {
        1 => VerticalAlignment::Center,
        2 => VerticalAlignment::Bottom,
        _ => VerticalAlignment::Top,
    }
}

fn raw_section(
    reader: &BflytReader<'_>,
    section: &BflytSection,
) -> Result<UnknownBflytSection, BflytError> {
    Ok(UnknownBflytSection {
        magic: String::from_utf8_lossy(&section.magic).into_owned(),
        data: reader
            .slice(
                section.start + 8,
                section.end - section.start - 8,
                section.end,
            )?
            .to_vec(),
    })
}

fn materialize_pane(index: usize, panes: &mut [TemporaryPane]) -> PaneNode {
    let children = std::mem::take(&mut panes[index].children);
    let mut pane = std::mem::take(&mut panes[index].pane);
    pane.children = children
        .into_iter()
        .map(|child| materialize_pane(child, panes))
        .collect();
    pane
}
