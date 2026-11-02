use std::{fmt, io::Cursor};

use binrw::{BinRead, BinWrite, Endian};
use serde::{Deserialize, Serialize};

use crate::binary::{ByteOrder, FileHeader, SectionHeader};

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
    pub byte_order: ByteOrder,
    pub tag: Option<Pat1>,
    pub share: Option<Pah1>,
    pub info: Option<Pai1>,
    pub unknown_sections: Vec<UnknownBflanSection>,
    pub trailing_data: Vec<u8>,
}

impl Bflan {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BflanError> {
        if bytes.len() < 8 || bytes[..4] != MAGIC {
            return Err(BflanError::Invalid("missing FLAN signature"));
        }

        let byte_order = match &bytes[4..6] {
            [0xFE, 0xFF] => ByteOrder::BigEndian,
            [0xFF, 0xFE] => ByteOrder::LittleEndian,
            _ => return Err(BflanError::Invalid("invalid FLAN byte-order mark")),
        };
        let endian = bflan_endian(byte_order);
        let mut cursor = Cursor::new(bytes);
        let header = FileHeader::read_with_endian(&mut cursor, endian)?;

        if !header.has_magic(&MAGIC) || header.byte_order_mark != 0xFEFF {
            return Err(BflanError::Invalid("invalid FLAN header"));
        }
        let file_size = usize::try_from(header.file_size)
            .map_err(|_| BflanError::Invalid("FLAN size does not fit memory"))?;
        let header_size = usize::from(header.header_size);
        if header_size < 0x14 || header_size > file_size || file_size > bytes.len() {
            return Err(BflanError::Invalid(
                "FLAN header or file size is out of bounds",
            ));
        }

        let major_version = (header.version >> 24) as u8;
        let mut result = Self {
            header,
            byte_order,
            ..Self::default()
        };
        cursor.set_position(header_size as u64);

        for _ in 0..result.header.section_count {
            let section_start = usize::try_from(cursor.position())
                .map_err(|_| BflanError::Invalid("FLAN section offset does not fit memory"))?;
            let section_header = SectionHeader::read_options(&mut cursor, endian, ())?;
            let section_size = usize::try_from(section_header.size)
                .map_err(|_| BflanError::Invalid("FLAN section size does not fit memory"))?;
            if section_size < 8 {
                return Err(BflanError::Invalid(
                    "FLAN section is smaller than its header",
                ));
            }
            let section_end = section_start
                .checked_add(section_size)
                .filter(|end| *end <= file_size)
                .ok_or(BflanError::Invalid("FLAN section is out of bounds"))?;
            let section = &bytes[section_start..section_end];

            match &section_header.magic {
                b"pat1" => {
                    if result
                        .tag
                        .replace(parse_pat1(section, endian, major_version)?)
                        .is_some()
                    {
                        return Err(BflanError::Invalid("duplicate PAT1 section"));
                    }
                }
                b"pah1" => {
                    if result.share.replace(parse_pah1(section, endian)?).is_some() {
                        return Err(BflanError::Invalid("duplicate PAH1 section"));
                    }
                }
                b"pai1" => {
                    if result.info.replace(parse_pai1(section, endian)?).is_some() {
                        return Err(BflanError::Invalid("duplicate PAI1 section"));
                    }
                }
                _ => result.unknown_sections.push(UnknownBflanSection {
                    magic: section_header.magic,
                    data: section[8..].to_vec(),
                }),
            }
            cursor.set_position(section_end as u64);
        }

