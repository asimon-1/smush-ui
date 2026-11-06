use bilge::prelude::*;
use binrw::{BinRead, BinResult, BinWrite, Endian, Error, binrw, io::SeekFrom};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read, Seek, Write};

use crate::bflyt::Section;

use super::common::{align_up, fixed_string_to_string, string_to_fixed_string};
use super::{PaneInfo, User, Vec2, Vec3};

/// Which textbox values a parts property overrides.
#[bitsize(8, new = pub)]
#[derive(
    Clone, Copy, DebugBits, DefaultBits, DeserializeBits, Eq, FromBits, PartialEq, SerializeBits,
)]
pub struct TextboxUsageFlags {
    pub use_text: bool,
    pub use_position: bool,
    pub use_shadow: bool,
    /// Unspecified flag bits, retained when reading and writing.
    pub reserved: u5,
}

/// Which common pane values a parts property overrides.
#[bitsize(8, new = pub)]
#[derive(
    Clone, Copy, DebugBits, DefaultBits, DeserializeBits, Eq, FromBits, PartialEq, SerializeBits,
)]
pub struct PaneUsageFlags {
    pub set_visibility: bool,
    pub is_visible: bool,
    pub use_user_name: bool,
    pub use_position: bool,
    pub use_size: bool,
    pub use_scale: bool,
    pub use_rotation: bool,
    pub use_alpha: bool,
}

/// Which material-related pane values a parts property overrides.
#[bitsize(8, new = pub)]
#[derive(
    Clone, Copy, DebugBits, DefaultBits, DeserializeBits, Eq, FromBits, PartialEq, SerializeBits,
)]
pub struct PartsMaterialUsageFlags {
    pub use_color_blend: bool,
    pub use_texture: bool,
    pub override_capture_texture: bool,
    pub use_picture_procedural_shape: bool,
    /// Unspecified flag bits, retained when reading and writing.
    pub reserved: u4,
}

/// The compact common-pane override stored by a parts property.
#[derive(Serialize, Deserialize)]
#[binrw]
pub struct PartsPaneBasicInfo {
    #[br(try_map = fixed_string_to_string::<8>)]
    #[bw(try_map = string_to_fixed_string::<8>)]
    pub user_name: String,
    pub position: Vec3,
    pub rotation: Vec3,
    pub scale: Vec2,
    pub size: Vec2,
    pub alpha: u8,
    #[serde(skip)]
    #[brw(pad_after = 3)]
    padding: (),
}

