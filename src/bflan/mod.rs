use std::io::{Cursor, Read, Seek, SeekFrom, Write};

use binrw::{BinReaderExt, BinResult, BinWriterExt, Endian, binrw};
use serde::{Deserialize, Serialize};

mod common;
pub mod inspect;
mod pah1;
mod pai1;
mod pat1;

use common::{checked_u32, invalid};

pub use pah1::{Pah1, SharedAnimation};
pub use pai1::{AnimationEntry, AnimationTag, Curve, HermiteKey, Pai1, PaiFlags, StepKey, Track};
pub use pat1::{GroupRecordSize, Pat1};

const HEADER_SIZE: usize = 20;
const VERSION: u32 = 0x0900_0000;

#[binrw]
#[brw(little, magic = b"FLAN\xff\xfe\x14\x00")]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bflan {
    #[br(assert(version == VERSION, "unsupported BFLAN version"))]
    #[bw(assert(*version == VERSION, "unsupported BFLAN version"))]
    pub version: u32,
    #[br(temp)]
    #[bw(calc = serialized_file_size(sections)?)]
    file_size: u32,
    #[br(temp)]
    #[bw(calc = u16::try_from(sections.len()).map_err(|_| invalid(16, "too many BFLAN sections"))?)]
    #[brw(pad_after = 2)]
    section_count: u16,
    #[br(parse_with = read_sections, args(file_size, section_count))]
    #[bw(write_with = write_sections)]
    pub sections: Vec<Section>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Section {
    Pat1(Pat1),
    Pah1(Pah1),
    Pai1(Pai1),
    Unknown { magic: String, data: Vec<u8> },
}

impl Bflan {
    pub fn new(sections: Vec<Section>) -> Self {
        Self {
            version: VERSION,
            sections,
        }
    }

    pub fn from_bytes(data: &[u8]) -> BinResult<Self> {
        let mut reader = Cursor::new(data);
        let animation = reader.read_le::<Self>()?;
        if reader.position() != data.len() as u64 {
            return Err(invalid(
                reader.position() as usize,
                "trailing bytes after BFLAN",
            ));
        }
        Ok(animation)
    }

    pub fn to_bytes(&self) -> BinResult<Vec<u8>> {
        let mut writer = Cursor::new(Vec::new());
        writer.write_le(self)?;
        Ok(writer.into_inner())
    }
}

fn read_sections<R: Read + Seek>(
    reader: &mut R,
    _: Endian,
    (file_size, section_count): (u32, u16),
) -> BinResult<Vec<Section>> {
    let body_start = reader.stream_position()?;
    let file_start = body_start
        .checked_sub(HEADER_SIZE as u64)
        .ok_or_else(|| invalid(0, "BFLAN starts before the stream"))?;
    let file_end = file_start
        .checked_add(file_size as u64)
        .ok_or_else(|| invalid(body_start as usize, "BFLAN file size overflow"))?;
    if file_size < HEADER_SIZE as u32 || file_end > reader.seek(SeekFrom::End(0))? {
        return Err(invalid(
            body_start as usize,
            "BFLAN file size exceeds input",
        ));
    }
    reader.seek(SeekFrom::Start(body_start))?;
    let mut sections = Vec::with_capacity((section_count as usize).min(file_size as usize / 8));
    for _ in 0..section_count {
        let start = reader.stream_position()?;
        if start.checked_add(8).is_none_or(|end| end > file_end) {
            return Err(invalid(start as usize, "truncated BFLAN section header"));
        }
        let header: SectionHeader = reader.read_le()?;
        let size = header.size as usize;
        if size < 8
            || start
                .checked_add(size as u64)
                .is_none_or(|end| end > file_end)
        {
            return Err(invalid(start as usize + 4, "invalid BFLAN section size"));
        }
        let mut data = vec![0u8; size];
        data[..4].copy_from_slice(&header.magic);
        data[4..8].copy_from_slice(&header.size.to_le_bytes());
        reader.read_exact(&mut data[8..])?;
        let section = match &header.magic {
            b"pat1" => Section::Pat1(Cursor::new(&data).read_le()?),
            b"pah1" => Section::Pah1(Cursor::new(&data).read_le()?),
            b"pai1" => Section::Pai1(Cursor::new(&data).read_le()?),
            magic => Section::Unknown {
                magic: std::str::from_utf8(magic)
                    .map_err(|_| invalid(start as usize, "non-ASCII BFLAN section tag"))?
                    .to_owned(),
                data: data[8..].to_vec(),
            },
        };
        sections.push(section);
    }
    if reader.stream_position()? != file_end {
        return Err(invalid(
            file_end as usize,
            "BFLAN section count does not cover the file",
        ));
    }
    Ok(sections)
}

#[allow(clippy::ptr_arg)] // binrw requires the write callback to accept the field's Vec type.
fn write_sections<W: Write + Seek>(
    sections: &Vec<Section>,
    writer: &mut W,
    _: Endian,
    _: (),
) -> BinResult<()> {
    writer.write_all(&serialized_sections(sections)?)?;
    Ok(())
}

fn serialized_file_size(sections: &[Section]) -> BinResult<u32> {
    let body = serialized_sections(sections)?;
    checked_u32(HEADER_SIZE + body.len(), 12)
}

fn serialized_sections(sections: &[Section]) -> BinResult<Vec<u8>> {
    let mut out = Vec::new();
    for section in sections {
        let start = out.len();
        match section {
            Section::Pat1(value) => {
                let mut writer = Cursor::new(Vec::new());
                writer.write_le(value)?;
                out.extend(writer.into_inner());
            }
            Section::Pah1(value) => {
                let mut writer = Cursor::new(Vec::new());
                writer.write_le(value)?;
                out.extend(writer.into_inner());
            }
            Section::Pai1(value) => {
                let mut writer = Cursor::new(Vec::new());
                writer.write_le(value)?;
                out.extend(writer.into_inner());
            }
            Section::Unknown { magic, data } => {
                if magic.len() != 4 || !magic.is_ascii() {
                    return Err(invalid(start, "BFLAN section tag must be four ASCII bytes"));
                }
                out.extend_from_slice(magic.as_bytes());
                out.extend_from_slice(&checked_u32(data.len() + 8, start)?.to_le_bytes());
                out.extend_from_slice(data);
            }
        }
    }
    Ok(out)
}

#[binrw]
#[brw(little)]
struct SectionHeader {
    magic: [u8; 4],
    size: u32,
}
