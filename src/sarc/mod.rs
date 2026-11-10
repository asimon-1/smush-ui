use binrw::{
    BinRead, BinReaderExt, BinResult, BinWrite, BinWriterExt, Endian, Error as BinError,
    NullString, binread, binrw, io::TakeSeekExt,
};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};

const SARC_HEADER_SIZE: u64 = 0x14;
const SFAT_HEADER_SIZE: u64 = 0x0c;
const SFNT_HEADER_SIZE: u64 = 0x08;
const SFAT_RECORD_SIZE: u64 = 0x10;
const MAX_ENTRIES: u16 = 0x3fff;
const HASH_MULTIPLIER: u32 = 101;
const ORDINARY_ALIGNMENT: u64 = 0x10;
const GPU_ALIGNMENT: u64 = 0x1000;

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

#[binrw]
#[brw(little, magic = b"SFAT\x0c\x00")]
struct Sfat {
    #[bw(try_calc = u16::try_from(records.len()))]
    #[br(assert(entry_count <= MAX_ENTRIES, "SFAT entry count exceeds 0x3fff"))]
    entry_count: u16,
    #[br(count = entry_count)]
    #[brw(magic = 101u32)] // HASH_MULTIPLIER
    records: Vec<SfatRecord>,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Copy)]
struct SfatRecord {
    hash: u32,
    attributes: u32,
    start: u32,
    end: u32,
}

#[binrw]
#[br(import(filename_count: usize))]
#[brw(little, magic = b"SFNT\x08\x00\x00\x00")]
struct Sfnt {
    #[br(count = filename_count)]
    filenames: Vec<AlignedFilename>,
}

#[binrw]
#[brw(little)]
struct AlignedFilename {
    #[br(
        try_map = |value: NullString| String::try_from(value)
    )]
    #[bw(map = |value: &String| NullString::from(value.as_str()))]
    #[brw(align_after = 4)]
    value: String,
}

impl SarcArchive {
    /// Creates a new archive with the standard version and zero padding.
    pub fn new(entries: Vec<SarcEntry>) -> Self {
        Self {
            version: 0x0100,
            padding: 0,
            entries,
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> BinResult<Self> {
        Cursor::new(bytes).read_le()
    }

    pub fn to_bytes(&self) -> BinResult<Vec<u8>> {
        let mut writer = Cursor::new(Vec::new());
        writer.write_le(self)?;
        Ok(writer.into_inner())
    }
}

impl SarcEntry {
    /// Creates a named archive entry and calculates its SARC filename hash.
    pub fn new(name: impl Into<String>, data: Vec<u8>) -> Self {
        let name = name.into();
        Self {
            hash: filename_hash(&name),
            name: Some(name),
            data,
        }
    }

    /// Creates an entry whose filename is not present in the archive.
    pub fn hash_only(hash: u32, data: Vec<u8>) -> Self {
        Self {
            hash,
            name: None,
            data,
        }
    }
}

impl BinWrite for SarcArchive {
    type Args<'a> = ();

    fn write_options<W: Write + Seek>(
        &self,
        writer: &mut W,
        _: Endian,
        _: Self::Args<'_>,
    ) -> BinResult<()> {
        let archive_start = writer.stream_position()?;
        let layout = build_write_layout(self, archive_start)?;
        write_archive(writer, archive_start, layout)
    }
}

struct PendingEntry<'a> {
    entry: &'a SarcEntry,
    hash: u32,
}

struct WriteEntryLayout<'a> {
    entry: &'a SarcEntry,
    record: SfatRecord,
}

struct WriteLayout<'a> {
    file_size: u32,
    data_offset: u32,
    version: u16,
    padding: u16,
    sfnt: Sfnt,
    entries: Vec<WriteEntryLayout<'a>>,
}

