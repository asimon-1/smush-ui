use binrw::{BinResult, Endian, binrw};
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, Write};

use super::common::{
    align_up, fixed_string_to_string, read_null_string_at, string_to_fixed_string,
};
use super::{PaneInfo, Vec2};

#[derive(Serialize, Deserialize)]
#[binrw]
pub struct PartsProperty {
    #[br(try_map = fixed_string_to_string::<24>)]
    #[bw(try_map = string_to_fixed_string::<24>)]
    pub name: String,
    pub usage_flags: u8,
    pub basic_usage_flags: u8,
    pub material_usage_flags: u8,
    pub system_ext_user_data_override_flags: u8,
    pub property_offset: u32,
    pub ext_user_data_offset: u32,
    pub pane_basic_info_offset: u32,
}

#[derive(Serialize, Deserialize)]
#[binrw]
#[brw(magic = b"prt1")]
pub struct Parts {
    #[serde(skip)]
    #[bw(map = |_: &u32| 0x60 + 40 * properties.len() as u32 + data.serialized_size() as u32)]
    size: u32,
    pub pane: PaneInfo,
    #[bw(map = |_: &u32| properties.len() as u32)]
    #[serde(skip)]
    property_count: u32,
    pub magnify: Vec2,
    #[br(count = property_count)]
    pub properties: Vec<PartsProperty>,
    #[br(parse_with = read_parts_data, args(size, property_count))]
    #[bw(write_with = write_parts_data)]
    pub data: PartsData,
}

/// The variable `prt1` payload. Referenced pane and complement subsections remain raw.
#[derive(Serialize, Deserialize)]
pub struct PartsData {
    pub part_name: String,
    pub referenced_data: Vec<u8>,
}
impl PartsData {
    fn serialized_size(&self) -> usize {
        align_up(self.part_name.len() + 1, 4) + self.referenced_data.len()
    }
}

fn read_parts_data<R: Read + Seek>(
    reader: &mut R,
    endian: Endian,
    (size, property_count): (u32, u32),
) -> BinResult<PartsData> {
    let payload_size = (size - 0x60 - 40 * property_count) as usize;
    let mut payload = vec![0; payload_size];
    reader.read_exact(&mut payload)?;
    let part_name = read_null_string_at(&payload, 0, endian)?;
    let name_size = align_up(part_name.len() + 1, 4).min(payload.len());
    Ok(PartsData {
        part_name,
        referenced_data: payload[name_size..].to_vec(),
    })
}
fn write_parts_data<W: Write + Seek>(
    data: &PartsData,
    writer: &mut W,
    _: Endian,
    _: (),
) -> BinResult<()> {
    let mut payload = Vec::with_capacity(data.serialized_size());
    payload.extend(data.part_name.bytes());
    payload.resize(align_up(payload.len() + 1, 4), 0);
    payload.extend(&data.referenced_data);
    writer.write_all(&payload)?;
    Ok(())
}

impl Parts {
    pub fn new(
        pane: PaneInfo,
        magnify: Vec2,
        properties: Vec<PartsProperty>,
        data: PartsData,
    ) -> Self {
        Self {
            size: 0,
            pane,
            property_count: 0,
            magnify,
            properties,
            data,
        }
    }
}
