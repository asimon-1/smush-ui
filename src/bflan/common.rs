use std::io;

use binrw::{BinResult, Error as BinError};

pub(crate) fn invalid(pos: usize, message: &'static str) -> BinError {
    BinError::Custom {
        pos: pos as u64,
        err: Box::new(io::Error::new(io::ErrorKind::InvalidData, message)),
    }
}

pub(crate) fn bytes_at(data: &[u8], offset: usize, len: usize) -> BinResult<&[u8]> {
    let end = offset
        .checked_add(len)
        .ok_or_else(|| invalid(offset, "offset overflow"))?;
    data.get(offset..end)
        .ok_or_else(|| invalid(offset, "offset outside BFLAN record"))
}

pub(crate) fn u16_at(data: &[u8], offset: usize) -> BinResult<u16> {
    Ok(u16::from_le_bytes(
        bytes_at(data, offset, 2)?.try_into().unwrap(),
    ))
}

pub(crate) fn u32_at(data: &[u8], offset: usize) -> BinResult<u32> {
    Ok(u32::from_le_bytes(
        bytes_at(data, offset, 4)?.try_into().unwrap(),
    ))
}

pub(crate) fn checked_u32(value: usize, pos: usize) -> BinResult<u32> {
    u32::try_from(value).map_err(|_| invalid(pos, "BFLAN offset exceeds u32"))
}

pub(crate) fn read_cstr(data: &[u8], offset: usize) -> BinResult<String> {
    let tail = data
        .get(offset..)
        .ok_or_else(|| invalid(offset, "string offset outside record"))?;
    let end = tail
        .iter()
        .position(|&byte| byte == 0)
        .ok_or_else(|| invalid(offset, "unterminated BFLAN string"))?;
    std::str::from_utf8(&tail[..end])
        .map(str::to_owned)
        .map_err(|_| invalid(offset, "invalid UTF-8 BFLAN string"))
}

pub(crate) fn align4(out: &mut Vec<u8>) {
    out.resize((out.len() + 3) & !3, 0);
}

pub(crate) fn patch_u32(out: &mut [u8], offset: usize, value: usize) -> BinResult<()> {
    let value = checked_u32(value, offset)?;
    out.get_mut(offset..offset + 4)
        .ok_or_else(|| invalid(offset, "offset patch outside output"))?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}

pub(crate) fn patch_relative(out: &mut [u8], patch: usize, base: usize) -> BinResult<()> {
    let value = out
        .len()
        .checked_sub(base)
        .ok_or_else(|| invalid(base, "negative relative offset"))?;
    patch_u32(out, patch, value)
}