        let sections_end = usize::try_from(cursor.position())
            .map_err(|_| BflanError::Invalid("FLAN end position does not fit memory"))?;
        result.trailing_data = bytes[sections_end..file_size].to_vec();
        Ok(result)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, BflanError> {
        let endian = bflan_endian(self.byte_order);
        let major_version = (self.header.version >> 24) as u8;
        let mut sections = Vec::new();
        if let Some(tag) = &self.tag {
            sections.push(encode_section(
                *b"pat1",
                &encode_pat1(tag, major_version, endian)?,
                endian,
            )?);
        }
        if let Some(share) = &self.share {
            sections.push(encode_section(
                *b"pah1",
                &encode_pah1(share, endian)?,
                endian,
            )?);
        }
        if let Some(info) = &self.info {
            sections.push(encode_section(
                *b"pai1",
                &encode_pai1(info, endian)?,
                endian,
            )?);
        }
        for section in &self.unknown_sections {
            sections.push(encode_section(section.magic, &section.data, endian)?);
        }

        let section_count = u16::try_from(sections.len())
            .map_err(|_| BflanError::Invalid("too many FLAN sections"))?;
        let mut body = Vec::new();
        for section in sections {
            body.extend_from_slice(&section);
        }
        body.extend_from_slice(&self.trailing_data);
        let file_size = 0x14usize
            .checked_add(body.len())
            .ok_or(BflanError::Invalid("FLAN file size overflow"))?;
        let header = FileHeader {
            magic: MAGIC,
            byte_order_mark: 0xFEFF,
            header_size: 0x14,
            version: self.header.version,
            file_size: u32::try_from(file_size)
                .map_err(|_| BflanError::Invalid("FLAN file exceeds u32"))?,
            section_count,
            padding: self.header.padding,
        };
        let mut output = encode_record(&header, endian)?;
        output.extend_from_slice(&body);
        Ok(output)
    }
}

#[derive(Debug)]
pub enum BflanError {
    Binary(binrw::Error),
    Invalid(&'static str),
    OutOfBounds {
        offset: usize,
        size: usize,
        data_len: usize,
    },
    CurveDataOutOfBounds {
        curve_offset: usize,
        key_data_offset: u32,
        curve_type: u8,
        key_count: u16,
        data_len: usize,
    },
    UnsupportedCurveType(u8),
}

impl fmt::Display for BflanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Binary(error) => write!(formatter, "BFLAN binary error: {error}"),
            Self::Invalid(message) => write!(formatter, "invalid BFLAN: {message}"),
            Self::OutOfBounds {
                offset,
                size,
                data_len,
            } => write!(
                formatter,
                "BFLAN range {offset:#x}..{:#x} exceeds section length {data_len:#x}",
                offset.saturating_add(*size)
            ),
            Self::CurveDataOutOfBounds {
                curve_offset,
                key_data_offset,
                curve_type,
                key_count,
                data_len,
            } => write!(
                formatter,
                "BFLAN curve at {curve_offset:#x} (type {curve_type}, {key_count} keys, data +{key_data_offset:#x}) exceeds section length {data_len:#x}"
            ),
            Self::UnsupportedCurveType(curve_type) => {
                write!(formatter, "unsupported BFLAN curve type: {curve_type}")
            }
        }
    }
}

impl std::error::Error for BflanError {}

impl From<binrw::Error> for BflanError {
    fn from(error: binrw::Error) -> Self {
        Self::Binary(error)
    }
}

fn bflan_endian(byte_order: ByteOrder) -> Endian {
    match byte_order {
        ByteOrder::BigEndian => Endian::Big,
        ByteOrder::LittleEndian => Endian::Little,
    }
}

fn encode_record<T: for<'a> BinWrite<Args<'a> = ()>>(
    record: &T,
    endian: Endian,
) -> Result<Vec<u8>, BflanError> {
    let mut cursor = Cursor::new(Vec::new());
    record.write_options(&mut cursor, endian, ())?;
    Ok(cursor.into_inner())
}

fn encode_section(magic: [u8; 4], payload: &[u8], endian: Endian) -> Result<Vec<u8>, BflanError> {
    let section_size = 8usize
        .checked_add(payload.len())
        .ok_or(BflanError::Invalid("FLAN section size overflow"))?;
    let header = SectionHeader {
        magic,
        size: u32::try_from(section_size)
            .map_err(|_| BflanError::Invalid("FLAN section exceeds u32"))?,
    };
    let mut section = encode_record(&header, endian)?;
    section.extend_from_slice(payload);
    Ok(section)
}

