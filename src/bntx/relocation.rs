use super::{
    binary::{RelocationEntryDisk, RelocationSectionDisk},
    bntx_error,
};
use binrw::BinResult;

pub(crate) struct Table {
    pub sections: [RelocationSectionDisk; 2],
    pub entries: Vec<RelocationEntryDisk>,
}

#[derive(Clone, Copy)]
pub(crate) struct CanonicalLayout<'a> {
    pub pointer_table: u64,
    pub dictionary: u64,
    pub texture_offsets: &'a [u64],
    pub mip_tables: &'a [u64],
    pub metadata_end: u64,
    pub brtd: u64,
    pub relocation: u64,
}

pub(crate) fn build(
    layout: CanonicalLayout<'_>,
    metadata_pointers: &[u64],
    data_pointers: &[u64],
) -> BinResult<Table> {
    if let Some(table) = canonical_table(layout)? {
        return Ok(table);
    }

    let metadata_entries = consecutive_entries(metadata_pointers)?;
    let data_entries = consecutive_entries(data_pointers)?;
    finish_table(layout, metadata_entries, data_entries)
}

fn canonical_table(layout: CanonicalLayout<'_>) -> BinResult<Option<Table>> {
    let texture_count = layout.texture_offsets.len();
    if texture_count == 0 || texture_count != layout.mip_tables.len() || texture_count > 255 {
        return Ok(None);
    }
    if layout.metadata_end != layout.texture_offsets[texture_count - 1] + 0x2a8 {
        return Ok(None);
    }
    if layout
        .texture_offsets
        .windows(2)
        .any(|pair| pair[1] - pair[0] != 0x2a8)
        || layout
            .texture_offsets
            .iter()
            .zip(layout.mip_tables)
            .any(|(&texture, &mip)| mip != texture + 0x2a0)
    {
        return Ok(None);
    }
    if texture_count > 1 && layout.dictionary + 0x10 - 0x28 > 256 * 8 {
        return Ok(None);
    }

    let count = u16::try_from(texture_count)
        .map_err(|_| bntx_error(0, "BNTX relocation array count exceeds 65535"))?;
    let mut metadata = Vec::new();
    let mut data = Vec::new();
    if texture_count == 1 {
        metadata.push(entry_with_next(0x28, layout.pointer_table, 1)?);
        metadata.push(entry_with_next(0x38, layout.texture_offsets[0] + 0x80, 2)?);
        metadata.push(RelocationEntryDisk {
            offset: to_u32(layout.dictionary + 0x10, "dictionary relocation")?,
            array_count: 2,
            pointer_count: 1,
            padding_count: 1,
        });
        metadata.push(RelocationEntryDisk {
            offset: to_u32(layout.texture_offsets[0] + 0x60, "texture relocation")?,
            array_count: 1,
            pointer_count: 3,
            padding_count: 0,
        });
        data.push(entry_with_next(0x30, layout.mip_tables[0], 1)?);
    } else {
        metadata.push(entry_with_next(0x28, layout.dictionary + 0x10, 1)?);
        metadata.push(RelocationEntryDisk {
            offset: 0x38,
            array_count: 1,
            pointer_count: 2,
            padding_count: 0,
        });
        metadata.push(RelocationEntryDisk {
            offset: to_u32(layout.pointer_table, "texture pointer relocation")?,
            array_count: 1,
            pointer_count: texture_count as u8,
            padding_count: 0,
        });
        metadata.push(RelocationEntryDisk {
            offset: to_u32(layout.dictionary + 0x20, "dictionary relocation")?,
            array_count: count,
            pointer_count: 1,
            padding_count: 1,
        });
        metadata.push(repeated_texture_entry(
            layout.texture_offsets[0] + 0x60,
            count,
            3,
        )?);
        metadata.push(repeated_texture_entry(
            layout.texture_offsets[0] + 0x80,
            count,
            2,
        )?);
        data.push(RelocationEntryDisk {
            offset: 0x30,
            array_count: 1,
            pointer_count: 1,
            padding_count: 0,
        });
        data.push(repeated_texture_entry(layout.mip_tables[0], count, 1)?);
    }
    Ok(Some(finish_table(layout, metadata, data)?))
}

