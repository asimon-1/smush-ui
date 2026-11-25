use std::io::{Read, Seek, Write};

use binrw::{BinReaderExt, BinResult, BinWriterExt, Endian, NullString, binrw};
use serde::{Deserialize, Serialize};

use crate::bflyt::sections::common::{fixed_string_to_string, string_to_fixed_string};
use crate::bflyt::sections::user::{UserData, UserDataEntry, UserDataValue};

use super::common::{
    align4, bytes_at, checked_u32, invalid, patch_relative, read_cstr, u16_at, u32_at,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub enum GroupRecordSize {
    Compact,
    #[default]
    Full,
}

impl GroupRecordSize {
    fn bytes(self) -> usize {
        match self {
            Self::Compact => 28,
            Self::Full => 36,
        }
    }

    fn is_full(&self) -> bool {
        *self == Self::Full
    }
}

impl From<GroupRecordSize> for u8 {
    fn from(size: GroupRecordSize) -> Self {
        size.bytes() as u8
    }
}

impl TryFrom<u8> for GroupRecordSize {
    type Error = String;

    fn try_from(size: u8) -> Result<Self, Self::Error> {
        match size {
            28 => Ok(Self::Compact),
            36 => Ok(Self::Full),
            _ => Err(format!(
                "PAT1 group record size must be 28 or 36, got {size}"
            )),
        }
    }
}

#[binrw]
#[brw(little, magic = b"pat1")]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pat1 {
    #[br(temp)]
    #[bw(calc = pat1_size(name, groups, user_data, *group_record_size)?)]
    size: u32,
    pub order: u16,
    #[br(temp)]
    #[bw(calc = u16::try_from(groups.len()).map_err(|_| invalid(10, "too many PAT1 groups"))?)]
    group_count: u16,
    #[brw(magic = 32u32)]
    #[br(temp)]
    #[bw(calc = pat1_groups_offset(name)?)]
    groups_offset: u32,
    #[br(temp)]
    #[bw(calc = pat1_user_data_offset(name, groups, user_data, *group_record_size)?)]
    user_data_offset: u32,
    #[serde(default, skip_serializing_if = "GroupRecordSize::is_full")]
    #[br(try_calc = infer_group_record_size(size, group_count, groups_offset, user_data_offset))]
    #[bw(ignore)]
    pub group_record_size: GroupRecordSize,
    pub start_frame: i16,
    pub end_frame: i16,
    #[br(try_map = |value: u8| if value <= 1 { Ok(value != 0) } else { Err(invalid(28, "invalid PAT1 child-binding flag")) })]
    #[bw(map = |value: &bool| u8::from(*value))]
    #[brw(pad_after = 3)]
    pub child_binding: bool,
    #[br(try_map = |name: NullString| String::try_from(name), align_after = 4)]
    #[bw(map = |name: &String| NullString::from(name.as_str()), align_after = 4)]
    pub name: String,
    #[br(parse_with = read_groups, args(group_count, group_record_size, groups_offset, pat1_groups_offset(&name)?))]
    #[bw(write_with = write_groups, args(*group_record_size))]
    pub groups: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[br(parse_with = read_user_data, args(size, user_data_offset, pat1_body_end(&name, &groups, group_record_size)?))]
    #[bw(write_with = write_user_data)]
    pub user_data: Option<UserData>,
}

fn pat1_groups_offset(name: &str) -> BinResult<u32> {
    let length = 32usize
        .checked_add(name.len())
        .and_then(|n| n.checked_add(1))
        .ok_or_else(|| invalid(16, "PAT1 name length overflow"))?;
    checked_u32(
        length
            .checked_add(3)
            .ok_or_else(|| invalid(16, "PAT1 name length overflow"))?
            & !3,
        16,
    )
}

fn infer_group_record_size(
    size: u32,
    count: u16,
    groups_offset: u32,
    user_data_offset: u32,
) -> BinResult<GroupRecordSize> {
    let end = if user_data_offset == 0 {
        size
    } else {
        user_data_offset
    };
    let length = end
        .checked_sub(groups_offset)
        .ok_or_else(|| invalid(16, "PAT1 group records exceed section"))? as usize;
    if count == 0 && length == 0 || length == count as usize * 36 {
        Ok(GroupRecordSize::Full)
    } else if length == count as usize * 28 {
        Ok(GroupRecordSize::Compact)
    } else {
        Err(invalid(
            16,
            "PAT1 group records must be 28 or 36 bytes each",
        ))
    }
}

fn pat1_body_end(name: &str, groups: &[String], record_size: GroupRecordSize) -> BinResult<u32> {
    let base = pat1_groups_offset(name)? as usize;
    checked_u32(
        base.checked_add(
            groups
                .len()
                .checked_mul(record_size.bytes())
                .ok_or_else(|| invalid(16, "PAT1 group size overflow"))?,
        )
        .ok_or_else(|| invalid(16, "PAT1 group size overflow"))?,
        16,
    )
}

fn pat1_user_data_offset(
    name: &str,
    groups: &[String],
    user_data: &Option<UserData>,
    record_size: GroupRecordSize,
) -> BinResult<u32> {
    if user_data.is_some() {
        pat1_body_end(name, groups, record_size)
    } else {
        Ok(0)
    }
}

fn pat1_size(
    name: &str,
    groups: &[String],
    user_data: &Option<UserData>,
    record_size: GroupRecordSize,
) -> BinResult<u32> {
    let mut size = pat1_body_end(name, groups, record_size)? as usize;
    if let Some(data) = user_data {
        let mut bytes = Vec::new();
        write_string_user_data(&mut bytes, data)?;
        size = size
            .checked_add(bytes.len())
            .ok_or_else(|| invalid(4, "PAT1 size overflow"))?;
    }
    checked_u32(size, 4)
}