fn encode_pat1(tag: &Pat1, major_version: u8, endian: Endian) -> Result<Vec<u8>, BflanError> {
    let group_count =
        u16::try_from(tag.groups.len()).map_err(|_| BflanError::Invalid("too many PAT1 groups"))?;
    let mut section = encode_record(
        &wire::Pat1DataPrefix {
            order: tag.order,
            group_count,
            animation_name_offset: 0,
            group_names_offset: 0,
        },
        endian,
    )?;
    if major_version >= 8 {
        append_u32(&mut section, tag.version_extension.unwrap_or(0), endian);
    }
    section.extend_from_slice(&encode_record(
        &wire::Pat1FrameMetadata {
            start_frame: tag.start_frame,
            end_frame: tag.end_frame,
            child_binding: tag.flags | u8::from(tag.child_binding),
        },
        endian,
    )?);
    section.extend_from_slice(&tag.pre_name_data);

    let name_offset = 8usize
        .checked_add(section.len())
        .ok_or(BflanError::Invalid("PAT1 name offset overflow"))?;
    section.extend_from_slice(tag.name.as_bytes());
    section.push(0);
    align_vec(&mut section, 4);
    let group_names_offset = 8usize
        .checked_add(section.len())
        .ok_or(BflanError::Invalid("PAT1 group offset overflow"))?;

    let (name_width, record_size) = if major_version >= 8 {
        (28, 28)
    } else if major_version < 5 {
        (24, 28)
    } else {
        (33, 36)
    };
    for (index, group) in tag.groups.iter().enumerate() {
        append_fixed_string(&mut section, group, name_width)?;
        if major_version < 8 {
            section.push(tag.group_flags.get(index).copied().unwrap_or(0));
            let padding_size = record_size - name_width - 1;
            let padding = tag
                .group_padding
                .get(index)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            section.extend_from_slice(&padding[..padding.len().min(padding_size)]);
            section.resize(
                section.len() + padding_size.saturating_sub(padding.len()),
                0,
            );
        }
    }
    section.extend_from_slice(&tag.unknown_data);
    write_u32(
        &mut section,
        4,
        u32::try_from(name_offset)
            .map_err(|_| BflanError::Invalid("PAT1 name offset exceeds u32"))?,
        endian,
    )?;
    write_u32(
        &mut section,
        8,
        u32::try_from(group_names_offset)
            .map_err(|_| BflanError::Invalid("PAT1 group offset exceeds u32"))?,
        endian,
    )?;
    Ok(section)
}

fn encode_pah1(share: &Pah1, endian: Endian) -> Result<Vec<u8>, BflanError> {
    let share_count = u16::try_from(share.shares.len())
        .map_err(|_| BflanError::Invalid("too many PAH1 shares"))?;
    let mut payload = Vec::new();
    append_u32(&mut payload, 16, endian);
    append_u16(&mut payload, share_count, endian);
    payload.extend_from_slice(&share.padding);
    for item in &share.shares {
        let record = wire::Pah1Share {
            source_pane: fixed_bytes(&item.source_pane)?,
            target_group: fixed_bytes(&item.target_group)?,
            padding: item.padding,
        };
        payload.extend_from_slice(&encode_record(&record, endian)?);
    }
    Ok(payload)
}

fn encode_pai1(info: &Pai1, endian: Endian) -> Result<Vec<u8>, BflanError> {
    let texture_count = u16::try_from(info.textures.len())
        .map_err(|_| BflanError::Invalid("too many PAI1 textures"))?;
    let entry_count = u16::try_from(info.entries.len())
        .map_err(|_| BflanError::Invalid("too many PAI1 entries"))?;
    let mut payload = encode_record(
        &wire::Pai1DataPrefix {
            frame_size: info.frame_size,
            looping: u8::from(info.looping),
            padding: info.padding,
            texture_count,
            entry_count,
            entry_offset_table: 0,
        },
        endian,
    )?;

    if !info.textures.is_empty() {
        let table_size = info.textures.len() * 4;
        payload.resize(12 + table_size, 0);
        for (index, texture) in info.textures.iter().enumerate() {
            let offset = payload
                .len()
                .checked_sub(12)
                .ok_or(BflanError::Invalid("PAI1 texture offset underflow"))?;
            write_u32(
                &mut payload,
                12 + index * 4,
                u32::try_from(offset)
                    .map_err(|_| BflanError::Invalid("PAI1 texture offset exceeds u32"))?,
                endian,
            )?;
            payload.extend_from_slice(texture.as_bytes());
            payload.push(0);
        }
        align_vec(&mut payload, 4);
    }

    let entry_table_offset = if info.entries.is_empty() {
        0
    } else {
        8usize
            .checked_add(payload.len())
            .ok_or(BflanError::Invalid("PAI1 entry table offset overflow"))?
    };
    if !info.entries.is_empty() {
        let entry_table_in_payload = payload.len();
        payload.resize(payload.len() + info.entries.len() * 4, 0);
        for (index, entry) in info.entries.iter().enumerate() {
            let entry_offset = 8usize
                .checked_add(payload.len())
                .ok_or(BflanError::Invalid("PAI1 entry offset overflow"))?;
            write_u32(
                &mut payload,
                entry_table_in_payload + index * 4,
                u32::try_from(entry_offset)
                    .map_err(|_| BflanError::Invalid("PAI1 entry offset exceeds u32"))?,
                endian,
            )?;
            payload.extend_from_slice(&encode_pai_entry(entry, endian)?);
        }
    }
    write_u32(
        &mut payload,
        8,
        u32::try_from(entry_table_offset)
            .map_err(|_| BflanError::Invalid("PAI1 entry table offset exceeds u32"))?,
        endian,
    )?;
    Ok(payload)
}

