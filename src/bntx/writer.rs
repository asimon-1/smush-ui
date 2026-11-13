use super::{
    binary::{
        BLOCK_HEADER_SIZE, BRTI_SIZE, DICTIONARY_NODE_SIZE, DictionaryHeader, DictionaryNodeDisk,
        FILE_HEADER_SIZE, FileHeader, NX_HEADER_SIZE, NxHeader, RELOCATION_ENTRY_SIZE,
        RELOCATION_SECTION_SIZE, RelocationHeader, StringPoolHeader, TextureDataHeader,
        TextureInfoDisk, USER_DATA_SIZE, UserDataDisk,
    },
    bntx_error, dictionary,
    model::{Bntx, Texture, UserDataValue},
    relocation::{self, CanonicalLayout},
    validation, zeroed_bytes,
};
use binrw::{BinResult, BinWrite, Endian, io::Cursor};
use std::collections::{HashMap, HashSet};

const POINTER_TABLE_OFFSET: u64 = FILE_HEADER_SIZE + NX_HEADER_SIZE + 0x140;
// Each BRTI reserves two 0x100-byte regions for runtime texture and view
// objects. Their pointers are relocated, but their bytes remain zero on disk.
const RUNTIME_SCRATCH_SIZE: u64 = 0x100;
const TEXTURE_RUNTIME_OFFSET: u64 = BRTI_SIZE;
const TEXTURE_VIEW_OFFSET: u64 = TEXTURE_RUNTIME_OFFSET + RUNTIME_SCRATCH_SIZE;
const MIP_TABLE_OFFSET: u64 = TEXTURE_VIEW_OFFSET + RUNTIME_SCRATCH_SIZE;

pub(crate) fn write(bntx: &Bntx) -> BinResult<Vec<u8>> {
    let layout = Layout::plan(bntx)?;
    layout.write(bntx)
}

struct LayoutCursor(u64);

impl LayoutCursor {
    fn new(position: u64) -> Self {
        Self(position)
    }

    fn position(&self) -> u64 {
        self.0
    }

    fn advance(&mut self, size: u64, label: &str) -> BinResult<u64> {
        let start = self.0;
        self.0 = self
            .0
            .checked_add(size)
            .ok_or_else(|| bntx_error(start, format!("BNTX {label} overflows")))?;
        Ok(start)
    }

    fn align(&mut self, alignment: u64) -> BinResult<()> {
        self.0 = align_up(self.0, alignment)?;
        Ok(())
    }
}

struct StringTable {
    block_offset: u64,
    values: Vec<String>,
    offsets: HashMap<String, u64>,
    end_offset: u64,
}

impl StringTable {
    fn plan(bntx: &Bntx, block_offset: u64) -> BinResult<Self> {
        let values = ordered_strings(bntx);
        let mut offsets = HashMap::with_capacity(values.len().saturating_add(1));
        offsets.insert(String::new(), block_offset + 0x14);
        let mut cursor = LayoutCursor::new(block_offset + 0x18);
        for value in &values {
            offsets.insert(value.clone(), cursor.position());
            cursor.advance(utf8_record_size(value)? as u64, "string table")?;
        }
        Ok(Self {
            block_offset,
            values,
            offsets,
            end_offset: cursor.position(),
        })
    }

    fn empty_offset(&self) -> u64 {
        self.block_offset + 0x14
    }

    fn dictionary_offset(&self) -> BinResult<u64> {
        align_up(self.end_offset, 8)
    }

    fn offset(&self, value: &str) -> BinResult<u64> {
        self.offsets
            .get(value)
            .copied()
            .ok_or_else(|| bntx_error(0, format!("BNTX string {value:?} was not interned")))
    }

    fn write(&self, output: &mut Output, next_block: u64) -> BinResult<()> {
        let block_size = next_block - self.block_offset;
        output.write_at(
            self.block_offset,
            &StringPoolHeader {
                block_size: block_size as u32,
                string_count: u32::try_from(self.values.len())
                    .map_err(|_| bntx_error(self.block_offset, "too many BNTX strings"))?,
            },
            "string-pool header",
        )?;
        for value in &self.values {
            output.write_raw(
                self.offset(value)?,
                &encode_utf8_record(value)?,
                "string record",
            )?;
        }
        Ok(())
    }
}

