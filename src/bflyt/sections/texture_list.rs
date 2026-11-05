use binrw::{NullString, binrw, file_ptr::parse_from_iter};

/// `txl1` resource list whose filename offsets are relative to the byte after the count field.
#[binrw]
#[brw(magic = b"txl1")]
pub struct TextureList {
    #[bw(calc = 0xC + 4 * filenames.len() as u32 + filenames.iter().map(|filename| filename.len() + 1).sum::<usize>() as u32)]
    pub size: u32,
    #[bw(calc = filenames.len() as u32)]
    pub filename_count: u32,
    /// Offsets are relative to the byte immediately after `filename_count`.
    #[br(count = filename_count)]
    #[bw(calc = filenames.iter().scan(4 * filenames.len() as u32, |offset, filename| {
        let current = *offset;
        *offset += filename.len() as u32 + 1;
        Some(current)
    }).collect())]
    pub offsets: Vec<u32>,
    #[br(
    parse_with = parse_from_iter(
        offsets.iter().map(|&offset| {
            offset - 4 * filename_count as u32
        })
    )
)]
    pub filenames: Vec<NullString>,
}
