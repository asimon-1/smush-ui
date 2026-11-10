use binrw::{
    BinRead, BinReaderExt, BinResult, Endian, Error as BinError, NullString, binread,
    io::TakeSeekExt,
};
use std::io::{Cursor, Read, Seek, SeekFrom};

const SARC_HEADER_SIZE: u64 = 0x14;
const SFAT_HEADER_SIZE: u64 = 0x0c;
const SFNT_HEADER_SIZE: u64 = 0x08;
const SFAT_RECORD_SIZE: u64 = 0x10;
const MAX_ENTRIES: u16 = 0x3fff;
const HASH_MULTIPLIER: u32 = 101;

/// A file stored in a SARC archive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SarcEntry {
    pub hash: u32,
    pub name: Option<String>,
    pub data: Vec<u8>,
}

#[binread]
#[derive(Clone, Debug, PartialEq, Eq)]
#[br(little, magic = b"SARC\x14\x00\xff\xfe")]
pub struct SarcArchive {
    #[br(temp)]
    file_size: u32,
    #[br(temp)]
    data_offset: u32,
    #[br(assert(version == 0x0100, "unsupported SARC version {:#06x}", version))]
    pub version: u16,
    pub padding: u16,
    #[br(parse_with = |reader, endian, _: ()| read_entries(reader, endian, file_size, data_offset))]
    pub entries: Vec<SarcEntry>,
}

#[binread]
#[br(little, magic = b"SFAT\x0c\x00")]
struct Sfat {
    #[br(assert(entry_count <= MAX_ENTRIES, "SFAT entry count exceeds 0x3fff"))]
    entry_count: u16,
    #[br(
        magic = 101u32, // HASH_MULTIPLIER
        count = entry_count
    )]
    records: Vec<SfatRecord>,
}

#[binread]
#[br(little)]
#[derive(Clone, Copy)]
struct SfatRecord {
    hash: u32,
    attributes: u32,
    start: u32,
    end: u32,
}

#[binread]
#[br(import(filename_count: usize), little, magic = b"SFNT\x08\x00\x00\x00")]
struct Sfnt {
    #[br(count = filename_count)]
    filenames: Vec<AlignedFilename>,
}

#[binread]
#[br(little)]
struct AlignedFilename {
    #[br(
        try_map = |value: NullString| String::try_from(value),
        align_after = 4
    )]
    value: String,
}

impl SarcArchive {
    pub fn from_bytes(bytes: &[u8]) -> BinResult<Self> {
        Cursor::new(bytes).read_le()
    }
}

fn read_entries<R: Read + Seek>(
    reader: &mut R,
    _endian: Endian,
    file_size: u32,
    data_offset: u32,
) -> BinResult<Vec<SarcEntry>> {
    let sections_start = reader.stream_position()?;
    let archive_start = sections_start
        .checked_sub(SARC_HEADER_SIZE)
        .ok_or_else(|| sarc_error(sections_start, "invalid SARC header position"))?;
    if u64::from(file_size) < SARC_HEADER_SIZE {
        return Err(sarc_error(
            archive_start + 8,
            "declared archive size is smaller than the SARC header",
        ));
    }
    let archive_end = archive_start
        .checked_add(u64::from(file_size))
        .ok_or_else(|| sarc_error(archive_start + 8, "declared archive size overflows"))?;
    let stream_end = reader.seek(SeekFrom::End(0))?;
    if archive_end > stream_end {
        return Err(sarc_error(
            archive_start + 8,
            "declared archive size exceeds available bytes",
        ));
    }
    reader.seek(SeekFrom::Start(sections_start))?;
    let mut archive = reader.take_seek(archive_end - sections_start);

    let sfat_start = sections_start;
    let sfat = Sfat::read_options(&mut archive, Endian::Little, ())?;
    let records_start = sfat_start + SFAT_HEADER_SIZE;
    let sfnt_start = archive.stream_position()?;
    let data_start = archive_start
        .checked_add(u64::from(data_offset))
        .ok_or_else(|| sarc_error(archive_start + 12, "data offset overflows"))?;
    if !(sfnt_start..=archive_end).contains(&data_start) {
        return Err(sarc_error(
            archive_start + 12,
            "data section overlaps or precedes SFNT",
        ));
    }

    let sfnt_bytes = read_bytes(
        &mut archive,
        data_start - sfnt_start,
        sfnt_start,
        "SFNT section is too large",
    )?;
    let filename_count = sfat
        .records
        .iter()
        .filter(|record| record.attributes >> 24 != 0)
        .count();
    let sfnt = Sfnt::read_options(
        &mut Cursor::new(sfnt_bytes),
        Endian::Little,
        (filename_count,),
    )?;
    let filenames = index_filenames(sfnt.filenames, sfnt_start + SFNT_HEADER_SIZE)?;

    validate_payload_ranges(&sfat.records, records_start, archive_end - data_start)?;
    let mut entries = Vec::with_capacity(usize::from(sfat.entry_count));
    let mut previous_hash = None;
    let mut previous_ordinal = 0u8;

    for (index, record) in sfat.records.iter().copied().enumerate() {
        let record_position = records_start + index as u64 * SFAT_RECORD_SIZE;
        let same_hash = previous_hash == Some(record.hash);
        if previous_hash.is_some_and(|hash| hash > record.hash) {
            return Err(sarc_error(
                record_position,
                "SFAT entries are not sorted by hash",
            ));
        }

        let next_has_same_hash = sfat
            .records
            .get(index + 1)
            .is_some_and(|next| next.hash == record.hash);
        let ordinal = validate_name_attributes(
            record,
            record_position,
            same_hash,
            next_has_same_hash,
            previous_ordinal,
        )?;
        let name = resolve_name(record, record_position, ordinal, &filenames)?;

        let payload_start = data_start + u64::from(record.start);
        let payload_length = u64::from(record.end - record.start);
        archive.seek(SeekFrom::Start(payload_start))?;
        let data = read_bytes(
            &mut archive,
            payload_length,
            record_position + 8,
            "entry payload is too large",
        )?;
        entries.push(SarcEntry {
            hash: record.hash,
            name,
            data,
        });

        previous_hash = Some(record.hash);
        previous_ordinal = ordinal;
    }

    archive.seek(SeekFrom::Start(archive_end))?;
    Ok(entries)
}