struct UserValuePlan {
    offset: u64,
    count: u32,
    value_type: u8,
    bytes: Vec<u8>,
}

struct UserDataPlan {
    records_offset: u64,
    dictionary_offset: u64,
    values: Vec<UserValuePlan>,
    dictionary_nodes: Vec<DictionaryNodeDisk>,
}

impl UserDataPlan {
    fn write(
        &self,
        output: &mut Output,
        texture: &Texture,
        strings: &StringTable,
    ) -> BinResult<()> {
        for (index, (user, value)) in texture.user_data.iter().zip(&self.values).enumerate() {
            output.write_at(
                self.records_offset + index as u64 * USER_DATA_SIZE,
                &UserDataDisk {
                    name_offset: strings.offset(&user.name)?,
                    value_offset: value.offset,
                    count: value.count,
                    value_type: value.value_type,
                },
                "user-data record",
            )?;
            output.write_raw(value.offset, &value.bytes, "user-data value")?;
        }
        write_dictionary(
            output,
            self.dictionary_offset,
            &self.dictionary_nodes,
            "user-data dictionary",
        )
    }
}

struct TexturePlan {
    offset: u64,
    user_data: Option<UserDataPlan>,
    image_start: u64,
}

impl TexturePlan {
    fn plan(texture: &Texture, offset: u64, strings: &StringTable) -> BinResult<(Self, u64)> {
        let mut cursor = LayoutCursor::new(offset + MIP_TABLE_OFFSET);
        cursor.advance(texture.mip_offsets.len() as u64 * 8, "mip table")?;
        let (user_data, metadata_end) = if texture.user_data.is_empty() {
            (None, cursor.position())
        } else {
            cursor.align(8)?;
            let records_offset = cursor.advance(
                texture.user_data.len() as u64 * USER_DATA_SIZE,
                "user-data records",
            )?;
            let dictionary_offset = cursor.position();
            let keys = texture
                .user_data
                .iter()
                .map(|value| Ok((value.name.as_str(), strings.offset(&value.name)?)))
                .collect::<BinResult<Vec<_>>>()?;
            let dictionary_nodes = dictionary::build(&keys, strings.empty_offset())?;
            cursor.advance(
                8 + dictionary_nodes.len() as u64 * DICTIONARY_NODE_SIZE,
                "user-data dictionary",
            )?;
            cursor.align(8)?;
            let mut values = Vec::with_capacity(texture.user_data.len());
            for value in &texture.user_data {
                let (count, value_type, bytes) = encode_user_value(&value.value)?;
                let value_offset = cursor.advance(bytes.len() as u64, "user-data values")?;
                values.push(UserValuePlan {
                    offset: value_offset,
                    count,
                    value_type,
                    bytes,
                });
                cursor.align(8)?;
            }
            (
                Some(UserDataPlan {
                    records_offset,
                    dictionary_offset,
                    values,
                    dictionary_nodes,
                }),
                cursor.position(),
            )
        };
        Ok((
            Self {
                offset,
                user_data,
                image_start: 0,
            },
            metadata_end,
        ))
    }

    fn runtime_offset(&self) -> u64 {
        self.offset + TEXTURE_RUNTIME_OFFSET
    }

    fn view_offset(&self) -> u64 {
        self.offset + TEXTURE_VIEW_OFFSET
    }

    fn mip_table_offset(&self) -> u64 {
        self.offset + MIP_TABLE_OFFSET
    }

