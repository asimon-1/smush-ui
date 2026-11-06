use super::common::{fixed_string_to_string, string_to_fixed_string};
use binrw::{BinRead, BinResult, Endian, Error, NullString, binrw};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read, Seek, Write};

#[derive(Serialize, Deserialize)]
pub struct ControlData {
    pub control_name: String,
    pub reserve0_name: String,
    pub pane_bindings: Vec<String>,
    pub core_anims: Vec<String>,
    pub pane_names: Vec<String>,
    pub anim_names: Vec<String>,
}
impl ControlData {
    fn payload_size(&self) -> usize {
        let align_to_four_bytes = |size| (size + 3) & !3;
        let null_terminated_strings_size =
            |strings: &[String]| strings.iter().map(|string| string.len() + 1).sum::<usize>();
        20 + align_to_four_bytes(self.control_name.len() + 1)
            + align_to_four_bytes(self.reserve0_name.len() + 1)
            + 24 * self.pane_bindings.len()
            + align_to_four_bytes(
                4 * self.core_anims.len() + null_terminated_strings_size(&self.core_anims),
            )
            + align_to_four_bytes(
                4 * self.pane_names.len() + null_terminated_strings_size(&self.pane_names),
            )
            + align_to_four_bytes(
                4 * self.anim_names.len() + null_terminated_strings_size(&self.anim_names),
            )
    }
}

#[derive(Serialize, Deserialize)]
#[binrw]
#[brw(magic = b"cnt1")]
pub struct Control {
    #[serde(skip)]
    #[bw(map = |_: &u32| 8 + data.payload_size() as u32)]
    size: u32,
    #[br(parse_with = read_control_data, args(size))]
    #[bw(write_with = write_control_data)]
    pub data: ControlData,
}
impl Control {
    pub fn new(data: ControlData) -> Self {
        Self { size: 0, data }
    }
}

fn read_control_data<R: Read + Seek>(
    reader: &mut R,
    endian: Endian,
    (size,): (u32,),
) -> BinResult<ControlData> {
    let mut payload = vec![0; (size - 8) as usize];
    reader.read_exact(&mut payload)?;
    let mut header = Cursor::new(payload.as_slice());
    let reserve0_offset = u32::read_options(&mut header, endian, ())? as usize - 8;
    let bindings_offset = u32::read_options(&mut header, endian, ())? as usize - 8;
    let pane_count = u16::read_options(&mut header, endian, ())? as usize;
    let animation_count = u16::read_options(&mut header, endian, ())? as usize;
    let pane_table_offset = u32::read_options(&mut header, endian, ())? as usize - 8;
    let animation_table_offset = u32::read_options(&mut header, endian, ())? as usize - 8;
    let control_name = NullString::read_options(&mut header, endian, ())?
        .try_into()
        .map_err(|x| Error::Custom {
            pos: 0,
            err: Box::new(x),
        })?;
    let read_string_at = |offset: usize| -> BinResult<String> {
        let mut string_reader = Cursor::new(&payload[offset..]);
        NullString::read_options(&mut string_reader, endian, ())?
            .try_into()
            .map_err(|x| Error::Custom {
                pos: offset as u64,
                err: Box::new(x),
            })
    };
    let read_fixed_name_at = |offset: usize| {
        fixed_string_to_string::<24>(payload[offset..offset + 24].try_into().unwrap()).map_err(
            |error| Error::Custom {
                pos: offset as u64,
                err: Box::new(error),
            },
        )
    };
    let read_string_table = |table_offset: usize, count: usize| -> BinResult<Vec<String>> {
        (0..count)
            .map(|index| {
                let bytes: [u8; 4] = payload
                    [table_offset + index * 4..table_offset + index * 4 + 4]
                    .try_into()
                    .unwrap();
                let string_offset = if endian == Endian::Little {
                    u32::from_le_bytes(bytes)
                } else {
                    u32::from_be_bytes(bytes)
                } as usize;
                read_string_at(table_offset + string_offset)
            })
            .collect()
    };
    Ok(ControlData {
        control_name,
        reserve0_name: read_string_at(reserve0_offset)?,
        pane_bindings: (0..pane_count)
            .map(|index| read_fixed_name_at(bindings_offset + index * 24))
            .collect::<BinResult<_>>()?,
        core_anims: read_string_table(bindings_offset + pane_count * 24, animation_count)?,
        pane_names: read_string_table(pane_table_offset, pane_count)?,
        anim_names: read_string_table(animation_table_offset, animation_count)?,
    })
}

fn write_u32(bytes: &mut Vec<u8>, value: u32, endian: Endian) {
    bytes.extend_from_slice(&if endian == Endian::Little {
        value.to_le_bytes()
    } else {
        value.to_be_bytes()
    });
}
fn write_u16(bytes: &mut Vec<u8>, value: u16, endian: Endian) {
    bytes.extend_from_slice(&if endian == Endian::Little {
        value.to_le_bytes()
    } else {
        value.to_be_bytes()
    });
}
fn write_control_data<W: Write + Seek>(
    data: &ControlData,
    writer: &mut W,
    endian: Endian,
    _: (),
) -> BinResult<()> {
    let mut p = vec![0; 20];
    let align_to_four_bytes = |bytes: &mut Vec<u8>| bytes.resize((bytes.len() + 3) & !3, 0);
    let write_null_terminated_string = |bytes: &mut Vec<u8>, string: &str| {
        bytes.extend(string.bytes());
        bytes.push(0)
    };
    let section_relative_offset = |payload_offset: usize| (payload_offset + 8) as u32;
    write_null_terminated_string(&mut p, &data.control_name);
    align_to_four_bytes(&mut p);
    let reserve0_offset = section_relative_offset(p.len());
    write_null_terminated_string(&mut p, &data.reserve0_name);
    align_to_four_bytes(&mut p);
    let bindings_offset = section_relative_offset(p.len());
    for binding in &data.pane_bindings {
        p.extend(
            string_to_fixed_string::<24>(binding).map_err(|error| Error::Custom {
                pos: 0,
                err: Box::new(error),
            })?,
        )
    }
    let write_string_table = |bytes: &mut Vec<u8>, strings: &[String]| {
        let table_start = bytes.len();
        bytes.resize(table_start + strings.len() * 4, 0);
        for (index, string) in strings.iter().enumerate() {
            let string_offset = (bytes.len() - table_start) as u32;
            let encoded_offset = if endian == Endian::Little {
                string_offset.to_le_bytes()
            } else {
                string_offset.to_be_bytes()
            };
            bytes[table_start + index * 4..table_start + index * 4 + 4]
                .copy_from_slice(&encoded_offset);
            write_null_terminated_string(bytes, string)
        }
        align_to_four_bytes(bytes);
        section_relative_offset(table_start)
    };
    let _core_table_offset = write_string_table(&mut p, &data.core_anims);
    let pane_table_offset = write_string_table(&mut p, &data.pane_names);
    let animation_table_offset = write_string_table(&mut p, &data.anim_names);
    let mut header = Vec::new();
    write_u32(&mut header, reserve0_offset, endian);
    write_u32(&mut header, bindings_offset, endian);
    write_u16(&mut header, data.pane_names.len() as u16, endian);
    write_u16(&mut header, data.anim_names.len() as u16, endian);
    write_u32(&mut header, pane_table_offset, endian);
    write_u32(&mut header, animation_table_offset, endian);
    p[..20].copy_from_slice(&header);
    writer.write_all(&p)?;
    Ok(())
}