fn index_filenames(
    filenames: Vec<AlignedFilename>,
    names_start: u64,
) -> BinResult<Vec<(u32, String)>> {
    let mut indexed = Vec::with_capacity(filenames.len());
    let mut offset = 0usize;
    for filename in filenames {
        offset = align_up(offset, 4);
        let stored_offset = u32::try_from(offset)
            .map_err(|_| sarc_error(names_start, "SFNT filename offset is too large"))?;
        let next_offset = offset
            .checked_add(filename.value.len() + 1)
            .ok_or_else(|| sarc_error(names_start, "SFNT filename table size overflows"))?;
        indexed.push((stored_offset, filename.value));
        offset = next_offset;
    }
    Ok(indexed)
}

fn validate_payload_ranges(
    records: &[SfatRecord],
    records_start: u64,
    data_length: u64,
) -> BinResult<()> {
    let mut ranges = Vec::with_capacity(records.len());
    for (index, record) in records.iter().enumerate() {
        let record_position = records_start + index as u64 * SFAT_RECORD_SIZE;
        if record.start > record.end {
            return Err(sarc_error(
                record_position + 8,
                "entry data start exceeds data end",
            ));
        }
        if u64::from(record.end) > data_length {
            return Err(sarc_error(
                record_position + 12,
                "entry payload exceeds declared archive size",
            ));
        }
        ranges.push((record.start, record.end, index));
    }

    ranges.sort_unstable();
    for pair in ranges.windows(2) {
        let previous = pair[0];
        let current = pair[1];
        if current.0 < previous.1 {
            let record_position = records_start + current.2 as u64 * SFAT_RECORD_SIZE;
            return Err(sarc_error(
                record_position + 8,
                "entry payload ranges overlap",
            ));
        }
    }
    Ok(())
}

fn validate_name_attributes(
    record: SfatRecord,
    record_position: u64,
    same_hash: bool,
    next_has_same_hash: bool,
    previous_ordinal: u8,
) -> BinResult<u8> {
    let ordinal = (record.attributes >> 24) as u8;
    if ordinal == 0 {
        if record.attributes & 0x00ff_ffff != 0 {
            return Err(sarc_error(
                record_position + 4,
                "invalid hash-only filename attributes",
            ));
        }
        if same_hash || next_has_same_hash {
            return Err(sarc_error(
                record_position + 4,
                "hash-only entry is ambiguous within a hash collision",
            ));
        }
        return Ok(ordinal);
    }

    let expected = if same_hash {
        previous_ordinal
            .checked_add(1)
            .ok_or_else(|| sarc_error(record_position + 4, "filename collision ordinal overflow"))?
    } else {
        1
    };
    if ordinal != expected {
        return Err(sarc_error(
            record_position + 4,
            format!("invalid filename collision ordinal {ordinal}, expected {expected}"),
        ));
    }
    Ok(ordinal)
}

fn resolve_name(
    record: SfatRecord,
    record_position: u64,
    ordinal: u8,
    filenames: &[(u32, String)],
) -> BinResult<Option<String>> {
    if ordinal == 0 {
        return Ok(None);
    }

    let byte_offset = (record.attributes & 0x00ff_ffff) << 2;
    let filename_index = filenames
        .binary_search_by_key(&byte_offset, |(offset, _)| *offset)
        .map_err(|_| {
            sarc_error(
                record_position + 4,
                "filename offset is outside SFNT filename table",
            )
        })?;
    let name = filenames[filename_index].1.clone();
    let calculated_hash = filename_hash(&name);
    if calculated_hash != record.hash {
        return Err(sarc_error(
            record_position,
            format!(
                "filename hash mismatch for {name:?}: expected {:#010x}, calculated {calculated_hash:#010x}",
                record.hash
            ),
        ));
    }
    Ok(Some(name))
}

fn read_bytes<R: Read>(
    reader: &mut R,
    length: u64,
    position: u64,
    too_large_message: &str,
) -> BinResult<Vec<u8>> {
    let length =
        usize::try_from(length).map_err(|_| sarc_error(position, too_large_message.to_owned()))?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|error| BinError::Custom {
            pos: position,
            err: Box::new(error),
        })?;
    bytes.resize(length, 0);
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn align_up(size: usize, alignment: usize) -> usize {
    debug_assert!(alignment.is_power_of_two());
    (size + alignment - 1) & !(alignment - 1)
}

fn sarc_error(position: u64, message: impl Into<String>) -> BinError {
    BinError::AssertFail {
        pos: position,
        message: message.into(),
    }
}

fn filename_hash(name: &str) -> u32 {
    name.bytes().fold(0, |hash, byte| {
        let signed_byte = i32::from(byte as i8) as u32;
        hash.wrapping_mul(HASH_MULTIPLIER).wrapping_add(signed_byte)
    })
}