    fn write(
        &self,
        output: &mut Output,
        texture: &Texture,
        strings: &StringTable,
        next_offset: u64,
    ) -> BinResult<()> {
        let block_size = next_offset - self.offset;
        output.write_at(
            self.offset,
            &TextureInfoDisk {
                block_size: block_size as u32,
                flags: texture.info.flags,
                image_dimension: texture.info.image_dimension.to_raw(),
                tile_mode: texture.info.tile_mode.to_raw(),
                swizzle: texture.info.swizzle,
                mip_count: u16::try_from(texture.mip_offsets.len())
                    .map_err(|_| bntx_error(self.offset, "BNTX mip count exceeds 65535"))?,
                sample_count: texture.info.sample_count,
                format: texture.info.format.to_raw(),
                gpu_access: texture.info.gpu_access,
                width: texture.info.width,
                height: texture.info.height,
                depth: texture.info.depth,
                array_count: texture.info.array_count,
                texture_layout: texture.info.texture_layout.to_raw(),
                texture_layout2: texture.info.texture_layout2,
                reserved: texture.info.reserved,
                image_size: u32::try_from(texture.image_data.len())
                    .map_err(|_| bntx_error(self.offset, "BNTX image payload exceeds 4 GiB"))?,
                image_alignment: texture.info.image_alignment,
                channel_red: texture.info.channels[0].to_raw(),
                channel_green: texture.info.channels[1].to_raw(),
                channel_blue: texture.info.channels[2].to_raw(),
                channel_alpha: texture.info.channels[3].to_raw(),
                surface_dimension: texture.info.surface_dimension.to_raw(),
                name_offset: strings.offset(&texture.name)?,
                mip_offsets_offset: self.mip_table_offset(),
                user_data_offset: self
                    .user_data
                    .as_ref()
                    .map_or(0, |user_data| user_data.records_offset),
                texture_pointer: self.runtime_offset(),
                texture_view_pointer: self.view_offset(),
                descriptor_slot_offset: 0,
                user_data_dictionary_offset: self
                    .user_data
                    .as_ref()
                    .map_or(0, |user_data| user_data.dictionary_offset),
            },
            "BRTI record",
        )?;
        for (index, &mip_offset) in texture.mip_offsets.iter().enumerate() {
            output.write_at(
                self.mip_table_offset() + index as u64 * 8,
                &(self.image_start + mip_offset),
                "mip pointer",
            )?;
        }
        if let Some(user_data) = &self.user_data {
            user_data.write(output, texture, strings)?;
        }
        output.write_raw(self.image_start, &texture.image_data, "image payload")
    }
}

fn plan_images(
    bntx: &Bntx,
    textures: &mut [TexturePlan],
    brtd: u64,
    container_alignment: u64,
) -> BinResult<u64> {
    let mut cursor = LayoutCursor::new(brtd + BLOCK_HEADER_SIZE);
    for (texture, plan) in bntx.textures.iter().zip(textures) {
        cursor.align(u64::from(texture.info.image_alignment))?;
        plan.image_start = cursor.advance(texture.image_data.len() as u64, "image payloads")?;
    }
    cursor.align(container_alignment)?;
    Ok(cursor.position())
}

fn relocation_pointers(
    bntx: &Bntx,
    textures: &[TexturePlan],
    dictionary_offset: u64,
    dictionary_node_count: usize,
) -> (Vec<u64>, Vec<u64>) {
    let mut metadata = vec![0x28, 0x38, 0x40];
    let mut data = vec![0x30];
    metadata.extend((0..textures.len()).map(|index| POINTER_TABLE_OFFSET + index as u64 * 8));
    append_dictionary_pointers(&mut metadata, dictionary_offset, dictionary_node_count);
    for (texture, plan) in bntx.textures.iter().zip(textures) {
        metadata.extend([
            plan.offset + 0x60,
            plan.offset + 0x68,
            plan.offset + 0x70,
            plan.offset + 0x80,
            plan.offset + 0x88,
        ]);
        if let Some(user_data) = &plan.user_data {
            metadata.extend([plan.offset + 0x78, plan.offset + 0x98]);
            for index in 0..texture.user_data.len() {
                metadata.extend([
                    user_data.records_offset + index as u64 * USER_DATA_SIZE,
                    user_data.records_offset + index as u64 * USER_DATA_SIZE + 8,
                ]);
            }
            append_dictionary_pointers(
                &mut metadata,
                user_data.dictionary_offset,
                user_data.dictionary_nodes.len(),
            );
        }
        data.extend(
            (0..texture.mip_offsets.len()).map(|index| plan.mip_table_offset() + index as u64 * 8),
        );
    }
    (metadata, data)
}