impl PartsPaneBasicInfo {
    pub fn new(
        user_name: String,
        position: Vec3,
        rotation: Vec3,
        scale: Vec2,
        size: Vec2,
        alpha: u8,
    ) -> Self {
        Self {
            user_name,
            position,
            rotation,
            scale,
            size,
            alpha,
            padding: (),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct PartsProperty {
    pub name: String,
    pub usage_flags: TextboxUsageFlags,
    pub basic_usage_flags: PaneUsageFlags,
    pub material_usage_flags: PartsMaterialUsageFlags,
    pub user_data_type: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_override: Option<Box<Section>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_data_override: Option<User>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_user_data_sentinel: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_basic_info: Option<PartsPaneBasicInfo>,
}

#[derive(BinRead, BinWrite)]
struct RawPartsProperty {
    #[br(try_map = fixed_string_to_string::<24>)]
    #[bw(try_map = string_to_fixed_string::<24>)]
    name: String,
    #[br(map = |bits: u8| TextboxUsageFlags::from(u8::new(bits)))]
    #[bw(map = |flags: &TextboxUsageFlags| u8::from(*flags).value())]
    usage_flags: TextboxUsageFlags,
    #[br(map = |bits: u8| PaneUsageFlags::from(u8::new(bits)))]
    #[bw(map = |flags: &PaneUsageFlags| u8::from(*flags).value())]
    basic_usage_flags: PaneUsageFlags,
    #[br(map = |bits: u8| PartsMaterialUsageFlags::from(u8::new(bits)))]
    #[bw(map = |flags: &PartsMaterialUsageFlags| u8::from(*flags).value())]
    material_usage_flags: PartsMaterialUsageFlags,
    user_data_type: u8,
    pane_offset: u32,
    user_data_offset: u32,
    pane_basic_info_offset: u32,
}

#[derive(Serialize, Deserialize)]
pub struct Parts {
    pub pane: PaneInfo,
    pub magnify: Vec2,
    pub properties: Vec<PartsProperty>,
    pub data: PartsData,
}

#[derive(Serialize, Deserialize)]
pub struct PartsData {
    pub part_name: String,
}

fn offset_error(position: u64, offset: u32, size: u32, field: &str) -> Error {
    Error::AssertFail {
        pos: position,
        message: format!("parts {field} offset {offset:#x} is outside section size {size:#x}"),
    }
}

fn checked_offset(section_start: u64, size: u32, offset: u32, field: &str) -> BinResult<u64> {
    if offset < size {
        Ok(section_start + u64::from(offset))
    } else {
        Err(offset_error(section_start, offset, size, field))
    }
}

impl BinRead for Parts {
    type Args<'a> = ();

    fn read_options<R: Read + Seek>(
        reader: &mut R,
        endian: Endian,
        _: Self::Args<'_>,
    ) -> BinResult<Self> {
        let section_start = reader.stream_position()?;
        let mut magic = [0; 4];
        reader.read_exact(&mut magic)?;
        if &magic != b"prt1" {
            return Err(Error::AssertFail {
                pos: section_start,
                message: format!("expected prt1, found {:?}", String::from_utf8_lossy(&magic)),
            });
        }

        let size = u32::read_options(reader, endian, ())?;
        let pane = PaneInfo::read_options(reader, endian, ())?;
        let property_count = u32::read_options(reader, endian, ())?;
        let magnify = Vec2::read_options(reader, endian, ())?;
        let raw_properties = (0..property_count)
            .map(|_| RawPartsProperty::read_options(reader, endian, ()))
            .collect::<BinResult<Vec<_>>>()?;
        let part_name: String = binrw::NullString::read_options(reader, endian, ())?
            .try_into()
            .map_err(|error| Error::Custom {
                pos: reader.stream_position().unwrap_or(section_start),
                err: Box::new(error),
            })?;

        let mut properties = Vec::with_capacity(raw_properties.len());
        for raw in raw_properties {
            let pane_override = if raw.pane_offset == 0 {
                None
            } else {
                reader.seek(SeekFrom::Start(checked_offset(
                    section_start,
                    size,
                    raw.pane_offset,
                    "pane",
                )?))?;
                let section = Section::read_options(reader, endian, ())?;
                if !section.is_pane_section() {
                    return Err(Error::AssertFail {
                        pos: section_start + u64::from(raw.pane_offset),
                        message: "parts pane override is not a pane section".to_owned(),
                    });
                }
                Some(Box::new(section))
            };
            let user_data_override = if raw.user_data_offset <= 1 {
                None
            } else {
                reader.seek(SeekFrom::Start(checked_offset(
                    section_start,
                    size,
                    raw.user_data_offset,
                    "user-data",
                )?))?;
                Some(User::read_options(reader, endian, ())?)
            };
            let pane_basic_info = if raw.pane_basic_info_offset == 0 {
                None
            } else {
                reader.seek(SeekFrom::Start(checked_offset(
                    section_start,
                    size,
                    raw.pane_basic_info_offset,
                    "pane-basic-info",
                )?))?;
                Some(PartsPaneBasicInfo::read_options(reader, endian, ())?)
            };
            properties.push(PartsProperty {
                name: raw.name,
                usage_flags: raw.usage_flags,
                basic_usage_flags: raw.basic_usage_flags,
                material_usage_flags: raw.material_usage_flags,
                user_data_type: raw.user_data_type,
                pane_override,
                user_data_override,
                is_user_data_sentinel: raw.user_data_offset == 1,
                pane_basic_info,
            });
        }

        reader.seek(SeekFrom::Start(section_start + u64::from(size)))?;
        Ok(Self {
            pane,
            magnify,
            properties,
            data: PartsData { part_name },
        })
    }
}

impl BinWrite for Parts {
    type Args<'a> = ();

    fn write_options<W: Write + Seek>(
        &self,
        writer: &mut W,
        endian: Endian,
        _: Self::Args<'_>,
    ) -> BinResult<()> {
        let property_table_end = 0x60 + 40 * self.properties.len();
        let mut payload = Cursor::new(Vec::new());
        payload.write_all(self.data.part_name.as_bytes())?;
        payload.write_all(&[0])?;
        let name_end = align_up(payload.position() as usize, 4);
        payload.get_mut().resize(name_end, 0);
        payload.set_position(name_end as u64);

        let mut raw_properties = Vec::with_capacity(self.properties.len());
        for property in &self.properties {
            let pane_offset = if let Some(value) = &property.pane_override {
                if !value.is_pane_section() {
                    return Err(Error::AssertFail {
                        pos: writer.stream_position()?,
                        message: "parts pane override is not a pane section".to_owned(),
                    });
                }
                let offset = property_table_end + payload.position() as usize;
                value.write_options(&mut payload, endian, ())?;
                offset as u32
            } else {
                0
            };
            let user_data_offset = if let Some(value) = &property.user_data_override {
                let offset = property_table_end + payload.position() as usize;
                value.write_options(&mut payload, endian, ())?;
                offset as u32
            } else if property.is_user_data_sentinel {
                1
            } else {
                0
            };
            let pane_basic_info_offset = if let Some(value) = &property.pane_basic_info {
                let offset = property_table_end + payload.position() as usize;
                value.write_options(&mut payload, endian, ())?;
                offset as u32
            } else {
                0
            };
            raw_properties.push(RawPartsProperty {
                name: property.name.clone(),
                usage_flags: property.usage_flags,
                basic_usage_flags: property.basic_usage_flags,
                material_usage_flags: property.material_usage_flags,
                user_data_type: property.user_data_type,
                pane_offset,
                user_data_offset,
                pane_basic_info_offset,
            });
        }

        let size = property_table_end + payload.get_ref().len();
        writer.write_all(b"prt1")?;
        (size as u32).write_options(writer, endian, ())?;
        self.pane.write_options(writer, endian, ())?;
        (self.properties.len() as u32).write_options(writer, endian, ())?;
        self.magnify.write_options(writer, endian, ())?;
        for property in &raw_properties {
            property.write_options(writer, endian, ())?;
        }
        writer.write_all(payload.get_ref())?;
        Ok(())
    }
}

impl Parts {
    pub fn new(
        pane: PaneInfo,
        magnify: Vec2,
        properties: Vec<PartsProperty>,
        data: PartsData,
    ) -> Self {
        Self {
            pane,
            magnify,
            properties,
            data,
        }
    }
}
