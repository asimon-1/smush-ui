use std::collections::BTreeMap;
use std::fmt;
use std::io::Cursor;

use binrw::{BinRead, BinWrite, Endian};

use crate::binary::ByteOrder;

const SARC_HEADER_SIZE: u16 = 0x14;
const SFAT_HEADER_SIZE: u16 = 0x0C;
const SFNT_HEADER_SIZE: u16 = 0x08;
const MAX_LAYOUT_ENTRIES: usize = 0x3FFF;
const DEFAULT_HASH_MULTIPLIER: u32 = 0x65;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BinRead, BinWrite)]
struct SarcHeader {
    magic: [u8; 4],
    header_size: u16,
    byte_order_mark: u16,
    file_size: u32,
    data_offset: u32,
    version: u16,
    reserved: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BinRead, BinWrite)]
struct SfatHeader {
    magic: [u8; 4],
    header_size: u16,
    node_count: u16,
    hash_multiplier: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BinRead, BinWrite)]
struct SfatEntry {
    name_hash: u32,
    attributes: u32,
    data_start: u32,
    data_end: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BinRead, BinWrite)]
struct SfntHeader {
    magic: [u8; 4],
    header_size: u16,
    reserved: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SarcArchive {
    pub files: BTreeMap<String, Vec<u8>>,
    pub byte_order: ByteOrder,
    pub hash_multiplier: u32,
}

impl Default for SarcArchive {
    fn default() -> Self {
        Self {
            files: BTreeMap::new(),
            byte_order: ByteOrder::LittleEndian,
            hash_multiplier: DEFAULT_HASH_MULTIPLIER,
        }
    }
}

impl SarcArchive {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SarcError> {
        if bytes.len() < 8 || &bytes[..4] != b"SARC" {
            return Err(SarcError::Invalid("missing SARC signature"));
        }

        let byte_order = match &bytes[6..8] {
            [0xFE, 0xFF] => ByteOrder::BigEndian,
            [0xFF, 0xFE] => ByteOrder::LittleEndian,
            _ => return Err(SarcError::Invalid("invalid SARC byte-order mark")),
        };
        let endian = binrw_endian(byte_order);
        let mut cursor = Cursor::new(bytes);
        let header = SarcHeader::read_options(&mut cursor, endian, ())?;

        if header.magic != *b"SARC" {
            return Err(SarcError::Invalid("invalid SARC signature"));
        }
        if header.header_size != SARC_HEADER_SIZE {
            return Err(SarcError::Invalid("unsupported SARC header size"));
        }
        if header.byte_order_mark != 0xFEFF {
            return Err(SarcError::Invalid("invalid SARC byte-order mark"));
        }

        let file_size = usize::try_from(header.file_size)
            .map_err(|_| SarcError::Invalid("SARC file size does not fit memory"))?;
        let data_offset = usize::try_from(header.data_offset)
            .map_err(|_| SarcError::Invalid("SARC data offset does not fit memory"))?;
        if file_size > bytes.len() || file_size < SARC_HEADER_SIZE as usize {
            return Err(SarcError::Invalid("SARC file size is out of bounds"));
        }
        if data_offset > file_size || data_offset < SARC_HEADER_SIZE as usize {
            return Err(SarcError::Invalid("SARC data offset is out of bounds"));
        }

        cursor.set_position(header.header_size as u64);
        let sfat = SfatHeader::read_options(&mut cursor, endian, ())?;
        if sfat.magic != *b"SFAT" || sfat.header_size != SFAT_HEADER_SIZE {
            return Err(SarcError::Invalid("invalid SFAT header"));
        }
        if usize::from(sfat.node_count) > MAX_LAYOUT_ENTRIES {
            return Err(SarcError::Invalid(
                "SARC contains too many entries for game layouts",
            ));
        }

        let entries_size = usize::from(sfat.node_count)
            .checked_mul(std::mem::size_of::<SfatEntry>())
            .ok_or(SarcError::Invalid("SFAT entry table size overflow"))?;
        let entries_end = usize::try_from(cursor.position())
            .map_err(|_| SarcError::Invalid("SFAT position does not fit memory"))?
            .checked_add(entries_size)
            .ok_or(SarcError::Invalid("SFAT entry table size overflow"))?;
        if entries_end > data_offset || entries_end > file_size {
            return Err(SarcError::Invalid("SFAT entry table is out of bounds"));
        }

        let mut entries = Vec::with_capacity(usize::from(sfat.node_count));
        for _ in 0..sfat.node_count {
            entries.push(SfatEntry::read_options(&mut cursor, endian, ())?);
        }

