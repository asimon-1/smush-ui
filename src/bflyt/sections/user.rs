use binrw::{BinRead, BinResult, Endian, Error, binrw};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read, Seek, Write};

use super::common::{align_up, read_null_string_at, write_u16, write_u32, write_u32_at};

/// `usd1` user-data section.
#[derive(Serialize, Deserialize)]
#[binrw]
#[brw(magic = b"usd1")]
pub struct User {
    #[serde(skip)]
    #[bw(map = |_: &u32| 8 + data.payload_size() as u32)]
    size: u32,
    #[br(parse_with = read_user_data, args(size))]
    #[bw(write_with = write_user_data)]
    pub data: UserData,
}

/// The named values stored by a `usd1` section.
#[derive(Serialize, Deserialize)]
pub struct UserData {
    pub entries: Vec<UserDataEntry>,
}

impl UserData {
    fn payload_size(&self) -> usize {
        let data_size = self
            .entries
            .iter()
            .map(|entry| entry.value.serialized_size())
            .sum::<usize>();
        let names_size = self
            .entries
            .iter()
            .map(|entry| entry.name.len() + 1)
            .sum::<usize>();
        4 + self.entries.len() * 12 + data_size + align_up(names_size, 4)
    }
}

#[derive(Serialize, Deserialize)]
pub struct UserDataEntry {
    pub name: String,
    pub value: UserDataValue,
    pub reserved: u8,
}

#[derive(Serialize, Deserialize)]
pub enum UserDataValue {
    String(String),
    Integers(Vec<i32>),
    Floats(Vec<f32>),
    SystemData(Vec<UserSystemData>),
}

impl UserDataValue {
    fn type_tag(&self) -> u8 {
        match self {
            Self::String(_) => 0,
            Self::Integers(_) => 1,
            Self::Floats(_) => 2,
            Self::SystemData(_) => 3,
        }
    }
    fn count(&self) -> u16 {
        match self {
            Self::String(value) => value.len() as u16,
            Self::Integers(values) => values.len() as u16,
            Self::Floats(values) => values.len() as u16,
            Self::SystemData(values) => values.len() as u16,
        }
    }
    fn serialized_size(&self) -> usize {
        match self {
            Self::String(value) if value.is_empty() => 0,
            Self::String(value) => align_up(value.len() + 1, 4),
            Self::Integers(values) => values.len() * 4,
            Self::Floats(values) => values.len() * 4,
            Self::SystemData(values) if values.is_empty() => 0,
            Self::SystemData(values) => {
                (if values.len() > 1 { 12 } else { 8 })
                    + values
                        .iter()
                        .map(UserSystemData::serialized_size)
                        .sum::<usize>()
            }
        }
    }
}

/// System data currently understood by BFLYT.
#[derive(Serialize, Deserialize)]
pub enum UserSystemData {
    /// Layout animation-tag names.
    AnimationTagNames(Vec<String>),
}

impl UserSystemData {
    fn serialized_size(&self) -> usize {
        match self {
            Self::AnimationTagNames(names) => {
                let strings_size = names.iter().map(|name| name.len() + 1).sum::<usize>();
                8 + names.len() * 4 + align_up(strings_size, 64)
            }
        }
    }
}

impl User {
    pub fn new(data: UserData) -> Self {
        Self { size: 0, data }
    }
}

fn custom_error(position: u64, error: impl std::error::Error + Send + Sync + 'static) -> Error {
    Error::Custom {
        pos: position,
        err: Box::new(error),
    }
}

fn read_user_data<R: Read + Seek>(
    reader: &mut R,
    endian: Endian,
    (size,): (u32,),
) -> BinResult<UserData> {
    let mut payload = vec![0; (size - 8) as usize];
    reader.read_exact(&mut payload)?;
    let mut header = Cursor::new(payload.as_slice());
    let entry_count = u16::read_options(&mut header, endian, ())? as usize;
    let _reserved = u16::read_options(&mut header, endian, ())?;
    let mut entries = Vec::with_capacity(entry_count);
    for entry_index in 0..entry_count {
        let entry_offset = 4 + entry_index * 12;
        let mut entry_reader = Cursor::new(&payload[entry_offset..entry_offset + 12]);
        let name_offset = u32::read_options(&mut entry_reader, endian, ())? as usize;
        let data_offset = u32::read_options(&mut entry_reader, endian, ())? as usize;
        let count = u16::read_options(&mut entry_reader, endian, ())? as usize;
        let type_tag = u8::read_options(&mut entry_reader, endian, ())?;
        let reserved = u8::read_options(&mut entry_reader, endian, ())?;
        let name = read_null_string_at(&payload, entry_offset + name_offset, endian)?;
        let value = read_value(
            &payload,
            entry_offset + data_offset,
            data_offset == 0,
            count,
            type_tag,
            endian,
        )?;
        entries.push(UserDataEntry {
            name,
            value,
            reserved,
        });
    }
    Ok(UserData { entries })
}