fn build_write_layout(archive: &SarcArchive, error_position: u64) -> BinResult<WriteLayout<'_>> {
    if archive.version != 0x0100 {
        return Err(sarc_error(
            error_position,
            format!("unsupported SARC version {:#06x}", archive.version),
        ));
    }
    if archive.entries.len() > usize::from(MAX_ENTRIES) {
        return Err(sarc_error(
            error_position,
            "SFAT entry count exceeds 0x3fff",
        ));
    }

    let mut pending = archive
        .entries
        .iter()
        .map(|entry| PendingEntry {
            entry,
            hash: entry.name.as_deref().map_or(entry.hash, filename_hash),
        })
        .collect::<Vec<_>>();
    pending.sort_by_key(|entry| entry.hash);

    let mut filenames = Vec::new();
    let mut names_size = 0u64;
    let mut attributes = Vec::with_capacity(pending.len());
    let mut group_start = 0usize;
    while group_start < pending.len() {
        let hash = pending[group_start].hash;
        let mut group_end = group_start + 1;
        while group_end < pending.len() && pending[group_end].hash == hash {
            group_end += 1;
        }
        let group = &pending[group_start..group_end];
        if group.len() > 1 && group.iter().any(|entry| entry.entry.name.is_none()) {
            return Err(sarc_error(
                error_position,
                format!("hash-only entry is ambiguous within collision group {hash:#010x}"),
            ));
        }
        if group.len() > usize::from(u8::MAX) {
            return Err(sarc_error(
                error_position,
                format!("filename collision group {hash:#010x} exceeds 255 entries"),
            ));
        }

        for (index, pending_entry) in group.iter().enumerate() {
            let Some(name) = pending_entry.entry.name.as_deref() else {
                attributes.push(0);
                continue;
            };
            if name.as_bytes().contains(&0) {
                return Err(sarc_error(
                    error_position,
                    format!("filename {name:?} contains an interior NUL byte"),
                ));
            }
            let name_offset_words = names_size / 4;
            if name_offset_words > 0x00ff_ffff {
                return Err(sarc_error(
                    error_position,
                    "SFNT filename offset exceeds the 24-bit word field",
                ));
            }
            let name_size = u64::try_from(name.len())
                .map_err(|_| sarc_error(error_position, "SFNT filename size is too large"))?
                .checked_add(1)
                .ok_or_else(|| sarc_error(error_position, "SFNT filename size overflows"))?;
            names_size = align_up_checked(
                names_size
                    .checked_add(name_size)
                    .ok_or_else(|| sarc_error(error_position, "SFNT filename table overflows"))?,
                4,
                error_position,
            )?;
            filenames.push(AlignedFilename {
                value: name.to_owned(),
            });

            let ordinal = u32::try_from(index + 1)
                .map_err(|_| sarc_error(error_position, "filename collision ordinal overflows"))?;
            attributes.push((ordinal << 24) | name_offset_words as u32);
        }
        group_start = group_end;
    }

    let records_size = (pending.len() as u64)
        .checked_mul(SFAT_RECORD_SIZE)
        .ok_or_else(|| sarc_error(error_position, "SFAT record table size overflows"))?;
    let sections_size = SARC_HEADER_SIZE
        .checked_add(SFAT_HEADER_SIZE)
        .and_then(|size| size.checked_add(records_size))
        .and_then(|size| size.checked_add(SFNT_HEADER_SIZE))
        .and_then(|size| size.checked_add(names_size))
        .ok_or_else(|| sarc_error(error_position, "SARC section sizes overflow"))?;
    let strongest_alignment = pending
        .iter()
        .map(|entry| entry_alignment(entry.entry.name.as_deref()))
        .max()
        .unwrap_or(ORDINARY_ALIGNMENT);

    let data_offset = u32::try_from(align_up_checked(
        sections_size,
        strongest_alignment,
        error_position,
    )?)
    .map_err(|_| sarc_error(error_position, "data-section offset exceeds 32 bits"))?;

    let mut entries = Vec::with_capacity(pending.len());
    let mut payload_position = u64::from(data_offset);
    for (pending_entry, attributes) in pending.iter().zip(attributes) {
        payload_position = align_up_checked(
            payload_position,
            entry_alignment(pending_entry.entry.name.as_deref()),
            error_position,
        )?;
        let start = payload_position
            .checked_sub(u64::from(data_offset))
            .ok_or_else(|| sarc_error(error_position, "payload offset underflows"))?;
        let data_length = u64::try_from(pending_entry.entry.data.len())
            .map_err(|_| sarc_error(error_position, "entry payload size is too large"))?;
        payload_position = payload_position
            .checked_add(data_length)
            .ok_or_else(|| sarc_error(error_position, "entry payload end overflows"))?;
        let end = payload_position
            .checked_sub(u64::from(data_offset))
            .ok_or_else(|| sarc_error(error_position, "payload end underflows"))?;
        entries.push(WriteEntryLayout {
            entry: pending_entry.entry,
            record: SfatRecord {
                hash: pending_entry.hash,
                attributes,
                start: u32::try_from(start).map_err(|_| {
                    sarc_error(error_position, "entry payload offset exceeds 32 bits")
                })?,
                end: u32::try_from(end)
                    .map_err(|_| sarc_error(error_position, "entry payload end exceeds 32 bits"))?,
            },
        });
    }
    let file_size = u32::try_from(payload_position)
        .map_err(|_| sarc_error(error_position, "archive size exceeds 32 bits"))?;

    Ok(WriteLayout {
        file_size,
        data_offset,
        version: archive.version,
        padding: archive.padding,
        sfnt: Sfnt { filenames },
        entries,
    })
}