        let sfnt_position = usize::try_from(cursor.position())
            .map_err(|_| SarcError::Invalid("SFNT position does not fit memory"))?;
        cursor.set_position(sfnt_position as u64);
        let sfnt = SfntHeader::read_options(&mut cursor, endian, ())?;
        if sfnt.magic != *b"SFNT" || sfnt.header_size != SFNT_HEADER_SIZE {
            return Err(SarcError::Invalid("invalid SFNT header"));
        }
        let names_start = usize::try_from(cursor.position())
            .map_err(|_| SarcError::Invalid("SFNT name position does not fit memory"))?;
        if names_start > data_offset {
            return Err(SarcError::Invalid("SFNT name table is out of bounds"));
        }

        let mut files = BTreeMap::new();
        for entry in entries {
            if entry.attributes & 0x0100_0000 == 0 {
                return Err(SarcError::Invalid("unnamed SARC entries are unsupported"));
            }
            let name_offset = usize::try_from(entry.attributes & 0x00FF_FFFF)
                .map_err(|_| SarcError::Invalid("SARC name offset does not fit memory"))?
                .checked_mul(4)
                .and_then(|offset| names_start.checked_add(offset))
                .ok_or(SarcError::Invalid("SARC name offset overflow"))?;
            if name_offset >= data_offset {
                return Err(SarcError::Invalid("SARC name offset is out of bounds"));
            }
            let name_end = bytes[name_offset..data_offset]
                .iter()
                .position(|byte| *byte == 0)
                .map(|length| name_offset + length)
                .ok_or(SarcError::Invalid("unterminated SARC filename"))?;
            let name = std::str::from_utf8(&bytes[name_offset..name_end])
                .map_err(|_| SarcError::Invalid("SARC filename is not UTF-8"))?
                .to_owned();

            let start = data_offset
                .checked_add(entry.data_start as usize)
                .ok_or(SarcError::Invalid("SARC entry offset overflow"))?;
            let end = data_offset
                .checked_add(entry.data_end as usize)
                .ok_or(SarcError::Invalid("SARC entry offset overflow"))?;
            if start > end || end > file_size {
                return Err(SarcError::Invalid("SARC entry data is out of bounds"));
            }
            if files.insert(name, bytes[start..end].to_vec()).is_some() {
                return Err(SarcError::Invalid("duplicate SARC filename"));
            }
        }