fn read_value(
    payload: &[u8],
    data_start: usize,
    is_empty: bool,
    count: usize,
    type_tag: u8,
    endian: Endian,
) -> BinResult<UserDataValue> {
    if is_empty {
        return match type_tag {
            0 => Ok(UserDataValue::String(String::new())),
            1 => Ok(UserDataValue::Integers(Vec::new())),
            2 => Ok(UserDataValue::Floats(Vec::new())),
            3 => Ok(UserDataValue::SystemData(Vec::new())),
            _ => Err(custom_error(
                data_start as u64,
                UnsupportedUserDataType(type_tag),
            )),
        };
    }
    match type_tag {
        0 => Ok(UserDataValue::String(
            String::from_utf8(payload[data_start..data_start + count].to_vec())
                .map_err(|error| custom_error(data_start as u64, error))?,
        )),
        1 => Ok(UserDataValue::Integers(read_integers(
            payload, data_start, count, endian,
        )?)),
        2 => Ok(UserDataValue::Floats(read_floats(
            payload, data_start, count, endian,
        )?)),
        3 => Ok(UserDataValue::SystemData(read_system_data(
            payload, data_start, count, endian,
        )?)),
        _ => Err(custom_error(
            data_start as u64,
            UnsupportedUserDataType(type_tag),
        )),
    }
}

fn read_integers(
    payload: &[u8],
    offset: usize,
    count: usize,
    endian: Endian,
) -> BinResult<Vec<i32>> {
    let mut reader = Cursor::new(&payload[offset..]);
    (0..count)
        .map(|_| i32::read_options(&mut reader, endian, ()))
        .collect()
}
fn read_floats(payload: &[u8], offset: usize, count: usize, endian: Endian) -> BinResult<Vec<f32>> {
    let mut reader = Cursor::new(&payload[offset..]);
    (0..count)
        .map(|_| f32::read_options(&mut reader, endian, ()))
        .collect()
}

fn read_system_data(
    payload: &[u8],
    data_start: usize,
    block_count: usize,
    endian: Endian,
) -> BinResult<Vec<UserSystemData>> {
    let mut result = Vec::new();
    let mut block_start = data_start;
    for _ in 0..block_count {
        let mut block = Cursor::new(&payload[block_start..]);
        let _reserved = u16::read_options(&mut block, endian, ())?;
        let item_count = u16::read_options(&mut block, endian, ())? as usize;
        let items_offset = u32::read_options(&mut block, endian, ())? as usize;
        let mut item_start = block_start + items_offset;
        for _ in 0..item_count {
            let mut item = Cursor::new(&payload[item_start..]);
            let data_type = u32::read_options(&mut item, endian, ())?;
            if data_type != 0 {
                return Err(custom_error(
                    item_start as u64,
                    UnsupportedSystemDataType(data_type),
                ));
            }
            let string_count = u32::read_options(&mut item, endian, ())? as usize;
            let mut names = Vec::with_capacity(string_count);
            for string_index in 0..string_count {
                let mut offset = Cursor::new(&payload[item_start + 8 + string_index * 4..]);
                let string_offset = u32::read_options(&mut offset, endian, ())? as usize;
                names.push(read_null_string_at(
                    payload,
                    item_start + string_offset,
                    endian,
                )?);
            }
            result.push(UserSystemData::AnimationTagNames(names));
            let strings_size = result.last().unwrap().serialized_size();
            item_start += strings_size;
        }
        block_start = item_start;
    }
    Ok(result)
}