fn encode_pai_entry(entry: &PaiEntry, endian: Endian) -> Result<Vec<u8>, BflanError> {
    let tag_count =
        u8::try_from(entry.tags.len()).map_err(|_| BflanError::Invalid("too many PAI1 tags"))?;
    let mut output = fixed_bytes::<28>(&entry.name)?.to_vec();
    output.push(tag_count);
    output.push(u8::from(entry.target));
    append_u16(&mut output, entry.padding, endian);
    let offsets_start = output.len();
    output.resize(output.len() + entry.tags.len() * 4, 0);
    for (index, tag) in entry.tags.iter().enumerate() {
        let tag_offset = u32::try_from(output.len())
            .map_err(|_| BflanError::Invalid("PAI1 tag offset exceeds u32"))?;
        write_u32(&mut output, offsets_start + index * 4, tag_offset, endian)?;
        output.extend_from_slice(&encode_pai_tag(tag, entry.target, endian)?);
    }
    Ok(output)
}

fn encode_pai_tag(
    tag: &PaiTag,
    target: AnimationEntryTarget,
    endian: Endian,
) -> Result<Vec<u8>, BflanError> {
    let mut output = Vec::new();
    if u8::from(target) == 2 {
        append_u32(
            &mut output,
            tag.unknown_prefix.ok_or(BflanError::Invalid(
                "target-2 PAI1 tag is missing its prefix",
            ))?,
            endian,
        );
    }
    let tag_start = output.len();
    if tag.tag.len() != 4 {
        return Err(BflanError::Invalid("PAI1 tag must be four bytes"));
    }
    output.extend_from_slice(tag.tag.as_bytes());
    output.push(
        u8::try_from(tag.curves.len())
            .map_err(|_| BflanError::Invalid("too many PAI1 curves in tag"))?,
    );
    output.extend_from_slice(&tag.padding);
    let offsets_start = output.len();
    output.resize(output.len() + tag.curves.len() * 4, 0);
    for (index, curve) in tag.curves.iter().enumerate() {
        let curve_offset = u32::try_from(output.len() - tag_start)
            .map_err(|_| BflanError::Invalid("PAI1 curve offset exceeds u32"))?;
        write_u32(&mut output, offsets_start + index * 4, curve_offset, endian)?;
        output.extend_from_slice(&encode_animation_curve(curve, endian)?);
    }
    Ok(output)
}

fn encode_animation_curve(curve: &AnimationCurve, endian: Endian) -> Result<Vec<u8>, BflanError> {
    let curve_type = match curve.curve_type {
        CurveType::Constant => 0,
        CurveType::Step => 1,
        CurveType::Hermite => 2,
    };
    let key_count = u16::try_from(curve.keyframes.len())
        .map_err(|_| BflanError::Invalid("too many PAI1 curve keyframes"))?;
    let key_data_offset = if curve.key_data_offset == 0 {
        12
    } else {
        curve.key_data_offset
    };
    let mut output = encode_record(
        &wire::AnimationCurveHeader {
            id: curve.id,
            target: curve.target,
            curve_type,
            reserved: curve.reserved,
            key_count,
            padding: curve.padding,
            key_data_offset,
        },
        endian,
    )?;
    let key_start = usize::try_from(key_data_offset)
        .map_err(|_| BflanError::Invalid("PAI1 key data offset does not fit memory"))?;
    if key_start < output.len() {
        return Err(BflanError::Invalid(
            "PAI1 key data overlaps its curve header",
        ));
    }
    output.resize(key_start, 0);
    for key in &curve.keyframes {
        match (curve.curve_type, key) {
            (
                CurveType::Step,
                Keyframe::Step {
                    frame,
                    value,
                    padding,
                },
            ) => {
                output.extend_from_slice(&encode_record(
                    &wire::StepKeyframe {
                        frame: *frame,
                        value: *value,
                        padding: *padding,
                    },
                    endian,
                )?);
            }
            (
                CurveType::Hermite,
                Keyframe::Hermite {
                    frame,
                    value,
                    slope,
                },
            ) => {
                output.extend_from_slice(&encode_record(
                    &wire::HermiteKeyframe {
                        frame: *frame,
                        value: *value,
                        slope: *slope,
                    },
                    endian,
                )?);
            }
            (CurveType::Constant, _) => {
                return Err(BflanError::Invalid(
                    "constant curves cannot contain keyframes",
                ));
            }
            _ => {
                return Err(BflanError::Invalid(
                    "keyframe type does not match curve type",
                ));
            }
        }
    }
    output.extend_from_slice(&curve.raw_key_data);
    Ok(output)
}

