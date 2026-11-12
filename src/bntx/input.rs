use super::bntx_error;
use binrw::{BinRead, BinResult, Endian, io::Cursor};

pub(crate) struct Input<'a> {
    bytes: &'a [u8],
}

impl<'a> Input<'a> {
    pub(crate) const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    pub(crate) const fn len(&self) -> usize {
        self.bytes.len()
    }

    pub(crate) fn read_at<T>(&self, offset: u64, size: u64, label: &str) -> BinResult<T>
    where
        T: for<'args> BinRead<Args<'args> = ()>,
    {
        ensure_range(self.len(), offset, size, label)?;
        let mut reader = Cursor::new(self.bytes);
        reader.set_position(offset);
        T::read_options(&mut reader, Endian::Little, ())
    }

    pub(crate) fn read_utf8_record(&self, offset: u64, limit: u64) -> BinResult<(String, u64)> {
        let length = usize::from(self.read_u16(offset)?);
        let text_end = 2 + length;
        let record_size = (length + 4) & !1;
        let record_size = record_size as u64;
        if offset
            .checked_add(record_size)
            .is_none_or(|end| end > limit)
        {
            return Err(bntx_error(
                offset,
                "BNTX aligned string exceeds its containing range",
            ));
        }
        let record = self.slice(offset, record_size, "BNTX string record")?;
        if record[text_end] != 0 {
            return Err(bntx_error(offset, "BNTX string is not terminated"));
        }
        if record[text_end + 1..].iter().any(|&byte| byte != 0) {
            return Err(bntx_error(
                offset + text_end as u64 + 1,
                "BNTX string alignment byte is nonzero",
            ));
        }
        let value = std::str::from_utf8(&record[2..text_end])
            .map_err(|_| bntx_error(offset, "BNTX string is not valid UTF-8"))?
            .to_owned();
        Ok((value, record_size))
    }

    pub(crate) fn read_utf16_record(&self, offset: u64) -> BinResult<(String, u64)> {
        let count = u64::from(self.read_u16(offset)?);
        let bytes = checked_mul(count, 2, offset, "wide-string units")?;
        let text_start = offset + 2;
        let terminator = text_start
            .checked_add(bytes)
            .ok_or_else(|| bntx_error(offset, "BNTX wide-string length overflows"))?;
        ensure_range(self.len(), text_start, bytes + 2, "BNTX wide string")?;
        if self.read_u16(terminator)? != 0 {
            return Err(bntx_error(offset, "BNTX wide string is not terminated"));
        }
        let units = (0..count)
            .map(|index| self.read_u16(text_start + index * 2))
            .collect::<BinResult<Vec<_>>>()?;
        let value = char::decode_utf16(units)
            .collect::<Result<String, _>>()
            .map_err(|_| bntx_error(offset, "BNTX wide string is not valid UTF-16"))?;
        Ok((value, bytes + 4))
    }

    pub(crate) fn read_u16(&self, offset: u64) -> BinResult<u16> {
        let bytes = self.slice(offset, 2, "BNTX u16")?;
        Ok(u16::from_le_bytes(
            bytes.try_into().expect("two-byte slice"),
        ))
    }

    pub(crate) fn read_u32(&self, offset: u64) -> BinResult<u32> {
        let bytes = self.slice(offset, 4, "BNTX u32")?;
        Ok(u32::from_le_bytes(
            bytes.try_into().expect("four-byte slice"),
        ))
    }

    pub(crate) fn read_u64(&self, offset: u64) -> BinResult<u64> {
        let bytes = self.slice(offset, 8, "BNTX u64")?;
        Ok(u64::from_le_bytes(
            bytes.try_into().expect("eight-byte slice"),
        ))
    }

    pub(crate) fn slice(&self, offset: u64, length: u64, label: &str) -> BinResult<&'a [u8]> {
        let start = usize::try_from(offset)
            .map_err(|_| bntx_error(offset, format!("{label} offset is too large")))?;
        let length = usize::try_from(length)
            .map_err(|_| bntx_error(offset, format!("{label} length is too large")))?;
        let end = start
            .checked_add(length)
            .filter(|&end| end <= self.len())
            .ok_or_else(|| bntx_error(offset, format!("{label} is outside the declared file")))?;
        Ok(&self.bytes[start..end])
    }
}

pub(crate) fn ensure_range(
    file_size: usize,
    offset: u64,
    length: u64,
    label: &str,
) -> BinResult<()> {
    let end = offset
        .checked_add(length)
        .ok_or_else(|| bntx_error(offset, format!("{label} range overflows")))?;
    if end > file_size as u64 {
        return Err(bntx_error(
            offset,
            format!("{label} exceeds the declared file size"),
        ));
    }
    Ok(())
}

pub(crate) fn ensure_contained(
    offset: u64,
    length: u64,
    minimum_offset: u64,
    maximum_end: u64,
    error_position: u64,
    label: &str,
) -> BinResult<()> {
    let end = offset
        .checked_add(length)
        .ok_or_else(|| bntx_error(error_position, format!("{label} range overflows")))?;
    if offset < minimum_offset || end > maximum_end {
        return Err(bntx_error(
            error_position,
            format!("{label} is outside its containing BRTI block"),
        ));
    }
    Ok(())
}

pub(crate) fn checked_mul(left: u64, right: u64, position: u64, label: &str) -> BinResult<u64> {
    left.checked_mul(right)
        .ok_or_else(|| bntx_error(position, format!("BNTX {label} size overflows")))
}
