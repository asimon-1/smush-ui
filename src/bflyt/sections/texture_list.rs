use binrw::{NullString, binrw, file_ptr::parse_from_iter};
use serde::{Deserialize, Serialize};

/// `txl1` resource list whose filename offsets are relative to the byte after the count field.
#[derive(Serialize, Deserialize)]
#[binrw]
#[brw(magic = b"txl1")]
pub struct TextureList {
    #[serde(skip)]
    #[bw(map = |_: &u32| 0xC + 4 * filenames.len() as u32 + filenames.iter().map(|filename| filename.len() + 1).sum::<usize>() as u32)]
    size: u32,
    #[serde(skip)]
    #[bw(map = |_: &u32| filenames.len() as u32)]
    filename_count: u32,
    /// Offsets are relative to the byte immediately after `filename_count`.
    #[br(count = filename_count)]
    #[bw(map = |_: &Vec<u32>| filenames.iter().scan(4 * filenames.len() as u32, |offset, filename| {
        let current = *offset;
        *offset += filename.len() as u32 + 1;
        Some(current)
    }).collect::<Vec<u32>>())]
    #[serde(skip)]
    offsets: Vec<u32>,
    #[br(
    parse_with = parse_from_iter(
        offsets.iter().map(|&offset| {
            offset - 4 * filename_count as u32
        })
    ),
    try_map = |filenames: Vec<NullString>| filenames.into_iter().map(String::try_from).collect::<Result<Vec<_>, _>>()
)]
    #[bw(map = |filenames: &Vec<String>| filenames.iter().map(|filename| NullString::from(filename.as_str())).collect::<Vec<_>>())]
    pub filenames: Vec<String>,
}

impl TextureList {
    pub fn new(filenames: Vec<String>) -> Self {
        Self {
            size: 0,
            filename_count: 0,
            offsets: Vec::new(),
            filenames,
        }
    }
}