fn write_archive<W: Write + Seek>(
    writer: &mut W,
    archive_start: u64,
    layout: WriteLayout<'_>,
) -> BinResult<()> {
    writer.write_all(b"SARC\x14\x00\xff\xfe")?;
    writer.write_le(&layout.file_size)?;
    writer.write_le(&layout.data_offset)?;
    writer.write_le(&layout.version)?;
    writer.write_le(&layout.padding)?;
    writer.write_le(&Sfat {
        records: layout.entries.iter().map(|entry| entry.record).collect(),
    })?;
    let mut sfnt_writer = Cursor::new(Vec::new());
    sfnt_writer.write_le(&layout.sfnt)?;
    writer.write_all(sfnt_writer.get_ref())?;
    write_zeros_until(
        writer,
        archive_start
            .checked_add(u64::from(layout.data_offset))
            .ok_or_else(|| sarc_error(archive_start, "data-section position overflows"))?,
    )?;

    for entry in &layout.entries {
        let payload_position = archive_start
            .checked_add(u64::from(layout.data_offset))
            .and_then(|position| position.checked_add(u64::from(entry.record.start)))
            .ok_or_else(|| sarc_error(archive_start, "payload position overflows"))?;
        write_zeros_until(writer, payload_position)?;
        writer.write_all(&entry.entry.data)?;
    }
    write_zeros_until(
        writer,
        archive_start
            .checked_add(u64::from(layout.file_size))
            .ok_or_else(|| sarc_error(archive_start, "archive end position overflows"))?,
    )?;
    Ok(())
}

fn write_zeros_until<W: Write + Seek>(writer: &mut W, target: u64) -> BinResult<()> {
    let position = writer.stream_position()?;
    if position > target {
        return Err(sarc_error(position, "SARC sections or payloads overlap"));
    }
    let mut remaining = target - position;
    let zeros = [0u8; 4096];
    while remaining != 0 {
        let length = remaining.min(zeros.len() as u64) as usize;
        writer.write_all(&zeros[..length])?;
        remaining -= length as u64;
    }
    Ok(())
}

fn entry_alignment(name: Option<&str>) -> u64 {
    match name.and_then(|name| name.rsplit_once('.').map(|(_, extension)| extension)) {
        Some(extension)
            if extension.eq_ignore_ascii_case("bntx") || extension.eq_ignore_ascii_case("bnsh") =>
        {
            GPU_ALIGNMENT
        }
        _ => ORDINARY_ALIGNMENT,
    }
}

fn align_up_checked(value: u64, alignment: u64, position: u64) -> BinResult<u64> {
    debug_assert!(alignment.is_power_of_two());
    value
        .checked_add(alignment - 1)
        .map(|value| value & !(alignment - 1))
        .ok_or_else(|| sarc_error(position, "aligned SARC offset overflows"))
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
    let mut filenames = index_filenames(sfnt.filenames, sfnt_start + SFNT_HEADER_SIZE)?.into_iter();

    validate_payload_ranges(&sfat.records, records_start, archive_end - data_start)?;
    let mut entries = Vec::with_capacity(sfat.records.len());
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
        let filename = if ordinal == 0 { None } else { filenames.next() };
        let name = resolve_name(record, record_position, ordinal, filename)?;

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
    let mut offset = 0u64;
    for filename in filenames {
        offset = align_up_checked(offset, 4, names_start)?;
        let stored_offset = u32::try_from(offset)
            .map_err(|_| sarc_error(names_start, "SFNT filename offset is too large"))?;
        let filename_size = u64::try_from(filename.value.len())
            .map_err(|_| sarc_error(names_start, "SFNT filename is too large"))?
            .checked_add(1)
            .ok_or_else(|| sarc_error(names_start, "SFNT filename size overflows"))?;
        let next_offset = offset
            .checked_add(filename_size)
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
    filename: Option<(u32, String)>,
) -> BinResult<Option<String>> {
    if ordinal == 0 {
        return Ok(None);
    }

    let byte_offset = (record.attributes & 0x00ff_ffff) << 2;
    let (expected_offset, name) = filename
        .ok_or_else(|| sarc_error(record_position + 4, "missing compact SFNT filename record"))?;
    if byte_offset != expected_offset {
        return Err(sarc_error(
            record_position + 4,
            "SFNT filenames must be a compact array in SFAT record order",
        ));
    }
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
