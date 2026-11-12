use super::binary::{
    BLOCK_HEADER_SIZE, BRTI_SIZE, DictionaryHeader, FILE_HEADER_SIZE, FileHeader, NX_HEADER_SIZE,
    NxHeader, StringPoolHeader, TextureDataHeader, TextureInfoDisk, USER_DATA_SIZE, UserDataDisk,
};
use super::bntx_error;
use binrw::BinResult;
use std::collections::{HashMap, HashSet};

use super::format::{
    ChannelSelector, ImageFormat, Platform, SurfaceDimension, TextureDimension, TextureLayout,
    TileMode,
};
use super::input::{Input, checked_mul, ensure_contained, ensure_range};
use super::model::{Bntx, FloatValue, Texture, TextureInfo, UserData, UserDataValue, Version};

const SUPPORTED_MAJOR_VERSION: u16 = 4;
const SUPPORTED_MINOR_VERSION: u8 = 0;
const SUPPORTED_MICRO_VERSION: u8 = 0;
const SUPPORTED_ALIGNMENT_EXPONENT: u8 = 12;
const SUPPORTED_ADDRESS_SIZE: u8 = 64;
const NX_MEMORY_POOL_SIZE: u64 = 0x140;
const MAX_TEXTURES: usize = u16::MAX as usize;

pub(crate) fn parse(bytes: &[u8]) -> BinResult<Bntx> {
    Parser::new(bytes)?.parse()
}

struct StringPool {
    values: HashMap<u64, String>,
    order: Vec<String>,
    block_end: u64,
}

impl StringPool {
    fn resolve(&self, offset: u64, position: u64) -> BinResult<&str> {
        self.values
            .get(&offset)
            .map(String::as_str)
            .ok_or_else(|| bntx_error(position, "BNTX string pointer does not name a pool entry"))
    }
}

#[derive(Clone, Copy)]
struct FileRange {
    start: u64,
    end: u64,
}

struct ParsedImage {
    mip_offsets: Vec<u64>,
    data: Vec<u8>,
    range: FileRange,
}

struct ParsedTexture {
    texture: Texture,
    block_offset: u64,
    payload_range: FileRange,
}

struct Parser<'a> {
    input: Input<'a>,
    header: FileHeader,
}

impl<'a> Parser<'a> {
    fn new(bytes: &'a [u8]) -> BinResult<Self> {
        let input = Input::new(bytes);
        let header: FileHeader = input.read_at(0, FILE_HEADER_SIZE, "BNTX header")?;
        let declared_size = usize::try_from(header.file_size)
            .map_err(|_| bntx_error(0x1c, "BNTX declared file size is too large"))?;
        if declared_size != bytes.len() {
            return Err(bntx_error(
                0x1c,
                format!(
                    "BNTX declared file size {declared_size} does not match {} available bytes",
                    bytes.len()
                ),
            ));
        }
        Ok(Self { input, header })
    }