fn fixed_bytes<const N: usize>(value: &str) -> Result<[u8; N], BflanError> {
    if value.len() > N {
        return Err(BflanError::Invalid("BFLAN fixed-width name is too long"));
    }
    let mut bytes = [0; N];
    bytes[..value.len()].copy_from_slice(value.as_bytes());
    Ok(bytes)
}

fn append_fixed_string(output: &mut Vec<u8>, value: &str, width: usize) -> Result<(), BflanError> {
    if value.len() > width {
        return Err(BflanError::Invalid("BFLAN fixed-width name is too long"));
    }
    output.extend_from_slice(value.as_bytes());
    output.resize(output.len() + width - value.len(), 0);
    Ok(())
}

fn append_u16(output: &mut Vec<u8>, value: u16, endian: Endian) {
    output.extend_from_slice(&match endian {
        Endian::Big => value.to_be_bytes(),
        Endian::Little => value.to_le_bytes(),
    });
}

fn append_u32(output: &mut Vec<u8>, value: u32, endian: Endian) {
    output.extend_from_slice(&match endian {
        Endian::Big => value.to_be_bytes(),
        Endian::Little => value.to_le_bytes(),
    });
}

fn write_u32(
    output: &mut [u8],
    offset: usize,
    value: u32,
    endian: Endian,
) -> Result<(), BflanError> {
    let bytes = match endian {
        Endian::Big => value.to_be_bytes(),
        Endian::Little => value.to_le_bytes(),
    };
    let range = output
        .get_mut(offset..offset + 4)
        .ok_or(BflanError::Invalid("BFLAN write offset is out of bounds"))?;
    range.copy_from_slice(&bytes);
    Ok(())
}

fn align_vec(output: &mut Vec<u8>, alignment: usize) {
    let remainder = output.len() % alignment;
    if remainder != 0 {
        output.resize(output.len() + alignment - remainder, 0);
    }
}

fn parse_pat1(section: &[u8], endian: Endian, major_version: u8) -> Result<Pat1, BflanError> {
    let payload = section
        .get(8..)
        .ok_or(BflanError::Invalid("PAT1 section is truncated"))?;
    let mut cursor = Cursor::new(payload);
    let prefix = wire::Pat1DataPrefix::read_options(&mut cursor, endian, ())?;
    let version_extension = if major_version >= 8 {
        Some(read_u32(section, 20, endian)?)
    } else {
        None
    };
    let metadata_offset = if version_extension.is_some() { 24 } else { 20 };
    let mut metadata_cursor = Cursor::new(checked_slice(section, metadata_offset, 5)?);
    let metadata = wire::Pat1FrameMetadata::read_options(&mut metadata_cursor, endian, ())?;
    let padding_start = metadata_offset + 5;
    if section.len() < padding_start {
        return Err(BflanError::Invalid("PAT1 metadata is truncated"));
    }

    let name_offset = usize::try_from(prefix.animation_name_offset)
        .map_err(|_| BflanError::Invalid("PAT1 name offset does not fit memory"))?;
    if name_offset < padding_start {
        return Err(BflanError::Invalid("PAT1 name overlaps its metadata"));
    }
    let name = read_section_string(section, name_offset)?;
    let (group_width, group_size) = if major_version >= 8 {
        (28, 28)
    } else if major_version < 5 {
        (24, 28)
    } else {
        (33, 36)
    };
    let group_start = prefix.group_names_offset as usize;
    let group_bytes = usize::from(prefix.group_count)
        .checked_mul(group_size)
        .ok_or(BflanError::Invalid("PAT1 group table size overflow"))?;
    checked_slice(section, group_start, group_bytes)?;

    let mut groups = Vec::with_capacity(usize::from(prefix.group_count));
    let mut group_flags = Vec::with_capacity(usize::from(prefix.group_count));
    let mut group_padding = Vec::with_capacity(usize::from(prefix.group_count));
    for index in 0..usize::from(prefix.group_count) {
        let offset = group_start + index * group_size;
        groups.push(read_fixed_string(section, offset, group_width)?);
        if major_version < 8 {
            group_flags.push(section[offset + group_width]);
            group_padding.push(section[offset + group_width + 1..offset + group_size].to_vec());
        }
    }

    let group_end = group_start + group_bytes;
    Ok(Pat1 {
        order: prefix.order,
        name,
        start_frame: metadata.start_frame,
        end_frame: metadata.end_frame,
        child_binding: metadata.child_binding & 1 != 0,
        flags: metadata.child_binding,
        version_extension,
        pre_name_data: checked_slice(section, padding_start, name_offset - padding_start)?.to_vec(),
        groups,
        group_flags,
        group_padding,
        unknown_data: section[group_end..].to_vec(),
    })
}

