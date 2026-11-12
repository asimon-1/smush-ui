use binrw::binread;

pub(crate) const FILE_HEADER_SIZE: u64 = 0x20;
pub(crate) const NX_HEADER_SIZE: u64 = 0x38;
pub(crate) const BLOCK_HEADER_SIZE: u64 = 0x10;
pub(crate) const BRTI_SIZE: u64 = 0xa0;
pub(crate) const USER_DATA_SIZE: u64 = 0x18;

#[binread]
#[br(little, magic = b"BNTX\0\0\0\0")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct FileHeader {
    pub micro_version: u8,
    pub minor_version: u8,
    pub major_version: u16,
    #[br(assert(_byte_order_mark == 0xfeff, "BNTX must be little-endian"))]
    pub _byte_order_mark: u16,
    pub alignment_exponent: u8,
    pub target_address_size: u8,
    pub name_offset: u32,
    pub flag: u16,
    pub first_block_offset: u16,
    pub relocation_offset: u32,
    pub file_size: u32,
}

#[binread]
#[br(little, magic = b"NX  ")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct NxHeader {
    pub texture_count: u32,
    pub texture_info_array_offset: u64,
    pub texture_data_block_offset: u64,
    pub texture_dictionary_offset: u64,
    pub memory_pool_offset: u64,
    pub current_memory_pool_pointer: u64,
    pub base_memory_pool_offset: u32,
    pub unknown: u32,
}

#[binread]
#[br(little, magic = b"_STR")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct StringPoolHeader {
    pub next_block_offset: u32,
    pub block_size: u32,
    #[br(assert(_padding == 0, "invalid BNTX string-pool padding"))]
    pub _padding: u32,
    pub string_count: u32,
    #[br(assert(_empty_string == 0, "invalid BNTX empty-string record"))]
    pub _empty_string: u32,
}

#[binread]
#[br(little, magic = b"_DIC")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct DictionaryHeader {
    pub entry_count: u32,
}

#[binread]
#[br(little, magic = b"BRTI")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct TextureInfoDisk {
    pub next_block_offset: u32,
    pub block_size: u32,
    #[br(assert(_block_padding == 0, "invalid BNTX BRTI padding"))]
    pub _block_padding: u32,
    pub flags: u8,
    pub image_dimension: u8,
    pub tile_mode: u16,
    pub swizzle: u16,
    pub mip_count: u16,
    pub sample_count: u32,
    pub format: u32,
    pub gpu_access: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub array_count: u32,
    pub texture_layout: u32,
    pub texture_layout2: u32,
    pub reserved: [u8; 20],
    pub image_size: u32,
    pub image_alignment: u32,
    pub channel_red: u8,
    pub channel_green: u8,
    pub channel_blue: u8,
    pub channel_alpha: u8,
    pub surface_dimension: u32,
    pub name_offset: u64,
    pub container_offset: u64,
    pub mip_offsets_offset: u64,
    pub user_data_offset: u64,
    pub texture_pointer: u64,
    pub texture_view_pointer: u64,
    pub descriptor_slot_offset: u64,
    pub user_data_dictionary_offset: u64,
}

#[binread]
#[br(little)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct UserDataDisk {
    pub name_offset: u64,
    pub value_offset: u64,
    pub count: u32,
    pub value_type: u8,
    #[br(assert(_padding == [0; 3], "invalid BNTX user-data padding"))]
    pub _padding: [u8; 3],
}

#[binread]
#[br(little, magic = b"BRTD\0\0\0\0")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct TextureDataHeader {
    pub block_size: u64,
}