    fn parse(self) -> BinResult<Bntx> {
        self.validate_file_layout()?;
        let nx: NxHeader =
            self.input
                .read_at(FILE_HEADER_SIZE, NX_HEADER_SIZE, "BNTX NX header")?;
        self.validate_nx_pointers(&nx)?;

        let texture_count = usize::try_from(nx.texture_count)
            .map_err(|_| bntx_error(0x24, "BNTX texture count is too large"))?;
        if texture_count > MAX_TEXTURES {
            return Err(bntx_error(0x24, "BNTX texture count exceeds 65535"));
        }
        let pointer_array_size = checked_mul(texture_count as u64, 8, 0x24, "texture pointers")?;
        ensure_range(
            self.input.len(),
            nx.texture_info_array_offset,
            pointer_array_size,
            "BNTX texture pointer table",
        )?;
        let pointer_array_end = nx
            .texture_info_array_offset
            .checked_add(pointer_array_size)
            .ok_or_else(|| bntx_error(0x28, "BNTX texture pointer table overflows"))?;
        if pointer_array_end > u64::from(self.header.first_block_offset) {
            return Err(bntx_error(
                0x28,
                "BNTX texture pointer table overlaps the first block",
            ));
        }
        let texture_offsets = (0..texture_count)
            .map(|index| {
                self.input
                    .read_u64(nx.texture_info_array_offset + index as u64 * 8)
            })
            .collect::<BinResult<Vec<_>>>()?;
        validate_strictly_increasing(&texture_offsets, 0x28, "BNTX texture pointers")?;

        let strings = self.parse_string_pool(
            u64::from(self.header.first_block_offset),
            nx.texture_dictionary_offset,
        )?;
        let expected_first_block = texture_offsets
            .first()
            .copied()
            .unwrap_or(nx.texture_data_block_offset);
        if strings.block_end != expected_first_block {
            return Err(bntx_error(
                u64::from(self.header.first_block_offset) + 4,
                "BNTX string block does not lead to the first texture block",
            ));
        }

        let file_name_offset = u64::from(self.header.name_offset)
            .checked_sub(2)
            .ok_or_else(|| bntx_error(0x10, "BNTX filename pointer is smaller than its prefix"))?;
        let name = strings.resolve(file_name_offset, 0x10)?.to_owned();

        let brtd_offset = nx.texture_data_block_offset;
        let brtd: TextureDataHeader =
            self.input
                .read_at(brtd_offset, BLOCK_HEADER_SIZE, "BNTX BRTD header")?;
        self.validate_brtd(brtd_offset, &brtd)?;

        let mut parsed_textures = Vec::with_capacity(texture_count);
        let mut names = HashSet::with_capacity(texture_count);
        for (index, &offset) in texture_offsets.iter().enumerate() {
            let expected_next = texture_offsets
                .get(index + 1)
                .copied()
                .unwrap_or(brtd_offset);
            let parsed = self.parse_texture(offset, expected_next, brtd_offset, &strings)?;
            if !names.insert(parsed.texture.name.clone()) {
                return Err(bntx_error(
                    offset + 0x60,
                    format!("duplicate BNTX texture name {:?}", parsed.texture.name),
                ));
            }
            parsed_textures.push(parsed);
        }
        validate_payload_ranges(&parsed_textures)?;

        let textures = parsed_textures
            .into_iter()
            .map(|parsed| parsed.texture)
            .collect::<Vec<_>>();
        Ok(Bntx {
            version: Version {
                major: SUPPORTED_MAJOR_VERSION,
                minor: SUPPORTED_MINOR_VERSION,
                micro: SUPPORTED_MICRO_VERSION,
            },
            alignment_exponent: SUPPORTED_ALIGNMENT_EXPONENT,
            target_address_size: SUPPORTED_ADDRESS_SIZE,
            flag: 0,
            name,
            platform: Platform::Nx,
            textures,
            source_string_order: strings.order,
        })
    }

    fn validate_file_layout(&self) -> BinResult<()> {
        if u64::from(self.header.first_block_offset) < FILE_HEADER_SIZE + NX_HEADER_SIZE {
            return Err(bntx_error(0x16, "BNTX first block overlaps the NX header"));
        }
        let relocation_offset = u64::from(self.header.relocation_offset);
        ensure_range(
            self.input.len(),
            relocation_offset,
            BLOCK_HEADER_SIZE,
            "BNTX relocation header",
        )
    }

    fn validate_nx_pointers(&self, nx: &NxHeader) -> BinResult<()> {
        if nx.texture_info_array_offset < FILE_HEADER_SIZE + NX_HEADER_SIZE + NX_MEMORY_POOL_SIZE
            || nx.texture_dictionary_offset == 0
            || nx.texture_data_block_offset == 0
        {
            return Err(bntx_error(0x28, "invalid BNTX NX section pointers"));
        }
        Ok(())
    }