fn parse_pah1(section: &[u8], endian: Endian) -> Result<Pah1, BflanError> {
    let share_offset = read_u32(section, 8, endian)? as usize;
    let share_count = usize::from(read_u16(section, 12, endian)?);
    let padding = checked_slice(section, 14, 2)?
        .try_into()
        .map_err(|_| BflanError::Invalid("PAH1 padding is truncated"))?;
    let share_size = std::mem::size_of::<wire::Pah1Share>();
    checked_slice(
        section,
        share_offset,
        share_count
            .checked_mul(share_size)
            .ok_or(BflanError::Invalid("PAH1 share table size overflow"))?,
    )?;

    let mut shares = Vec::with_capacity(share_count);
    for index in 0..share_count {
        let offset = share_offset + index * share_size;
        let mut cursor = Cursor::new(&section[offset..offset + share_size]);
        let share = wire::Pah1Share::read_options(&mut cursor, endian, ())?;
        shares.push(AnimationShare {
            source_pane: read_fixed_bytes(&share.source_pane)?,
            target_group: read_fixed_bytes(&share.target_group)?,
            padding: share.padding,
        });
    }
    Ok(Pah1 { padding, shares })
}

fn parse_pai1(section: &[u8], endian: Endian) -> Result<Pai1, BflanError> {
    let payload = section
        .get(8..)
        .ok_or(BflanError::Invalid("PAI1 section is truncated"))?;
    let mut cursor = Cursor::new(payload);
    let prefix = wire::Pai1DataPrefix::read_options(&mut cursor, endian, ())?;

    let texture_count = usize::from(prefix.texture_count);
    let mut textures = Vec::with_capacity(texture_count);
    for index in 0..texture_count {
        let offset = 20usize
            .checked_add(index * 4)
            .ok_or(BflanError::Invalid("PAI1 texture table offset overflow"))?;
        let name_offset = read_u32(section, offset, endian)? as usize;
        let absolute_name_offset = 20usize
            .checked_add(name_offset)
            .ok_or(BflanError::Invalid("PAI1 texture name offset overflow"))?;
        textures.push(read_section_string(section, absolute_name_offset)?);
    }

    let entry_count = usize::from(prefix.entry_count);
    let entry_table = prefix.entry_offset_table as usize;
    let table_size = entry_count
        .checked_mul(4)
        .ok_or(BflanError::Invalid("PAI1 entry table size overflow"))?;
    checked_slice(section, entry_table, table_size)?;
    let mut entries = Vec::with_capacity(entry_count);
    for index in 0..entry_count {
        let entry_offset = read_u32(section, entry_table + index * 4, endian)? as usize;
        entries.push(parse_pai_entry(section, entry_offset, endian)?);
    }

    Ok(Pai1 {
        frame_size: prefix.frame_size,
        looping: prefix.looping & 1 != 0,
        padding: prefix.padding,
        textures,
        entries,
    })
}