struct Layout {
    dictionary: u64,
    strings: StringTable,
    dictionary_nodes: Vec<DictionaryNodeDisk>,
    textures: Vec<TexturePlan>,
    brtd: u64,
    relocation: u64,
    relocation_table: relocation::Table,
    file_size: u64,
}

impl Layout {
    fn plan(bntx: &Bntx) -> BinResult<Self> {
        validation::validate(bntx)?;
        let texture_count = bntx.textures.len();
        let mut cursor = LayoutCursor::new(POINTER_TABLE_OFFSET);
        cursor.advance(texture_count as u64 * 8, "texture pointer table")?;
        cursor.align(8)?;
        let string_block = cursor.position();
        let strings = StringTable::plan(bntx, string_block)?;
        let dictionary = strings.dictionary_offset()?;
        let texture_keys = bntx
            .textures
            .iter()
            .map(|texture| {
                let offset = strings.offset(&texture.name)?;
                Ok((texture.name.as_str(), offset))
            })
            .collect::<BinResult<Vec<_>>>()?;
        let dictionary_nodes = dictionary::build(&texture_keys, strings.empty_offset())?;
        let dictionary_size = 8 + dictionary_nodes.len() as u64 * DICTIONARY_NODE_SIZE;

        let mut texture_plans = Vec::with_capacity(texture_count);
        let mut cursor = LayoutCursor::new(dictionary);
        cursor.advance(dictionary_size, "texture dictionary")?;
        for texture in &bntx.textures {
            let (plan, metadata_end) = TexturePlan::plan(texture, cursor.position(), &strings)?;
            texture_plans.push(plan);
            cursor = LayoutCursor::new(metadata_end);
        }
        let metadata_end = cursor.position();
        let alignment = 1u64 << bntx.alignment_exponent;
        cursor.advance(BLOCK_HEADER_SIZE, "metadata")?;
        cursor.align(alignment)?;
        let brtd = cursor.position() - BLOCK_HEADER_SIZE;

        let relocation = plan_images(bntx, &mut texture_plans, brtd, alignment)?;
        let (metadata_pointers, data_pointers) =
            relocation_pointers(bntx, &texture_plans, dictionary, dictionary_nodes.len());
        let texture_offsets = texture_plans
            .iter()
            .map(|texture| texture.offset)
            .collect::<Vec<_>>();
        let mip_tables = texture_plans
            .iter()
            .map(TexturePlan::mip_table_offset)
            .collect::<Vec<_>>();
        let relocation_table = relocation::build(
            CanonicalLayout {
                pointer_table: POINTER_TABLE_OFFSET,
                dictionary,
                texture_offsets: &texture_offsets,
                mip_tables: &mip_tables,
                metadata_end,
                brtd,
                relocation,
            },
            &metadata_pointers,
            &data_pointers,
        )?;
        let mut cursor = LayoutCursor::new(relocation);
        cursor.advance(
            0x10 + RELOCATION_SECTION_SIZE * 2
                + relocation_table.entries.len() as u64 * RELOCATION_ENTRY_SIZE,
            "relocation table",
        )?;
        let file_size = cursor.position();
        u32::try_from(file_size)
            .map_err(|_| bntx_error(file_size, "BNTX file size exceeds 4 GiB"))?;

        Ok(Self {
            dictionary,
            strings,
            dictionary_nodes,
            textures: texture_plans,
            brtd,
            relocation,
            relocation_table,
            file_size,
        })
    }