    fn parse_string_pool(&self, offset: u64, dictionary_offset: u64) -> BinResult<StringPool> {
        let header: StringPoolHeader =
            self.input
                .read_at(offset, BLOCK_HEADER_SIZE + 8, "BNTX string-pool header")?;
        let block_end = offset
            .checked_add(u64::from(header.block_size))
            .ok_or_else(|| bntx_error(offset + 8, "BNTX string-pool size overflows"))?;
        ensure_range(
            self.input.len(),
            offset,
            u64::from(header.block_size),
            "BNTX string-pool block",
        )?;

        let count = usize::try_from(header.string_count)
            .map_err(|_| bntx_error(offset + 0x10, "BNTX string count is too large"))?;
        if count > self.input.len() / 4 {
            return Err(bntx_error(
                offset + 0x10,
                "BNTX string count exceeds available bytes",
            ));
        }
        let empty_string_offset = offset + 0x14;
        let mut values = HashMap::with_capacity(count.saturating_add(1));
        let mut order = Vec::with_capacity(count);
        values.insert(empty_string_offset, String::new());
        let mut position = offset + 0x18;
        for _ in 0..count {
            let (value, consumed) = self.input.read_utf8_record(position, block_end)?;
            if values.insert(position, value.clone()).is_some() {
                return Err(bntx_error(position, "duplicate BNTX string offset"));
            }
            order.push(value);
            position = position
                .checked_add(consumed)
                .ok_or_else(|| bntx_error(position, "BNTX string table overflows"))?;
        }
        if position > block_end {
            return Err(bntx_error(position, "BNTX strings exceed their block"));
        }
        if position > dictionary_offset || dictionary_offset >= block_end {
            return Err(bntx_error(
                position,
                "BNTX string records overlap the texture dictionary",
            ));
        }
        let padding = self.input.slice(
            position,
            dictionary_offset - position,
            "BNTX string-pool padding",
        )?;
        if padding.iter().any(|&byte| byte != 0) {
            return Err(bntx_error(
                position,
                "BNTX string-pool alignment padding is nonzero",
            ));
        }

        Ok(StringPool {
            values,
            order,
            block_end,
        })
    }

    fn parse_texture(
        &self,
        offset: u64,
        expected_next: u64,
        brtd_offset: u64,
        strings: &StringPool,
    ) -> BinResult<ParsedTexture> {
        let disk: TextureInfoDisk = self.input.read_at(offset, BRTI_SIZE, "BNTX BRTI record")?;
        let block_end = offset
            .checked_add(u64::from(disk.block_size))
            .ok_or_else(|| bntx_error(offset + 8, "BNTX BRTI block size overflows"))?;
        if block_end != expected_next {
            return Err(bntx_error(
                offset + 4,
                "BNTX BRTI block chain is inconsistent",
            ));
        }

        let info = texture_info(&disk);
        self.validate_texture_profile(&disk, &info, offset, block_end)?;

        let name = strings.resolve(disk.name_offset, offset + 0x60)?.to_owned();
        if name.is_empty() {
            return Err(bntx_error(
                offset + 0x60,
                "BNTX texture name must not be empty",
            ));
        }

        let image = self.parse_image(&disk, offset, block_end, brtd_offset)?;
        let user_data = self.parse_user_data(&disk, offset, block_end, strings)?;

        Ok(ParsedTexture {
            texture: Texture {
                name,
                info,
                mip_offsets: image.mip_offsets,
                user_data,
                image_data: image.data,
            },
            block_offset: offset,
            payload_range: image.range,
        })
    }

    fn validate_texture_profile(
        &self,
        disk: &TextureInfoDisk,
        info: &TextureInfo,
        offset: u64,
        block_end: u64,
    ) -> BinResult<()> {
        if info.image_dimension != TextureDimension::TwoD
            || info.tile_mode != TileMode::BlockLinear
            || info.swizzle != 0
            || info.sample_count != 1
            || info.surface_dimension != SurfaceDimension::TwoD
            || info.depth != 1
            || info.array_count != 1
            || info.image_alignment != 0x200
        {
            return Err(bntx_error(
                offset + 0x10,
                "unsupported BNTX texture profile (expected 2D block-linear NX texture)",
            ));
        }
        if info.width == 0 || info.height == 0 {
            return Err(bntx_error(
                offset + 0x24,
                "BNTX texture dimensions must be nonzero",
            ));
        }
        if info.texture_layout.block_height_exponent() > 5 {
            return Err(bntx_error(
                offset + 0x34,
                "unsupported BNTX block-height exponent",
            ));
        }
        if disk.mip_count == 0 {
            return Err(bntx_error(
                offset + 0x16,
                "BNTX texture mip count must be nonzero",
            ));
        }
        for (pointer_position, pointer, label) in [
            (
                offset + 0x80,
                disk.texture_pointer,
                "texture runtime pointer",
            ),
            (
                offset + 0x88,
                disk.texture_view_pointer,
                "texture-view runtime pointer",
            ),
        ] {
            if pointer < offset + BRTI_SIZE || pointer >= block_end {
                return Err(bntx_error(
                    pointer_position,
                    format!("BNTX {label} is outside its BRTI block"),
                ));
            }
        }
        if disk.descriptor_slot_offset != 0
            && disk.descriptor_slot_offset >= self.input.len() as u64
        {
            return Err(bntx_error(
                offset + 0x90,
                "BNTX descriptor-slot pointer is outside the file",
            ));
        }
        Ok(())
    }