fn repeated_texture_entry(
    offset: u64,
    count: u16,
    pointer_count: u8,
) -> BinResult<RelocationEntryDisk> {
    let stride_words = 0x2a8usize / 8;
    Ok(RelocationEntryDisk {
        offset: to_u32(offset, "repeated texture relocation")?,
        array_count: count,
        pointer_count,
        padding_count: u8::try_from(stride_words - usize::from(pointer_count))
            .map_err(|_| bntx_error(offset, "BNTX relocation padding exceeds 255 words"))?,
    })
}

fn entry_with_next(offset: u64, next: u64, pointer_count: u8) -> BinResult<RelocationEntryDisk> {
    let byte_distance = next
        .checked_sub(offset)
        .filter(|distance| distance.is_multiple_of(8))
        .ok_or_else(|| bntx_error(offset, "BNTX relocation chunks are not word aligned"))?;
    let stride_words = byte_distance / 8;
    let padding = stride_words
        .checked_sub(u64::from(pointer_count))
        .ok_or_else(|| bntx_error(offset, "BNTX relocation chunks overlap"))?;
    Ok(RelocationEntryDisk {
        offset: to_u32(offset, "relocation entry")?,
        array_count: 2,
        pointer_count,
        padding_count: u8::try_from(padding)
            .map_err(|_| bntx_error(offset, "BNTX relocation padding exceeds 255 words"))?,
    })
}

fn consecutive_entries(positions: &[u64]) -> BinResult<Vec<RelocationEntryDisk>> {
    let mut positions = positions.to_vec();
    positions.sort_unstable();
    positions.dedup();
    let mut entries = Vec::new();
    let mut index = 0;
    while index < positions.len() {
        let start = positions[index];
        let mut count = 1usize;
        while count < 255
            && index + count < positions.len()
            && positions[index + count] == start + count as u64 * 8
        {
            count += 1;
        }
        entries.push(RelocationEntryDisk {
            offset: to_u32(start, "relocation pointer")?,
            array_count: 1,
            pointer_count: count as u8,
            padding_count: 0,
        });
        index += count;
    }
    Ok(entries)
}

fn finish_table(
    layout: CanonicalLayout<'_>,
    metadata_entries: Vec<RelocationEntryDisk>,
    data_entries: Vec<RelocationEntryDisk>,
) -> BinResult<Table> {
    let metadata_count = u32::try_from(metadata_entries.len())
        .map_err(|_| bntx_error(layout.relocation, "too many BNTX metadata relocations"))?;
    let data_count = u32::try_from(data_entries.len())
        .map_err(|_| bntx_error(layout.relocation, "too many BNTX data relocations"))?;
    // _RLT groups pointers into metadata (headers, dictionaries, BRTI) and
    // texture data (BRTD mip-pointer tables) sections with independent entries.
    let sections = [
        RelocationSectionDisk {
            file_offset: 0,
            file_size: to_u32(layout.metadata_end, "metadata relocation section")?,
            first_entry_index: 0,
            entry_count: metadata_count,
        },
        RelocationSectionDisk {
            file_offset: to_u32(layout.brtd, "data relocation section")?,
            file_size: to_u32(layout.relocation - layout.brtd, "data relocation section")?,
            first_entry_index: metadata_count,
            entry_count: data_count,
        },
    ];
    let mut entries = metadata_entries;
    entries.extend(data_entries);
    Ok(Table { sections, entries })
}

fn to_u32(value: u64, label: &str) -> BinResult<u32> {
    u32::try_from(value).map_err(|_| bntx_error(value, format!("BNTX {label} exceeds 4 GiB")))
}