    fn write_headers(&self, output: &mut Output, bntx: &Bntx) -> BinResult<()> {
        let name_record = self.strings.offset(&bntx.name)?;
        output.write_at(
            0,
            &FileHeader {
                // Unlike other string pointers, the file header points past
                // the u16 length prefix to the container-name UTF-8 bytes.
                name_offset: (name_record + 2) as u32,
                first_block_offset: u16::try_from(self.strings.block_offset).map_err(|_| {
                    bntx_error(
                        self.strings.block_offset,
                        "BNTX first block offset exceeds 65535",
                    )
                })?,
                relocation_offset: self.relocation as u32,
                file_size: self.file_size as u32,
            },
            "file header",
        )?;
        output.write_at(
            FILE_HEADER_SIZE,
            &NxHeader {
                texture_count: u32::try_from(bntx.textures.len())
                    .map_err(|_| bntx_error(0x24, "BNTX texture count exceeds u32"))?,
                texture_info_array_offset: POINTER_TABLE_OFFSET,
                texture_data_block_offset: self.brtd,
                texture_dictionary_offset: self.dictionary,
            },
            "NX header",
        )?;
        for (index, texture) in self.textures.iter().enumerate() {
            output.write_at(
                POINTER_TABLE_OFFSET + index as u64 * 8,
                &texture.offset,
                "texture pointer",
            )?;
        }
        Ok(())
    }

    fn write_metadata(&self, output: &mut Output, bntx: &Bntx) -> BinResult<()> {
        let first_texture = self
            .textures
            .first()
            .map_or(self.brtd, |texture| texture.offset);
        self.strings.write(output, first_texture)?;
        write_dictionary(
            output,
            self.dictionary,
            &self.dictionary_nodes,
            "texture dictionary",
        )?;
        for (index, (texture, plan)) in bntx.textures.iter().zip(&self.textures).enumerate() {
            let next_offset = self
                .textures
                .get(index + 1)
                .map_or(self.brtd, |texture| texture.offset);
            plan.write(output, texture, &self.strings, next_offset)?;
        }
        Ok(())
    }

    fn write_data_and_relocations(&self, output: &mut Output) -> BinResult<()> {
        output.write_at(
            self.brtd,
            &TextureDataHeader {
                block_size: self.relocation - self.brtd,
            },
            "BRTD header",
        )?;
        output.write_at(
            self.relocation,
            &RelocationHeader {
                table_offset: self.relocation as u32,
            },
            "relocation header",
        )?;
        let mut position = self.relocation + 0x10;
        for section in &self.relocation_table.sections {
            output.write_at(position, section, "relocation section")?;
            position += RELOCATION_SECTION_SIZE;
        }
        for entry in &self.relocation_table.entries {
            output.write_at(position, entry, "relocation entry")?;
            position += RELOCATION_ENTRY_SIZE;
        }
        Ok(())
    }

    fn write(self, bntx: &Bntx) -> BinResult<Vec<u8>> {
        let mut output = Output::new(self.file_size)?;
        self.write_headers(&mut output, bntx)?;
        self.write_metadata(&mut output, bntx)?;
        self.write_data_and_relocations(&mut output)?;
        Ok(output.bytes)
    }
}

struct Output {
    bytes: Vec<u8>,
}

impl Output {
    fn new(size: u64) -> BinResult<Self> {
        let size = usize::try_from(size)
            .map_err(|_| bntx_error(0, "BNTX output size exceeds addressable memory"))?;
        Ok(Self {
            bytes: zeroed_bytes(size, "output")?,
        })
    }

    fn write_at<T>(&mut self, offset: u64, value: &T, label: &str) -> BinResult<()>
    where
        T: for<'args> BinWrite<Args<'args> = ()>,
    {
        if offset > self.bytes.len() as u64 {
            return Err(bntx_error(
                offset,
                format!("BNTX {label} is outside output"),
            ));
        }
        let mut writer = Cursor::new(self.bytes.as_mut_slice());
        writer.set_position(offset);
        value.write_options(&mut writer, Endian::Little, ())
    }

    fn write_raw(&mut self, offset: u64, value: &[u8], label: &str) -> BinResult<()> {
        let start = usize::try_from(offset)
            .map_err(|_| bntx_error(offset, format!("BNTX {label} offset is too large")))?;
        let end = start
            .checked_add(value.len())
            .filter(|&end| end <= self.bytes.len())
            .ok_or_else(|| bntx_error(offset, format!("BNTX {label} exceeds output")))?;
        self.bytes[start..end].copy_from_slice(value);
        Ok(())
    }
}