    fn parse_image(
        &self,
        disk: &TextureInfoDisk,
        offset: u64,
        block_end: u64,
        brtd_offset: u64,
    ) -> BinResult<ParsedImage> {
        let mip_count = usize::from(disk.mip_count);
        let mip_table_size = checked_mul(mip_count as u64, 8, offset + 0x70, "mip pointers")?;
        ensure_range(
            self.input.len(),
            disk.mip_offsets_offset,
            mip_table_size,
            "BNTX mip pointer table",
        )?;
        if disk.mip_offsets_offset < offset + BRTI_SIZE
            || disk.mip_offsets_offset + mip_table_size > block_end
        {
            return Err(bntx_error(
                offset + 0x70,
                "BNTX mip pointer table is outside its BRTI block",
            ));
        }
        let absolute_mips = (0..mip_count)
            .map(|index| {
                self.input
                    .read_u64(disk.mip_offsets_offset + index as u64 * 8)
            })
            .collect::<BinResult<Vec<_>>>()?;
        validate_strictly_increasing_or_single(&absolute_mips, offset + 0x70, "BNTX mip pointers")?;
        let image_start = absolute_mips[0];
        let image_end = image_start
            .checked_add(u64::from(disk.image_size))
            .ok_or_else(|| bntx_error(offset + 0x50, "BNTX image payload size overflows"))?;
        if disk.image_size == 0
            || image_start < brtd_offset + BLOCK_HEADER_SIZE
            || image_end > u64::from(self.header.relocation_offset)
        {
            return Err(bntx_error(
                offset + 0x50,
                "BNTX image payload is outside the declared data region",
            ));
        }
        if !image_start.is_multiple_of(u64::from(disk.image_alignment)) {
            return Err(bntx_error(
                offset + 0x70,
                "BNTX image payload does not satisfy its declared alignment",
            ));
        }
        if absolute_mips
            .iter()
            .any(|&mip| mip < image_start || mip >= image_end)
        {
            return Err(bntx_error(
                offset + 0x70,
                "BNTX mip pointer is outside its image payload",
            ));
        }
        let mip_offsets = absolute_mips
            .iter()
            .map(|&mip| mip - image_start)
            .collect::<Vec<_>>();
        let data = self
            .input
            .slice(
                image_start,
                u64::from(disk.image_size),
                "BNTX image payload",
            )?
            .to_vec();
        Ok(ParsedImage {
            mip_offsets,
            data,
            range: FileRange {
                start: image_start,
                end: image_end,
            },
        })
    }