fn parse_pai_entry(section: &[u8], offset: usize, endian: Endian) -> Result<PaiEntry, BflanError> {
    let entry_bytes = checked_slice(section, offset, std::mem::size_of::<wire::PaiEntryHeader>())?;
    let mut cursor = Cursor::new(entry_bytes);
    let header = wire::PaiEntryHeader::read_options(&mut cursor, endian, ())?;
    let name = read_fixed_bytes(&header.name)?;
    let tag_count = usize::from(header.tag_count);
    let tag_table = offset + std::mem::size_of::<wire::PaiEntryHeader>();
    checked_slice(
        section,
        tag_table,
        tag_count
            .checked_mul(4)
            .ok_or(BflanError::Invalid("PAI1 tag table size overflow"))?,
    )?;

    let mut tags = Vec::with_capacity(tag_count);
    for index in 0..tag_count {
        let tag_offset = read_u32(section, tag_table + index * 4, endian)? as usize;
        let tag_start = offset
            .checked_add(tag_offset)
            .ok_or(BflanError::Invalid("PAI1 tag offset overflow"))?;
        tags.push(parse_pai_tag(section, tag_start, endian, header.target)?);
    }
    Ok(PaiEntry {
        name,
        target: AnimationEntryTarget::from(header.target),
        padding: header.padding,
        tags,
    })
}

fn parse_pai_tag(
    section: &[u8],
    offset: usize,
    endian: Endian,
    target: u8,
) -> Result<PaiTag, BflanError> {
    let (unknown_prefix, tag_offset) = if target == 2 {
        (Some(read_u32(section, offset, endian)?), offset + 4)
    } else {
        (None, offset)
    };
    checked_slice(
        section,
        tag_offset,
        std::mem::size_of::<wire::PaiTagHeader>(),
    )?;
    let magic: [u8; 4] = checked_slice(section, tag_offset, 4)?
        .try_into()
        .map_err(|_| BflanError::Invalid("PAI1 tag magic is truncated"))?;
    let count_and_padding: [u8; 4] = checked_slice(section, tag_offset + 4, 4)?
        .try_into()
        .map_err(|_| BflanError::Invalid("PAI1 tag header is truncated"))?;
    let count = usize::from(count_and_padding[0]);
    let curve_table = tag_offset + std::mem::size_of::<wire::PaiTagHeader>();
    checked_slice(
        section,
        curve_table,
        count
            .checked_mul(4)
            .ok_or(BflanError::Invalid("PAI1 curve table size overflow"))?,
    )?;

    let mut curves = Vec::with_capacity(count);
    for index in 0..count {
        let curve_offset = read_u32(section, curve_table + index * 4, endian)? as usize;
        let curve_start = tag_offset
            .checked_add(curve_offset)
            .ok_or(BflanError::Invalid("PAI1 curve offset overflow"))?;
        curves.push(parse_animation_curve(section, curve_start, endian)?);
    }
    let tag = String::from_utf8(magic.to_vec())
        .map_err(|_| BflanError::Invalid("PAI1 tag is not UTF-8"))?;
    Ok(PaiTag {
        tag,
        unknown_prefix,
        padding: count_and_padding[1..]
            .try_into()
            .map_err(|_| BflanError::Invalid("PAI1 tag padding is truncated"))?,
        curves,
    })
}

fn parse_animation_curve(
    section: &[u8],
    offset: usize,
    endian: Endian,
) -> Result<AnimationCurve, BflanError> {
    let header_bytes = checked_slice(
        section,
        offset,
        std::mem::size_of::<wire::AnimationCurveHeader>(),
    )?;
    let mut cursor = Cursor::new(header_bytes);
    let header = wire::AnimationCurveHeader::read_options(&mut cursor, endian, ())?;
    let key_count = usize::from(header.key_count);
    let key_data_offset = usize::try_from(header.key_data_offset)
        .map_err(|_| BflanError::Invalid("PAI1 key data offset does not fit memory"))?;
    let key_start = offset
        .checked_add(key_data_offset)
        .ok_or(BflanError::Invalid("PAI1 key data offset overflow"))?;
    let (curve_type, key_size) = match header.curve_type {
        0 => (CurveType::Constant, 0),
        1 => (CurveType::Step, std::mem::size_of::<wire::StepKeyframe>()),
        2 => (
            CurveType::Hermite,
            std::mem::size_of::<wire::HermiteKeyframe>(),
        ),
        value => return Err(BflanError::UnsupportedCurveType(value)),
    };
    if header.curve_type == 0 && key_count != 0 {
        return Err(BflanError::Invalid("constant curve has keyframes"));
    }
    let key_bytes = key_count
        .checked_mul(key_size)
        .ok_or(BflanError::Invalid("PAI1 keyframe data size overflow"))?;
    if key_start
        .checked_add(key_bytes)
        .is_none_or(|end| end > section.len())
    {
        return Err(BflanError::CurveDataOutOfBounds {
            curve_offset: offset,
            key_data_offset: header.key_data_offset,
            curve_type: header.curve_type,
            key_count: header.key_count,
            data_len: section.len(),
        });
    }
    checked_slice(section, key_start, key_bytes)?;

    let mut keyframes = Vec::with_capacity(key_count);
    for index in 0..key_count {
        let key_offset = key_start + index * key_size;
        if header.curve_type == 1 {
            let mut cursor = Cursor::new(&section[key_offset..key_offset + key_size]);
            let key = wire::StepKeyframe::read_options(&mut cursor, endian, ())?;
            keyframes.push(Keyframe::Step {
                frame: key.frame,
                value: key.value,
                padding: key.padding,
            });
        } else {
            let mut cursor = Cursor::new(&section[key_offset..key_offset + key_size]);
            let key = wire::HermiteKeyframe::read_options(&mut cursor, endian, ())?;
            keyframes.push(Keyframe::Hermite {
                frame: key.frame,
                value: key.value,
                slope: key.slope,
            });
        }
    }

    Ok(AnimationCurve {
        id: header.id,
        target: header.target,
        curve_type,
        reserved: header.reserved,
        padding: header.padding,
        key_data_offset: header.key_data_offset,
        keyframes,
        raw_key_data: Vec::new(),
    })
}

