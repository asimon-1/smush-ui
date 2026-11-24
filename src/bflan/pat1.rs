use std::io::{Read, Seek, Write};

use binrw::{BinResult, Endian, NullString, binrw};
use serde::{Deserialize, Serialize};

use crate::bflyt::sections::common::{fixed_string_to_string, string_to_fixed_string};
use crate::bflyt::sections::user::{UserData, UserDataEntry, UserDataValue};

use super::{align4, bytes_at, checked_u32, invalid, patch_relative, read_cstr, u16_at, u32_at};

#[binrw]
#[brw(little, magic = b"pat1")]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pat1 {
    #[br(temp)]
    #[bw(calc = pat1_size(name, groups, user_data)?)]
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
    #[bw(calc = pat1_user_data_offset(name, groups, user_data)?)]
    user_data_offset: u32,
    pub start_frame: i16,
    pub end_frame: i16,
    #[br(try_map = |value: u8| if value <= 1 { Ok(value != 0) } else { Err(invalid(28, "invalid PAT1 child-binding flag")) })]
    #[bw(map = |value: &bool| u8::from(*value))]
    #[brw(pad_after = 3)]
    pub child_binding: bool,
    #[br(try_map = |name: NullString| String::try_from(name), align_after = 4)]
    #[bw(map = |name: &String| NullString::from(name.as_str()), align_after = 4)]
    pub name: String,
    #[br(count = group_count, assert(groups_offset == pat1_groups_offset(&name)?, "PAT1 group offset does not follow name"), try_map = |groups: Vec<[u8; 36]>| groups.into_iter().map(fixed_string_to_string::<36>).collect::<Result<Vec<_>, _>>())]
    #[bw(try_map = |groups: &Vec<String>| groups.iter().map(string_to_fixed_string::<36>).collect::<Result<Vec<_>, _>>())]
    pub groups: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[br(parse_with = read_user_data, args(size, user_data_offset, pat1_body_end(&name, &groups)?))]
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

fn pat1_body_end(name: &str, groups: &[String]) -> BinResult<u32> {
    let base = pat1_groups_offset(name)? as usize;
    checked_u32(
        base.checked_add(
            groups
                .len()
                .checked_mul(36)
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
) -> BinResult<u32> {
    if user_data.is_some() {
        pat1_body_end(name, groups)
    } else {
        Ok(0)
    }
}

fn pat1_size(name: &str, groups: &[String], user_data: &Option<UserData>) -> BinResult<u32> {
    let mut size = pat1_body_end(name, groups)? as usize;
    if let Some(data) = user_data {
        let mut bytes = Vec::new();
        write_string_user_data(&mut bytes, data)?;
        size = size
            .checked_add(bytes.len())
            .ok_or_else(|| invalid(4, "PAT1 size overflow"))?;
    }
    checked_u32(size, 4)
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