    fn parse_user_data(
        &self,
        texture: &TextureInfoDisk,
        texture_offset: u64,
        block_end: u64,
        strings: &StringPool,
    ) -> BinResult<Vec<UserData>> {
        match (
            texture.user_data_offset,
            texture.user_data_dictionary_offset,
        ) {
            (0, 0) => return Ok(Vec::new()),
            (0, _) | (_, 0) => {
                return Err(bntx_error(
                    texture_offset + 0x78,
                    "BNTX user-data array and dictionary pointers must both be present",
                ));
            }
            _ => {}
        }

        let dictionary_header: DictionaryHeader = self.input.read_at(
            texture.user_data_dictionary_offset,
            8,
            "BNTX user-data dictionary header",
        )?;
        let count = usize::try_from(dictionary_header.entry_count).map_err(|_| {
            bntx_error(
                texture.user_data_dictionary_offset + 4,
                "BNTX user-data count is too large",
            )
        })?;
        let records_size = checked_mul(count as u64, USER_DATA_SIZE, texture_offset, "user data")?;
        ensure_range(
            self.input.len(),
            texture.user_data_offset,
            records_size,
            "BNTX user-data records",
        )?;
        ensure_contained(
            texture.user_data_offset,
            records_size,
            texture_offset + BRTI_SIZE,
            block_end,
            texture_offset + 0x78,
            "BNTX user-data records",
        )?;
        let records_end = texture.user_data_offset + records_size;
        if texture.user_data_dictionary_offset < records_end {
            return Err(bntx_error(
                texture_offset + 0x98,
                "BNTX user-data dictionary overlaps its record array",
            ));
        }
        let mut values = Vec::with_capacity(count);
        for index in 0..count {
            let position = texture.user_data_offset + index as u64 * USER_DATA_SIZE;
            let disk: UserDataDisk =
                self.input
                    .read_at(position, USER_DATA_SIZE, "BNTX user-data record")?;
            let name = strings.resolve(disk.name_offset, position)?.to_owned();
            let value = self.parse_user_value(&disk, position, records_end, block_end)?;
            values.push(UserData { name, value });
        }
        Ok(values)
    }

    fn parse_user_value(
        &self,
        disk: &UserDataDisk,
        position: u64,
        minimum_value_offset: u64,
        block_end: u64,
    ) -> BinResult<UserDataValue> {
        let count = usize::try_from(disk.count)
            .map_err(|_| bntx_error(position + 0x10, "BNTX user-data count is too large"))?;
        if count > self.input.len() {
            return Err(bntx_error(
                position + 0x10,
                "BNTX user-data count exceeds available bytes",
            ));
        }
        match disk.value_type {
            0 => {
                let byte_count = checked_mul(count as u64, 4, position, "i32 user data")?;
                ensure_range(
                    self.input.len(),
                    disk.value_offset,
                    byte_count,
                    "BNTX i32 user data",
                )?;
                ensure_contained(
                    disk.value_offset,
                    byte_count,
                    minimum_value_offset,
                    block_end,
                    position + 8,
                    "BNTX i32 user data",
                )?;
                let values = (0..count)
                    .map(|index| {
                        self.input
                            .read_u32(disk.value_offset + index as u64 * 4)
                            .map(|value| value as i32)
                    })
                    .collect::<BinResult<Vec<_>>>()?;
                Ok(UserDataValue::Int32(values))
            }
            1 => {
                let byte_count = checked_mul(count as u64, 4, position, "f32 user data")?;
                ensure_range(
                    self.input.len(),
                    disk.value_offset,
                    byte_count,
                    "BNTX f32 user data",
                )?;
                ensure_contained(
                    disk.value_offset,
                    byte_count,
                    minimum_value_offset,
                    block_end,
                    position + 8,
                    "BNTX f32 user data",
                )?;
                let values = (0..count)
                    .map(|index| {
                        self.input
                            .read_u32(disk.value_offset + index as u64 * 4)
                            .map(FloatValue::from_bits)
                    })
                    .collect::<BinResult<Vec<_>>>()?;
                Ok(UserDataValue::Single(values))
            }
            2 => {
                if disk.value_offset < minimum_value_offset {
                    return Err(bntx_error(
                        position + 8,
                        "BNTX string user data overlaps its record array",
                    ));
                }
                let mut values = Vec::with_capacity(count);
                let mut value_position = disk.value_offset;
                for _ in 0..count {
                    let (value, consumed) =
                        self.input.read_utf8_record(value_position, block_end)?;
                    values.push(value);
                    value_position = value_position.checked_add(consumed).ok_or_else(|| {
                        bntx_error(position, "BNTX string user-data range overflows")
                    })?;
                }
                Ok(UserDataValue::String(values))
            }
            3 => {
                ensure_contained(
                    disk.value_offset,
                    count as u64,
                    minimum_value_offset,
                    block_end,
                    position + 8,
                    "BNTX byte user data",
                )?;
                Ok(UserDataValue::Byte(
                    self.input
                        .slice(disk.value_offset, count as u64, "BNTX byte user data")?
                        .to_vec(),
                ))
            }
            4 => {
                if disk.value_offset < minimum_value_offset {
                    return Err(bntx_error(
                        position + 8,
                        "BNTX wide-string user data overlaps its record array",
                    ));
                }
                let mut values = Vec::with_capacity(count);
                let mut value_position = disk.value_offset;
                for _ in 0..count {
                    let (value, consumed) = self.input.read_utf16_record(value_position)?;
                    values.push(value);
                    value_position = value_position.checked_add(consumed).ok_or_else(|| {
                        bntx_error(position, "BNTX wide-string user-data range overflows")
                    })?;
                }
                if value_position > block_end {
                    return Err(bntx_error(
                        position + 8,
                        "BNTX wide-string user data exceeds its BRTI block",
                    ));
                }
                Ok(UserDataValue::WString(values))
            }
            value_type => Err(bntx_error(
                position + 0x14,
                format!("unsupported BNTX user-data type {value_type}"),
            )),
        }
    }