fn ordered_strings(bntx: &Bntx) -> Vec<String> {
    let mut required = Vec::new();
    required.push(bntx.name.as_str());
    for texture in &bntx.textures {
        required.push(texture.name.as_str());
        required.extend(texture.user_data.iter().map(|value| value.name.as_str()));
    }
    let required_set = required.iter().copied().collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let mut ordered = Vec::new();
    for value in bntx
        .source_string_order
        .iter()
        .map(String::as_str)
        .chain(required.iter().copied())
    {
        if !value.is_empty() && required_set.contains(value) && seen.insert(value.to_owned()) {
            ordered.push(value.to_owned());
        }
    }
    ordered
}

fn encode_user_value(value: &UserDataValue) -> BinResult<(u32, u8, Vec<u8>)> {
    let mut bytes = Vec::new();
    let (count, value_type) = match value {
        UserDataValue::Int32(values) => {
            for value in values {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            (values.len(), 0)
        }
        UserDataValue::Single(values) => {
            for value in values {
                bytes.extend_from_slice(&value.to_bits().to_le_bytes());
            }
            (values.len(), 1)
        }
        UserDataValue::String(values) => {
            for value in values {
                bytes.extend_from_slice(&encode_utf8_record(value)?);
            }
            (values.len(), 2)
        }
        UserDataValue::Byte(values) => {
            bytes.extend_from_slice(values);
            (values.len(), 3)
        }
        UserDataValue::WString(values) => {
            for value in values {
                let units = value.encode_utf16().collect::<Vec<_>>();
                bytes.extend_from_slice(&(units.len() as u16).to_le_bytes());
                for unit in units {
                    bytes.extend_from_slice(&unit.to_le_bytes());
                }
                bytes.extend_from_slice(&0u16.to_le_bytes());
            }
            (values.len(), 4)
        }
    };
    Ok((
        u32::try_from(count).map_err(|_| bntx_error(0, "BNTX user-data count exceeds u32"))?,
        value_type,
        bytes,
    ))
}

fn write_dictionary(
    output: &mut Output,
    offset: u64,
    nodes: &[DictionaryNodeDisk],
    label: &str,
) -> BinResult<()> {
    let entry_count = nodes
        .len()
        .checked_sub(1)
        .ok_or_else(|| bntx_error(offset, format!("BNTX {label} has no root")))?;
    output.write_at(
        offset,
        &DictionaryHeader {
            entry_count: u32::try_from(entry_count)
                .map_err(|_| bntx_error(offset, format!("BNTX {label} is too large")))?,
        },
        label,
    )?;
    for (index, node) in nodes.iter().enumerate() {
        output.write_at(
            offset + 8 + index as u64 * DICTIONARY_NODE_SIZE,
            node,
            "dictionary node",
        )?;
    }
    Ok(())
}

fn append_dictionary_pointers(values: &mut Vec<u64>, offset: u64, node_count: usize) {
    values.extend((0..node_count).map(|index| offset + 0x10 + index as u64 * 0x10));
}

fn encode_utf8_record(value: &str) -> BinResult<Vec<u8>> {
    let mut bytes = Vec::with_capacity(utf8_record_size(value)?);
    bytes.extend_from_slice(
        &u16::try_from(value.len())
            .map_err(|_| bntx_error(0, "BNTX string exceeds 65535 bytes"))?
            .to_le_bytes(),
    );
    bytes.extend_from_slice(value.as_bytes());
    bytes.push(0);
    if !bytes.len().is_multiple_of(2) {
        bytes.push(0);
    }
    Ok(bytes)
}

fn utf8_record_size(value: &str) -> BinResult<usize> {
    if value.len() > usize::from(u16::MAX) {
        return Err(bntx_error(0, "BNTX string exceeds 65535 bytes"));
    }
    Ok((value.len() + 4) & !1)
}

fn align_up(value: u64, alignment: u64) -> BinResult<u64> {
    if alignment == 0 || !alignment.is_power_of_two() {
        return Err(bntx_error(value, "invalid BNTX alignment"));
    }
    value
        .checked_add(alignment - 1)
        .map(|value| value & !(alignment - 1))
        .ok_or_else(|| bntx_error(value, "BNTX aligned offset overflows"))
}