        Ok(Self {
            files,
            byte_order,
            hash_multiplier: sfat.hash_multiplier,
        })
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, SarcError> {
        if self.files.len() > MAX_LAYOUT_ENTRIES {
            return Err(SarcError::Invalid(
                "SARC contains too many entries for game layouts",
            ));
        }

        let mut ordered_files: Vec<_> = self.files.iter().collect();
        ordered_files.sort_by(|(left_name, _), (right_name, _)| {
            let left_hash = hash_name(left_name, self.hash_multiplier);
            let right_hash = hash_name(right_name, self.hash_multiplier);
            left_hash
                .cmp(&right_hash)
                .then_with(|| left_name.cmp(right_name))
        });

        let mut name_table = Vec::new();
        let mut name_offsets = Vec::with_capacity(ordered_files.len());
        for (name, _) in &ordered_files {
            if name.is_empty() || name.as_bytes().contains(&0) {
                return Err(SarcError::Invalid(
                    "SARC filenames must be nonempty and contain no NUL",
                ));
            }
            let offset = name_table.len();
            if offset % 4 != 0 || offset / 4 > 0x00FF_FFFF {
                return Err(SarcError::Invalid("SARC filename table is too large"));
            }
            name_offsets.push((offset / 4) as u32);
            name_table.extend_from_slice(name.as_bytes());
            name_table.push(0);
            while name_table.len() % 4 != 0 {
                name_table.push(0);
            }
        }

        let sfat_size = (SFAT_HEADER_SIZE as usize)
            .checked_add(ordered_files.len() * std::mem::size_of::<SfatEntry>())
            .ok_or(SarcError::Invalid("SFAT size overflow"))?;
        let sfnt_size = (SFNT_HEADER_SIZE as usize)
            .checked_add(name_table.len())
            .ok_or(SarcError::Invalid("SFNT size overflow"))?;
        let has_gpu_resources = ordered_files
            .iter()
            .any(|(name, _)| needs_gpu_alignment(name));
        let data_alignment = if has_gpu_resources { 0x1000 } else { 0x100 };
        let data_offset = align_up(
            SARC_HEADER_SIZE as usize + sfat_size + sfnt_size,
            data_alignment,
        )?;

        let mut data_blob = Vec::new();
        let mut data_ranges = Vec::with_capacity(ordered_files.len());
        for (name, data) in &ordered_files {
            let alignment = if needs_gpu_alignment(name) {
                0x1000
            } else {
                0x10
            };
            let absolute_position = data_offset
                .checked_add(data_blob.len())
                .ok_or(SarcError::Invalid("SARC entry position overflow"))?;
            let aligned_position = align_up(absolute_position, alignment)?;
            data_blob.resize(data_blob.len() + aligned_position - absolute_position, 0);

            let start = u32::try_from(data_blob.len())
                .map_err(|_| SarcError::Invalid("SARC entry offset exceeds u32"))?;
            data_blob.extend_from_slice(data);
            let end = u32::try_from(data_blob.len())
                .map_err(|_| SarcError::Invalid("SARC entry size exceeds u32"))?;
            data_ranges.push((start, end));
        }

        let file_size = data_offset
            .checked_add(data_blob.len())
            .ok_or(SarcError::Invalid("SARC file size overflow"))?;
        let header = SarcHeader {
            magic: *b"SARC",
            header_size: SARC_HEADER_SIZE,
            byte_order_mark: 0xFEFF,
            file_size: u32::try_from(file_size)
                .map_err(|_| SarcError::Invalid("SARC file exceeds u32"))?,
            data_offset: u32::try_from(data_offset)
                .map_err(|_| SarcError::Invalid("SARC data offset exceeds u32"))?,
            version: 0x0100,
            reserved: 0,
        };
        let sfat = SfatHeader {
            magic: *b"SFAT",
            header_size: SFAT_HEADER_SIZE,
            node_count: u16::try_from(ordered_files.len())
                .map_err(|_| SarcError::Invalid("too many SARC entries"))?,
            hash_multiplier: self.hash_multiplier,
        };
        let sfnt = SfntHeader {
            magic: *b"SFNT",
            header_size: SFNT_HEADER_SIZE,
            reserved: 0,
        };
        let endian = binrw_endian(self.byte_order);
        let mut output = Cursor::new(Vec::with_capacity(file_size));
        header.write_options(&mut output, endian, ())?;
        sfat.write_options(&mut output, endian, ())?;
        for (((name, _), name_offset), (data_start, data_end)) in
            ordered_files.iter().zip(name_offsets).zip(data_ranges)
        {
            let entry = SfatEntry {
                name_hash: hash_name(name, self.hash_multiplier),
                attributes: 0x0100_0000 | name_offset,
                data_start,
                data_end,
            };
            entry.write_options(&mut output, endian, ())?;
        }
        sfnt.write_options(&mut output, endian, ())?;
        output.get_mut().extend_from_slice(&name_table);
        if output.get_ref().len() > data_offset {
            return Err(SarcError::Invalid("SARC tables overlap the data block"));
        }
        output.get_mut().resize(data_offset, 0);
        output.get_mut().extend_from_slice(&data_blob);
        Ok(output.into_inner())
    }
}

#[derive(Debug)]
pub enum SarcError {
    Binary(binrw::Error),
    Invalid(&'static str),
}

impl fmt::Display for SarcError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Binary(error) => write!(formatter, "SARC binary error: {error}"),
            Self::Invalid(message) => write!(formatter, "invalid SARC archive: {message}"),
        }
    }
}

impl std::error::Error for SarcError {}

impl From<binrw::Error> for SarcError {
    fn from(error: binrw::Error) -> Self {
        Self::Binary(error)
    }
}

fn binrw_endian(byte_order: ByteOrder) -> Endian {
    match byte_order {
        ByteOrder::BigEndian => Endian::Big,
        ByteOrder::LittleEndian => Endian::Little,
    }
}

fn hash_name(name: &str, multiplier: u32) -> u32 {
    name.as_bytes().iter().fold(0, |hash, byte| {
        hash.wrapping_mul(multiplier).wrapping_add(u32::from(*byte))
    })
}

fn needs_gpu_alignment(name: &str) -> bool {
    name.ends_with(".bnsh") || name.ends_with(".bntx")
}

fn align_up(value: usize, alignment: usize) -> Result<usize, SarcError> {
    let remainder = value % alignment;
    if remainder == 0 {
        Ok(value)
    } else {
        value
            .checked_add(alignment - remainder)
            .ok_or(SarcError::Invalid("SARC alignment overflow"))
    }
}