    fn validate_brtd(&self, offset: u64, header: &TextureDataHeader) -> BinResult<()> {
        let relocation_offset = u64::from(self.header.relocation_offset);
        let end = offset
            .checked_add(header.block_size)
            .ok_or_else(|| bntx_error(offset + 8, "BNTX BRTD block size overflows"))?;
        if end != relocation_offset || header.block_size < BLOCK_HEADER_SIZE {
            return Err(bntx_error(
                offset + 8,
                "BNTX BRTD block does not end at the relocation table",
            ));
        }
        let alignment = 1u64 << SUPPORTED_ALIGNMENT_EXPONENT;
        if !(offset + BLOCK_HEADER_SIZE).is_multiple_of(alignment) {
            return Err(bntx_error(
                offset,
                "BNTX image-data payload is not aligned to the container alignment",
            ));
        }
        Ok(())
    }
}

fn texture_info(disk: &TextureInfoDisk) -> TextureInfo {
    TextureInfo {
        flags: disk.flags,
        image_dimension: TextureDimension::from_raw(disk.image_dimension),
        tile_mode: TileMode::from_raw(disk.tile_mode),
        swizzle: disk.swizzle,
        sample_count: disk.sample_count,
        format: ImageFormat::from_raw(disk.format),
        gpu_access: disk.gpu_access,
        width: disk.width,
        height: disk.height,
        depth: disk.depth,
        array_count: disk.array_count,
        texture_layout: TextureLayout::from_raw(disk.texture_layout),
        texture_layout2: disk.texture_layout2,
        reserved: disk.reserved,
        image_alignment: disk.image_alignment,
        channels: [
            ChannelSelector::from_raw(disk.channel_red),
            ChannelSelector::from_raw(disk.channel_green),
            ChannelSelector::from_raw(disk.channel_blue),
            ChannelSelector::from_raw(disk.channel_alpha),
        ],
        surface_dimension: SurfaceDimension::from_raw(disk.surface_dimension),
    }
}

fn validate_strictly_increasing(values: &[u64], position: u64, label: &str) -> BinResult<()> {
    if values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(bntx_error(
            position,
            format!("{label} are not strictly increasing"),
        ));
    }
    Ok(())
}

fn validate_strictly_increasing_or_single(
    values: &[u64],
    position: u64,
    label: &str,
) -> BinResult<()> {
    if values.is_empty() {
        return Err(bntx_error(position, format!("{label} are empty")));
    }
    validate_strictly_increasing(values, position, label)
}

fn validate_payload_ranges(textures: &[ParsedTexture]) -> BinResult<()> {
    let mut ranges = textures
        .iter()
        .map(|texture| {
            (
                texture.payload_range.start,
                texture.payload_range.end,
                texture.block_offset,
            )
        })
        .collect::<Vec<_>>();
    ranges.sort_unstable();
    for pair in ranges.windows(2) {
        if pair[1].0 < pair[0].1 {
            return Err(bntx_error(pair[1].2, "BNTX texture image payloads overlap"));
        }
    }
    Ok(())
}
