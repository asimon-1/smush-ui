use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};

use binrw::{BinReaderExt, BinResult, BinWriterExt, Endian, Error as BinError, binrw};
use serde::{Deserialize, Serialize};

pub mod inspect;
mod pah1;
mod pai1;
mod pat1;

pub use pah1::{Pah1, SharedAnimation};
pub use pai1::{AnimationEntry, AnimationTag, Curve, HermiteKey, Pai1, PaiFlags, StepKey, Track};
pub use pat1::Pat1;

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

pub(crate) fn invalid(pos: usize, message: &'static str) -> BinError {
    BinError::Custom {
        pos: pos as u64,
        err: Box::new(io::Error::new(io::ErrorKind::InvalidData, message)),
    }
}

pub(crate) fn bytes_at(data: &[u8], offset: usize, len: usize) -> BinResult<&[u8]> {
    let end = offset
        .checked_add(len)
        .ok_or_else(|| invalid(offset, "offset overflow"))?;
    data.get(offset..end)
        .ok_or_else(|| invalid(offset, "offset outside BFLAN record"))
}

pub(crate) fn u16_at(data: &[u8], offset: usize) -> BinResult<u16> {
    Ok(u16::from_le_bytes(
        bytes_at(data, offset, 2)?.try_into().unwrap(),
    ))
}

pub(crate) fn u32_at(data: &[u8], offset: usize) -> BinResult<u32> {
    Ok(u32::from_le_bytes(
        bytes_at(data, offset, 4)?.try_into().unwrap(),
    ))
}

pub(crate) fn checked_u32(value: usize, pos: usize) -> BinResult<u32> {
    u32::try_from(value).map_err(|_| invalid(pos, "BFLAN offset exceeds u32"))
}

pub(crate) fn read_cstr(data: &[u8], offset: usize) -> BinResult<String> {
    let tail = data
        .get(offset..)
        .ok_or_else(|| invalid(offset, "string offset outside record"))?;
    let end = tail
        .iter()
        .position(|&byte| byte == 0)
        .ok_or_else(|| invalid(offset, "unterminated BFLAN string"))?;
    std::str::from_utf8(&tail[..end])
        .map(str::to_owned)
        .map_err(|_| invalid(offset, "invalid UTF-8 BFLAN string"))
}

pub(crate) fn read_fixed(data: &[u8], offset: usize, len: usize) -> BinResult<String> {
    let raw = bytes_at(data, offset, len)?;
    let end = raw.iter().position(|&byte| byte == 0).unwrap_or(len);
    std::str::from_utf8(&raw[..end])
        .map(str::to_owned)
        .map_err(|_| invalid(offset, "invalid UTF-8 BFLAN name"))
}

pub(crate) fn write_fixed(out: &mut Vec<u8>, value: &str, len: usize) -> BinResult<()> {
    if value.len() > len {
        return Err(invalid(out.len(), "BFLAN fixed name is too long"));
    }
    out.extend_from_slice(value.as_bytes());
    out.resize(out.len() + len - value.len(), 0);
    Ok(())
}

pub(crate) fn align4(out: &mut Vec<u8>) {
    out.resize((out.len() + 3) & !3, 0);
}

pub(crate) fn patch_u32(out: &mut [u8], offset: usize, value: usize) -> BinResult<()> {
    let value = checked_u32(value, offset)?;
    out.get_mut(offset..offset + 4)
        .ok_or_else(|| invalid(offset, "offset patch outside output"))?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}

pub(crate) fn patch_relative(out: &mut [u8], patch: usize, base: usize) -> BinResult<()> {
    let value = out
        .len()
        .checked_sub(base)
        .ok_or_else(|| invalid(base, "negative relative offset"))?;
    patch_u32(out, patch, value)
}