fn checked_slice(data: &[u8], offset: usize, size: usize) -> Result<&[u8], BflanError> {
    let Some(end) = offset.checked_add(size) else {
        return Err(BflanError::OutOfBounds {
            offset,
            size,
            data_len: data.len(),
        });
    };
    data.get(offset..end).ok_or(BflanError::OutOfBounds {
        offset,
        size,
        data_len: data.len(),
    })
}

fn read_u16(data: &[u8], offset: usize, endian: Endian) -> Result<u16, BflanError> {
    let bytes = checked_slice(data, offset, 2)?
        .try_into()
        .map_err(|_| BflanError::Invalid("BFLAN u16 is truncated"))?;
    Ok(match endian {
        Endian::Big => u16::from_be_bytes(bytes),
        Endian::Little => u16::from_le_bytes(bytes),
    })
}

fn read_u32(data: &[u8], offset: usize, endian: Endian) -> Result<u32, BflanError> {
    let bytes = checked_slice(data, offset, 4)?
        .try_into()
        .map_err(|_| BflanError::Invalid("BFLAN u32 is truncated"))?;
    Ok(match endian {
        Endian::Big => u32::from_be_bytes(bytes),
        Endian::Little => u32::from_le_bytes(bytes),
    })
}

fn read_section_string(section: &[u8], offset: usize) -> Result<String, BflanError> {
    let tail = section
        .get(offset..)
        .ok_or(BflanError::Invalid("BFLAN string offset is out of bounds"))?;
    let length = tail
        .iter()
        .position(|byte| *byte == 0)
        .ok_or(BflanError::Invalid("unterminated BFLAN string"))?;
    String::from_utf8(tail[..length].to_vec())
        .map_err(|_| BflanError::Invalid("BFLAN string is not UTF-8"))
}

fn read_fixed_string(section: &[u8], offset: usize, width: usize) -> Result<String, BflanError> {
    let bytes = checked_slice(section, offset, width)?;
    read_fixed_bytes(bytes)
}

fn read_fixed_bytes(bytes: &[u8]) -> Result<String, BflanError> {
    let length = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8(bytes[..length].to_vec())
        .map_err(|_| BflanError::Invalid("BFLAN name is not UTF-8"))
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
    pub flags: u8,
    pub version_extension: Option<u32>,
    pub pre_name_data: Vec<u8>,
    pub groups: Vec<String>,
    pub group_flags: Vec<u8>,
    pub group_padding: Vec<Vec<u8>>,
    pub unknown_data: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UnknownBflanSection {
    pub magic: [u8; 4],
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pah1 {
    pub padding: [u8; 2],
    pub shares: Vec<AnimationShare>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnimationShare {
    pub source_pane: String,
    pub target_group: String,
    pub padding: [u8; 2],
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pai1 {
    pub frame_size: u16,
    pub looping: bool,
    pub padding: u8,
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
    pub padding: [u8; 3],
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
    pub raw_key_data: Vec<u8>,
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
    Step {
        frame: f32,
        value: u16,
        #[serde(default)]
        padding: u16,
    },
    Hermite {
        frame: f32,
        value: f32,
        slope: f32,
    },
}