fn write_user_data<W: Write + Seek>(
    data: &UserData,
    writer: &mut W,
    endian: Endian,
    _: (),
) -> BinResult<()> {
    let mut payload = Vec::new();
    write_u16(&mut payload, data.entries.len() as u16, endian);
    write_u16(&mut payload, 0, endian);
    payload.resize(4 + data.entries.len() * 12, 0);
    let mut data_offsets = vec![0; data.entries.len()];
    for type_tag in [3, 1, 2, 0] {
        for (entry_index, entry) in data.entries.iter().enumerate() {
            if entry.value.type_tag() != type_tag || entry.value.serialized_size() == 0 {
                continue;
            }
            data_offsets[entry_index] = (payload.len() - (4 + entry_index * 12)) as u32;
            write_value(&mut payload, &entry.value, endian)?;
        }
    }
    let names_start = payload.len();
    let mut name_offsets = Vec::with_capacity(data.entries.len());
    for (entry_index, entry) in data.entries.iter().enumerate() {
        name_offsets.push((payload.len() - (4 + entry_index * 12)) as u32);
        payload.extend(entry.name.bytes());
        payload.push(0);
    }
    payload.resize(names_start + align_up(payload.len() - names_start, 4), 0);
    for (entry_index, entry) in data.entries.iter().enumerate() {
        let entry_offset = 4 + entry_index * 12;
        let mut encoded = Vec::new();
        write_u32(&mut encoded, name_offsets[entry_index], endian);
        write_u32(&mut encoded, data_offsets[entry_index], endian);
        write_u16(&mut encoded, entry.value.count(), endian);
        encoded.push(entry.value.type_tag());
        encoded.push(entry.reserved);
        payload[entry_offset..entry_offset + 12].copy_from_slice(&encoded);
    }
    writer.write_all(&payload)?;
    Ok(())
}

fn write_value(payload: &mut Vec<u8>, value: &UserDataValue, endian: Endian) -> BinResult<()> {
    match value {
        UserDataValue::String(value) => {
            payload.extend(value.bytes());
            payload.push(0);
            payload.resize(align_up(payload.len(), 4), 0);
        }
        UserDataValue::Integers(values) => {
            for value in values {
                write_u32(payload, *value as u32, endian);
            }
        }
        UserDataValue::Floats(values) => {
            for value in values {
                write_u32(payload, value.to_bits(), endian);
            }
        }
        UserDataValue::SystemData(values) => write_system_data(payload, values, endian)?,
    }
    Ok(())
}

fn write_system_data(
    payload: &mut Vec<u8>,
    values: &[UserSystemData],
    endian: Endian,
) -> BinResult<()> {
    write_u16(payload, 0, endian);
    write_u16(payload, values.len() as u16, endian);
    let items_offset = if values.len() > 1 { 12 } else { 8 };
    write_u32(payload, items_offset, endian);
    if values.len() > 1 {
        write_u32(
            payload,
            values
                .iter()
                .map(UserSystemData::serialized_size)
                .sum::<usize>() as u32,
            endian,
        );
    }
    for value in values {
        let UserSystemData::AnimationTagNames(names) = value;
        let item_start = payload.len();
        write_u32(payload, 0, endian);
        write_u32(payload, names.len() as u32, endian);
        let offsets_start = payload.len();
        payload.resize(offsets_start + names.len() * 4, 0);
        let strings_start = payload.len();
        for (index, name) in names.iter().enumerate() {
            let string_offset = (payload.len() - item_start) as u32;
            write_u32_at(payload, offsets_start + index * 4, string_offset, endian);
            payload.extend(name.bytes());
            payload.push(0);
        }
        payload.resize(
            strings_start + align_up(payload.len() - strings_start, 64),
            0,
        );
    }
    Ok(())
}

#[derive(Debug)]
struct UnsupportedUserDataType(u8);
impl std::fmt::Display for UnsupportedUserDataType {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "unsupported user-data type {}", self.0)
    }
}
impl std::error::Error for UnsupportedUserDataType {}
#[derive(Debug)]
struct UnsupportedSystemDataType(u32);
impl std::fmt::Display for UnsupportedSystemDataType {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "unsupported system-data type {}", self.0)
    }
}
impl std::error::Error for UnsupportedSystemDataType {}