fn read_groups<R: Read + Seek>(
    reader: &mut R,
    _: Endian,
    (count, record_size, offset, expected): (u16, GroupRecordSize, u32, u32),
) -> BinResult<Vec<String>> {
    if offset != expected {
        return Err(invalid(16, "PAT1 group offset does not follow name"));
    }
    let mut groups = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let position = reader.stream_position()? as usize;
        let group = match record_size {
            GroupRecordSize::Compact => fixed_string_to_string::<28>(reader.read_le()?),
            GroupRecordSize::Full => fixed_string_to_string::<36>(reader.read_le()?),
        }
        .map_err(|_| invalid(position, "invalid PAT1 group name"))?;
        groups.push(group);
    }
    Ok(groups)
}

#[allow(clippy::ptr_arg)] // binrw passes the Vec field directly.
fn write_groups<W: Write + Seek>(
    groups: &Vec<String>,
    writer: &mut W,
    _: Endian,
    (record_size,): (GroupRecordSize,),
) -> BinResult<()> {
    for group in groups {
        match record_size {
            GroupRecordSize::Compact => writer.write_le(&string_to_fixed_string::<28>(group)?)?,
            GroupRecordSize::Full => writer.write_le(&string_to_fixed_string::<36>(group)?)?,
        }
    }
    Ok(())
}

fn read_user_data<R: Read + Seek>(
    reader: &mut R,
    _: Endian,
    (size, offset, expected): (u32, u32, u32),
) -> BinResult<Option<UserData>> {
    if offset == 0 {
        if expected != size {
            return Err(invalid(4, "PAT1 size mismatch"));
        }
        return Ok(None);
    }
    if offset != expected || offset > size {
        return Err(invalid(20, "invalid PAT1 user-data offset"));
    }
    let mut bytes = vec![0; (size - offset) as usize];
    reader.read_exact(&mut bytes)?;
    Ok(Some(parse_string_user_data(&bytes)?))
}

fn write_user_data<W: Write + Seek>(
    data: &Option<UserData>,
    writer: &mut W,
    _: Endian,
    _: (),
) -> BinResult<()> {
    if let Some(data) = data {
        let mut bytes = Vec::new();
        write_string_user_data(&mut bytes, data)?;
        writer.write_all(&bytes)?;
    }
    Ok(())
}

fn parse_string_user_data(data: &[u8]) -> BinResult<UserData> {
    if bytes_at(data, 0, 4)? != b"usd1" {
        return Err(invalid(0, "invalid PAT1 user-data tag"));
    }
    if u32_at(data, 4)? as usize != data.len() {
        return Err(invalid(4, "PAT1 user-data size mismatch"));
    }
    let count = u16_at(data, 8)? as usize;
    bytes_at(
        data,
        12,
        count
            .checked_mul(12)
            .ok_or_else(|| invalid(8, "user-data count overflow"))?,
    )?;
    let mut entries = Vec::with_capacity(count);
    for index in 0..count {
        let record = 12 + index * 12;
        let name_offset = u32_at(data, record)? as usize;
        let value_offset = u32_at(data, record + 4)? as usize;
        let value_len = u16_at(data, record + 8)? as usize;
        if bytes_at(data, record + 10, 1)?[0] != 0 {
            return Err(invalid(record + 10, "unsupported PAT1 user-data type"));
        }
        let name_start = record
            .checked_add(name_offset)
            .ok_or_else(|| invalid(record, "user-data name offset overflow"))?;
        let name = read_cstr(data, name_start)?;
        let value = if value_offset == 0 && value_len == 0 {
            String::new()
        } else {
            let start = record
                .checked_add(value_offset)
                .ok_or_else(|| invalid(record, "user-data value offset overflow"))?;
            let raw = bytes_at(data, start, value_len)?;
            std::str::from_utf8(raw)
                .map_err(|_| invalid(start, "invalid PAT1 user-data string"))?
                .to_owned()
        };
        entries.push(UserDataEntry {
            name,
            value: UserDataValue::String(value),
        });
    }
    Ok(UserData { entries })
}

fn write_string_user_data(out: &mut Vec<u8>, data: &UserData) -> BinResult<()> {
    let start = out.len();
    out.extend_from_slice(b"usd1");
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(
        &u16::try_from(data.entries.len())
            .map_err(|_| invalid(start, "too many user data entries"))?
            .to_le_bytes(),
    );
    out.extend_from_slice(&0u16.to_le_bytes());
    let table = out.len();
    out.resize(out.len() + 12 * data.entries.len(), 0);
    for (i, entry) in data.entries.iter().enumerate() {
        let record = table + 12 * i;
        let UserDataValue::String(value) = &entry.value else {
            return Err(invalid(record, "unsupported PAT1 user-data value"));
        };
        patch_relative(out, record + 4, record)?;
        out.extend_from_slice(value.as_bytes());
        out.push(0);
        out[record + 8..record + 10].copy_from_slice(
            &u16::try_from(value.len())
                .map_err(|_| invalid(record, "user data string too long"))?
                .to_le_bytes(),
        );
    }
    for (i, entry) in data.entries.iter().enumerate() {
        let record = table + 12 * i;
        patch_relative(out, record, record)?;
        out.extend_from_slice(entry.name.as_bytes());
        out.push(0);
    }
    align4(out);
    patch_relative(out, start + 4, start)?;
    Ok(())
}
