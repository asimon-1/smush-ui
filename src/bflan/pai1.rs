use std::io::{Cursor, Read, Seek, SeekFrom, Write};

use bilge::prelude::*;
use binrw::{
    BinReaderExt, BinResult, BinWriterExt, Endian, NullString, binrw, file_ptr::parse_from_iter,
};
use serde::{Deserialize, Serialize};

use super::{
    align4, bytes_at, checked_u32, invalid, patch_relative, patch_u32, read_cstr, read_fixed,
    u32_at, write_fixed,
};

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StepKey {
    #[serde(with = "crate::float")]
    pub frame: f32,
    pub value: i16,
    pub reserved: u16,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HermiteKey {
    #[serde(with = "crate::float")]
    pub frame: f32,
    #[serde(with = "crate::float")]
    pub value: f32,
    #[serde(with = "crate::float")]
    pub slope: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Curve {
    Step(Vec<StepKey>),
    Hermite(Vec<HermiteKey>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub index: u8,
    pub target: u8,
    pub curve: Curve,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnimationTag {
    pub kind: String,
    pub tracks: Vec<Track>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnimationEntry {
    pub name: String,
    pub target: u8,
    pub tags: Vec<AnimationTag>,
}

#[bitsize(8, new = pub)]
#[derive(
    Clone, Copy, DebugBits, DefaultBits, DeserializeBits, Eq, FromBits, PartialEq, SerializeBits,
)]
pub struct PaiFlags {
    pub loop_enabled: bool,
    pub reserved: u7,
}

#[binrw]
#[brw(little, magic = b"pai1")]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pai1 {
    #[br(temp)]
    #[bw(calc = pai1_size(textures, entries)?)]
    size: u32,
    pub frame_count: u16,
    #[br(map = |value: u8| PaiFlags::from(u8::new(value)))]
    #[bw(map = |value: &PaiFlags| u8::from(*value).value())]
    #[brw(pad_after = 1)]
    pub flags: PaiFlags,
    #[br(temp)]
    #[bw(calc = u16::try_from(textures.len()).map_err(|_| invalid(12, "too many PAI1 textures"))?)]
    texture_count: u16,
    #[br(temp)]
    #[bw(calc = u16::try_from(entries.len()).map_err(|_| invalid(14, "too many PAI1 entries"))?)]
    entry_count: u16,
    #[br(temp)]
    #[bw(calc = pai1_entry_table(textures, entries)?)]
    entry_table: u32,
    #[br(temp, count = texture_count, assert(texture_offsets.iter().all(|&offset| offset >= 4 * texture_count as u32), "PAI1 texture offset precedes names"))]
    #[bw(calc = texture_offsets(textures)?)]
    texture_offsets: Vec<u32>,
    #[br(parse_with = parse_from_iter(texture_offsets.iter().map(|&offset| offset - 4 * texture_count as u32)), try_map = |textures: Vec<NullString>| textures.into_iter().map(String::try_from).collect::<Result<Vec<_>, _>>(), align_after = 4)]
    #[bw(map = |textures: &Vec<String>| textures.iter().map(|texture| NullString::from(texture.as_str())).collect::<Vec<_>>(), align_after = 4)]
    pub textures: Vec<String>,
    #[br(temp, count = entry_count, assert((entry_count == 0 && entry_table == 0) || (entry_count != 0 && entry_table as usize == texture_end(&textures)?), "invalid PAI1 entry table offset"))]
    #[bw(calc = entry_offsets(entries, entry_table as usize)?)]
    entry_offsets: Vec<u32>,
    #[br(parse_with = read_entries, args(size, entry_offsets, texture_end(&textures)?))]
    #[bw(write_with = write_entries)]
    pub entries: Vec<AnimationEntry>,
}

fn pai1_entry_table(textures: &[String], entries: &[AnimationEntry]) -> BinResult<u32> {
    if entries.is_empty() {
        Ok(0)
    } else {
        checked_u32(texture_end(textures)?, 16)
    }
}

fn pai1_size(textures: &[String], entries: &[AnimationEntry]) -> BinResult<u32> {
    let mut out = vec![0; texture_end(textures)?];
    out.resize(out.len() + 4 * entries.len(), 0);
    for entry in entries {
        write_entry(&mut out, entry)?;
    }
    checked_u32(out.len(), 4)
}

fn texture_end(textures: &[String]) -> BinResult<usize> {
    let table_end = 20usize
        .checked_add(4 * textures.len())
        .ok_or_else(|| invalid(12, "PAI1 texture table overflow"))?;
    let end = textures.iter().try_fold(table_end, |size, texture| {
        size.checked_add(texture.len() + 1)
            .ok_or_else(|| invalid(12, "PAI1 texture size overflow"))
    })?;
    Ok(if textures.is_empty() {
        end
    } else {
        (end + 3) & !3
    })
}

fn texture_offsets(textures: &[String]) -> BinResult<Vec<u32>> {
    let mut offset = 4usize * textures.len();
    textures
        .iter()
        .map(|texture| {
            let current = checked_u32(offset, 20)?;
            offset = offset
                .checked_add(texture.len() + 1)
                .ok_or_else(|| invalid(20, "PAI1 texture offset overflow"))?;
            Ok(current)
        })
        .collect()
}

fn entry_offsets(entries: &[AnimationEntry], table: usize) -> BinResult<Vec<u32>> {
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = vec![0; table + 4 * entries.len()];
    entries
        .iter()
        .map(|entry| {
            let offset = checked_u32(out.len(), table)?;
            write_entry(&mut out, entry)?;
            Ok(offset)
        })
        .collect()
}

fn section_bytes<R: Read + Seek>(reader: &mut R, start: u64, size: u32) -> BinResult<Vec<u8>> {
    let end = start
        .checked_add(size as u64)
        .ok_or_else(|| invalid(4, "PAI1 section size overflow"))?;
    if reader.seek(SeekFrom::End(0))? < end {
        return Err(invalid(4, "PAI1 section size mismatch"));
    }
    reader.seek(SeekFrom::Start(start))?;
    let mut data = vec![0; size as usize];
    reader.read_exact(&mut data)?;
    Ok(data)
}

fn read_entries<R: Read + Seek>(
    reader: &mut R,
    _: Endian,
    (size, offsets, texture_end): (u32, Vec<u32>, usize),
) -> BinResult<Vec<AnimationEntry>> {
    let table_end = texture_end + 4 * offsets.len();
    let start = reader
        .stream_position()?
        .checked_sub(table_end as u64)
        .ok_or_else(|| invalid(16, "PAI1 entry data precedes section"))?;
    if offsets.is_empty() {
        if size as usize != texture_end {
            return Err(invalid(4, "PAI1 size mismatch"));
        }
        return Ok(Vec::new());
    }
    let data = section_bytes(reader, start, size)?;
    let mut entries = Vec::with_capacity(offsets.len());
    for relative in offsets {
        entries.push(parse_entry(&data, relative as usize)?);
    }
    Ok(entries)
}

#[allow(clippy::ptr_arg)] // binrw passes the Vec field directly.
fn write_entries<W: Write + Seek>(
    entries: &Vec<AnimationEntry>,
    writer: &mut W,
    _: Endian,
    _: (),
) -> BinResult<()> {
    if entries.is_empty() {
        return Ok(());
    }
    let mut out = Vec::new();
    for entry in entries {
        write_entry(&mut out, entry)?;
    }
    writer.write_all(&out)?;
    Ok(())
}

fn offset(base: usize, index: usize, stride: usize) -> BinResult<usize> {
    base.checked_add(
        index
            .checked_mul(stride)
            .ok_or_else(|| invalid(base, "offset overflow"))?,
    )
    .ok_or_else(|| invalid(base, "offset overflow"))
}

fn parse_entry(data: &[u8], start: usize) -> BinResult<AnimationEntry> {
    let header: EntryHeader = Cursor::new(bytes_at(data, start, 32)?).read_le()?;
    let name = read_fixed(&header.name, 0, 28)?;
    let tag_count = header.tag_count as usize;
    let target = header.target;
    let mut tags = Vec::with_capacity(tag_count);
    for i in 0..tag_count {
        let relative = u32_at(data, offset(start + 32, i, 4)?)? as usize;
        let tag_start = start
            .checked_add(relative)
            .ok_or_else(|| invalid(start, "tag offset overflow"))?;
        tags.push(parse_tag(data, tag_start, target, start)?);
    }
    if target == 2 {
        let link = u32_at(data, offset(start + 32, tag_count, 4)?)? as usize;
        let link_pos = start
            .checked_add(link)
            .ok_or_else(|| invalid(start, "target-2 link overflow"))?;
        bytes_at(data, link_pos, 4)?;
    }
    Ok(AnimationEntry { name, target, tags })
}

fn parse_tag(data: &[u8], start: usize, target: u8, entry_start: usize) -> BinResult<AnimationTag> {
    let tag_start = if target == 2 {
        let declared = u32_at(data, start)? as usize;
        if declared != start + 4 - entry_start {
            return Err(invalid(start, "target-2 tag link does not point to tag"));
        }
        start + 4
    } else {
        start
    };
    let header: TagHeader = Cursor::new(bytes_at(data, tag_start, 8)?).read_le()?;
    let kind = std::str::from_utf8(&header.kind)
        .map_err(|_| invalid(tag_start, "invalid animation tag"))?
        .to_owned();
    let count = header.track_count as usize;
    let mut tracks = Vec::with_capacity(count);
    for i in 0..count {
        let relative = u32_at(data, offset(tag_start + 8, i, 4)?)? as usize;
        let track_start = tag_start
            .checked_add(relative)
            .ok_or_else(|| invalid(tag_start, "track offset overflow"))?;
        tracks.push(parse_track(data, track_start, &kind)?);
    }
    Ok(AnimationTag { kind, tracks })
}

fn parse_track(data: &[u8], start: usize, tag: &str) -> BinResult<Track> {
    let header: TrackHeader = Cursor::new(bytes_at(data, start, 12)?).read_le()?;
    let count = header.key_count as usize;
    let key_offset = header.key_offset as usize;
    let key_start = start
        .checked_add(key_offset)
        .ok_or_else(|| invalid(start, "key offset overflow"))?;
    let curve = match header.curve_kind {
        1 => {
            let mut keys = Vec::with_capacity(count.min(data.len() / 8));
            for i in 0..count {
                let pos = offset(key_start, i, 8)?;
                keys.push(Cursor::new(bytes_at(data, pos, 8)?).read_le()?);
            }
            Curve::Step(keys)
        }
        2 => {
            let mut keys = Vec::with_capacity(count.min(data.len() / 12));
            for i in 0..count {
                let pos = offset(key_start, i, 12)?;
                keys.push(Cursor::new(bytes_at(data, pos, 12)?).read_le()?);
            }
            Curve::Hermite(keys)
        }
        _ => return Err(invalid(start + 2, "unsupported BFLAN curve type")),
    };
    let key_len = count
        .checked_mul(if header.curve_kind == 1 { 8 } else { 12 })
        .ok_or_else(|| invalid(start, "key count overflow"))?;
    let end = key_start
        .checked_add(key_len)
        .ok_or_else(|| invalid(start, "key offset overflow"))?;
    let name = if tag == "FLEU" {
        let name_offset = u32_at(data, end)? as usize;
        let name_start = end
            .checked_add(name_offset)
            .ok_or_else(|| invalid(end, "FLEU name offset overflow"))?;
        Some(read_cstr(data, name_start)?)
    } else {
        None
    };
    Ok(Track {
        index: header.index,
        target: header.target,
        curve,
        name,
    })
}

fn write_entry(out: &mut Vec<u8>, entry: &AnimationEntry) -> BinResult<()> {
    let start = out.len();
    let mut name = Vec::new();
    write_fixed(&mut name, &entry.name, 28)?;
    let header = EntryHeader {
        name: name.try_into().unwrap(),
        tag_count: u8::try_from(entry.tags.len())
            .map_err(|_| invalid(start, "too many animation tags"))?,
        target: entry.target,
    };
    let mut writer = Cursor::new(Vec::new());
    writer.write_le(&header)?;
    out.extend_from_slice(&writer.into_inner());
    let table = out.len();
    out.resize(out.len() + 4 * entry.tags.len(), 0);
    let link_patch = if entry.target == 2 {
        let patch = out.len();
        out.extend_from_slice(&0u32.to_le_bytes());
        Some(patch)
    } else {
        None
    };
    let mut link_target = None;
    for (i, tag) in entry.tags.iter().enumerate() {
        patch_relative(out, table + 4 * i, start)?;
        link_target = write_tag(out, tag, entry.target, start)?.or(link_target);
    }
    if let Some(patch) = link_patch {
        let target =
            link_target.ok_or_else(|| invalid(start, "target-2 entry needs FLEU metadata"))?;
        patch_u32(out, patch, target - start)?;
    }
    Ok(())
}

fn write_tag(
    out: &mut Vec<u8>,
    tag: &AnimationTag,
    target: u8,
    entry_start: usize,
) -> BinResult<Option<usize>> {
    if tag.kind.len() != 4 || !tag.kind.is_ascii() {
        return Err(invalid(
            out.len(),
            "animation tag must have four ASCII bytes",
        ));
    }
    if target == 2 {
        out.extend_from_slice(&0u32.to_le_bytes());
        let prefix = checked_u32(out.len() - entry_start, out.len() - 4)?;
        let end = out.len();
        out[end - 4..end].copy_from_slice(&prefix.to_le_bytes());
    }
    let start = out.len();
    let header = TagHeader {
        kind: tag.kind.as_bytes().try_into().unwrap(),
        track_count: u8::try_from(tag.tracks.len())
            .map_err(|_| invalid(start, "too many tracks"))?,
    };
    let mut writer = Cursor::new(Vec::new());
    writer.write_le(&header)?;
    out.extend_from_slice(&writer.into_inner());
    let table = out.len();
    out.resize(out.len() + 4 * tag.tracks.len(), 0);
    let mut link_target = None;
    for (i, track) in tag.tracks.iter().enumerate() {
        patch_relative(out, table + 4 * i, start)?;
        link_target = write_track(out, track, &tag.kind)?.or(link_target);
    }
    Ok(link_target)
}

fn write_track(out: &mut Vec<u8>, track: &Track, tag: &str) -> BinResult<Option<usize>> {
    let (curve_kind, count) = match &track.curve {
        Curve::Step(keys) => (1u8, keys.len()),
        Curve::Hermite(keys) => (2u8, keys.len()),
    };
    let header = TrackHeader {
        index: track.index,
        target: track.target,
        curve_kind,
        key_count: u16::try_from(count).map_err(|_| invalid(out.len(), "too many keyframes"))?,
        key_offset: 12,
    };
    let mut writer = Cursor::new(Vec::new());
    writer.write_le(&header)?;
    out.extend_from_slice(&writer.into_inner());
    match &track.curve {
        Curve::Step(keys) => {
            for key in keys {
                writer = Cursor::new(Vec::new());
                writer.write_le(key)?;
                out.extend_from_slice(&writer.into_inner());
            }
        }
        Curve::Hermite(keys) => {
            for key in keys {
                writer = Cursor::new(Vec::new());
                writer.write_le(key)?;
                out.extend_from_slice(&writer.into_inner());
            }
        }
    }
    if tag == "FLEU" {
        let name = track
            .name
            .as_deref()
            .ok_or_else(|| invalid(out.len(), "FLEU track needs a name"))?;
        let link_target = out.len();
        out.extend_from_slice(&4u32.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.push(0);
        align4(out);
        Ok(Some(link_target))
    } else if track.name.is_some() {
        Err(invalid(out.len(), "only FLEU tracks have names"))
    } else {
        Ok(None)
    }
}

#[binrw]
#[brw(little)]
struct EntryHeader {
    name: [u8; 28],
    tag_count: u8,
    #[brw(pad_after = 2)]
    target: u8,
}

#[binrw]
#[brw(little)]
struct TagHeader {
    kind: [u8; 4],
    #[brw(pad_after = 3)]
    track_count: u8,
}

#[binrw]
#[brw(little)]
struct TrackHeader {
    index: u8,
    target: u8,
    #[brw(pad_after = 1)]
    curve_kind: u8,
    #[brw(pad_after = 2)]
    key_count: u16,
    key_offset: u32,
}
