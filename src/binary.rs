use std::io::{Read, Seek, Write};

use binrw::{BinRead, BinResult, BinWrite, Endian};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BinRead, BinWrite)]
pub struct FileHeader {
    pub magic: [u8; 4],
    pub byte_order_mark: u16,
    pub header_size: u16,
    pub version: u32,
    pub file_size: u32,
    pub section_count: u16,
    pub padding: u16,
}

impl FileHeader {
    pub fn read_with_endian<R: Read + Seek>(reader: &mut R, endian: Endian) -> BinResult<Self> {
        Self::read_options(reader, endian, ())
    }

    pub fn write_with_endian<W: Write + Seek>(
        &self,
        writer: &mut W,
        endian: Endian,
    ) -> BinResult<()> {
        self.write_options(writer, endian, ())
    }

    pub fn has_magic(&self, expected: &[u8; 4]) -> bool {
        &self.magic == expected
    }
}
