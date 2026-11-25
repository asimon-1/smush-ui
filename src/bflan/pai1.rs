use std::io::{Cursor, Read, Seek, SeekFrom, Write};

use bilge::prelude::*;
use binrw::{
    BinRead, BinReaderExt, BinResult, BinWriterExt, Endian, NullString, VecArgs, binrw,
    file_ptr::parse_from_iter,
};
use serde::{Deserialize, Serialize};

use crate::bflyt::sections::common::{fixed_string_to_string, string_to_fixed_string};

use super::common::{
    align4, bytes_at, checked_u32, invalid, patch_relative, patch_u32, read_cstr, u32_at,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target2_prefix: Option<u32>,
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
    for (index, relative) in offsets.iter().enumerate() {
        let end = offsets.get(index + 1).copied().unwrap_or(size) as usize;
        entries.push(parse_entry(&data, *relative as usize, end)?);
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

fn parse_entry(data: &[u8], start: usize, end: usize) -> BinResult<AnimationEntry> {
    if end > data.len() || end < start {
        return Err(invalid(start, "invalid animation entry bounds"));
    }
    let mut reader = Cursor::new(bytes_at(data, start, end - start)?);
    let EntryTable { header, offsets } = reader.read_le()?;
    let name = header.name;
    let target = header.target;
    let table_end = start + reader.position() as usize;
    let mut tag_starts = Vec::with_capacity(offsets.len());
    for relative in offsets {
        let tag_start = start
            .checked_add(relative as usize)
            .ok_or_else(|| invalid(start, "tag offset overflow"))?;
        if tag_start >= end {
            return Err(invalid(tag_start, "tag outside animation entry"));
        }
        tag_starts.push(tag_start);
    }
    let target2_link = if target == 2 && !tag_starts.is_empty() {
        let first = tag_starts[0];
        if first < table_end {
            return Err(invalid(first, "target-2 tag overlaps offset table"));
        }
        first != table_end
    } else {
        false
    };
    if target == 2 && target2_link {
        let link = u32_at(data, table_end)? as usize;
        let link_pos = start
            .checked_add(link)
            .ok_or_else(|| invalid(start, "target-2 link overflow"))?;
        if link_pos
            .checked_add(4)
            .is_none_or(|position| position > end)
        {
            return Err(invalid(link_pos, "target-2 link outside animation entry"));
        }
    }
    let mut tags = Vec::with_capacity(tag_starts.len());
    for (index, tag_start) in tag_starts.iter().enumerate() {
        let tag_end = tag_starts.get(index + 1).copied().unwrap_or(end);
        tags.push(parse_tag(data, *tag_start, tag_end, target, start)?);
    }
    if target == 2 && target2_link != has_fleu_name(&tags) {
        return Err(invalid(
            start,
            "target-2 link does not match FLEU name metadata",
        ));
    }
    Ok(AnimationEntry { name, target, tags })
}

fn has_fleu_name(tags: &[AnimationTag]) -> bool {
    tags.iter()
        .any(|tag| tag.kind == "FLEU" && tag.tracks.iter().any(|track| track.name.is_some()))
}

fn parse_tag(
    data: &[u8],
    start: usize,
    end: usize,
    target: u8,
    entry_start: usize,
) -> BinResult<AnimationTag> {
    let (tag_start, target2_prefix) = if target == 2 {
        let declared = u32_at(data, start)?;
        let expected = checked_u32(start + 4 - entry_start, start)?;
        (start + 4, (declared != expected).then_some(declared))
    } else {
        (start, None)
    };
    if tag_start
        .checked_add(8)
        .is_none_or(|position| position > end)
    {
        return Err(invalid(tag_start, "tag header outside animation entry"));
    }
    let mut reader = Cursor::new(bytes_at(data, tag_start, end - tag_start)?);
    let TagTable { header, offsets } = reader.read_le()?;
    let kind = std::str::from_utf8(&header.kind)
        .map_err(|_| invalid(tag_start, "invalid animation tag"))?
        .to_owned();
    let mut track_starts = Vec::with_capacity(offsets.len());
    for relative in offsets {
        let track_start = tag_start
            .checked_add(relative as usize)
            .ok_or_else(|| invalid(tag_start, "track offset overflow"))?;
        if track_start >= end {
            return Err(invalid(track_start, "track outside animation tag"));
        }
        track_starts.push(track_start);
    }
    let mut tracks = Vec::with_capacity(track_starts.len());
    for (index, track_start) in track_starts.iter().enumerate() {
        let track_end = track_starts.get(index + 1).copied().unwrap_or(end);
        tracks.push(parse_track(data, *track_start, track_end, &kind)?);
    }
    Ok(AnimationTag {
        kind,
        tracks,
        target2_prefix,
    })
}

fn parse_track(data: &[u8], start: usize, record_end: usize, tag: &str) -> BinResult<Track> {
    let header: TrackHeader = Cursor::new(bytes_at(data, start, 12)?).read_le()?;
    let count = header.key_count as usize;
    let key_start = start
        .checked_add(header.key_offset as usize)
        .ok_or_else(|| invalid(start, "key offset overflow"))?;
    let stride = match header.curve_kind {
        1 => 8,
        2 => 12,
        _ => return Err(invalid(start + 2, "unsupported BFLAN curve type")),
    };
    let key_len = count
        .checked_mul(stride)
        .ok_or_else(|| invalid(start, "key count overflow"))?;
    let end = key_start
        .checked_add(key_len)
        .ok_or_else(|| invalid(start, "key offset overflow"))?;
    if end > record_end {
        return Err(invalid(end, "animation keys exceed track bounds"));
    }
    let mut reader = Cursor::new(bytes_at(data, key_start, key_len)?);
    let curve = match header.curve_kind {
        1 => Curve::Step(reader.read_le_args(VecArgs { count, inner: () })?),
        _ => Curve::Hermite(reader.read_le_args(VecArgs { count, inner: () })?),
    };
    let name = if tag == "FLEU" && end < record_end {
        let name_offset = u32_at(&data[..record_end], end)? as usize;
        let name_start = end
            .checked_add(name_offset)
            .ok_or_else(|| invalid(end, "FLEU name offset overflow"))?;
        Some(read_cstr(&data[..record_end], name_start)?)
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
    let header = EntryHeader {
        name: entry.name.clone(),
        tag_count: u8::try_from(entry.tags.len())
            .map_err(|_| invalid(start, "too many animation tags"))?,
        target: entry.target,
    };
    let mut writer = Cursor::new(Vec::new());
    writer.write_le(&header)?;
    out.extend_from_slice(&writer.into_inner());
    let table = out.len();
    out.resize(out.len() + 4 * entry.tags.len(), 0);
    let link_patch = if entry.target == 2 && has_fleu_name(&entry.tags) {
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
        let prefix = tag
            .target2_prefix
            .unwrap_or(checked_u32(out.len() - entry_start, out.len() - 4)?);
        let end = out.len();
        out[end - 4..end].copy_from_slice(&prefix.to_le_bytes());
    } else if tag.target2_prefix.is_some() {
        return Err(invalid(
            out.len(),
            "target-2 prefix on another animation target",
        ));
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
    match &track.curve {
        Curve::Step(keys) => writer.write_le(keys)?,
        Curve::Hermite(keys) => writer.write_le(keys)?,
    }
    out.extend_from_slice(&writer.into_inner());
    if tag == "FLEU" {
        let Some(name) = track.name.as_deref() else {
            return Ok(None);
        };
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
    #[br(try_map = fixed_string_to_string::<28>)]
    #[bw(try_map = string_to_fixed_string::<28>)]
    name: String,
    tag_count: u8,
    #[brw(pad_after = 2)]
    target: u8,
}

#[derive(BinRead)]
#[br(little)]
struct EntryTable {
    header: EntryHeader,
    #[br(count = header.tag_count)]
    offsets: Vec<u32>,
}

#[binrw]
#[brw(little)]
struct TagHeader {
    kind: [u8; 4],
    #[brw(pad_after = 3)]
    track_count: u8,
}

#[derive(BinRead)]
#[br(little)]
struct TagTable {
    header: TagHeader,
    #[br(count = header.track_count)]
    offsets: Vec<u32>,
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
