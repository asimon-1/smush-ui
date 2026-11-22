use binrw::binrw;

pub(crate) const FILE_HEADER_SIZE: u64 = 0x20;
pub(crate) const NX_HEADER_SIZE: u64 = 0x38;
pub(crate) const BLOCK_HEADER_SIZE: u64 = 0x10;
pub(crate) const BRTI_SIZE: u64 = 0xa0;
pub(crate) const USER_DATA_SIZE: u64 = 0x18;
pub(crate) const DICTIONARY_NODE_SIZE: u64 = 0x10;
pub(crate) const RELOCATION_SECTION_SIZE: u64 = 0x18;
pub(crate) const RELOCATION_ENTRY_SIZE: u64 = 0x08;

#[binrw]
#[brw(little, magic = b"BNTX\0\0\0\0")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct FileHeader {
    #[br(temp, assert(micro_version == 0, "unsupported BNTX micro version"))]
    #[bw(calc = 0u8)]
    micro_version: u8,
    #[br(temp, assert(minor_version == 0, "unsupported BNTX minor version"))]
    #[bw(calc = 0u8)]
    minor_version: u8,
    #[br(temp, assert(major_version == 4, "unsupported BNTX major version"))]
    #[bw(calc = 4u16)]
    major_version: u16,
    #[br(temp, assert(byte_order_mark == 0xfeff, "BNTX must be little-endian"))]
    #[bw(calc = 0xfeffu16)]
    byte_order_mark: u16,
    #[br(temp, assert(alignment_exponent == 12, "unsupported BNTX alignment"))]
    #[bw(calc = 12u8)]
    alignment_exponent: u8,
    #[br(temp, assert(target_address_size == 64, "unsupported BNTX address size"))]
    #[bw(calc = 64u8)]
    target_address_size: u8,
    pub name_offset: u32,
    #[br(temp, assert(flag == 0, "relocated BNTX files are not supported"))]
    #[bw(calc = 0u16)]
    flag: u16,
    pub first_block_offset: u16,
    pub relocation_offset: u32,
    pub file_size: u32,
}

#[binrw]
#[brw(little, magic = b"NX  ")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct NxHeader {
    pub texture_count: u32,
    pub texture_info_array_offset: u64,
    pub texture_data_block_offset: u64,
    pub texture_dictionary_offset: u64,
    #[br(temp, assert(memory_pool_offset == FILE_HEADER_SIZE + NX_HEADER_SIZE, "unsupported BNTX memory-pool offset"))]
    #[bw(calc = FILE_HEADER_SIZE + NX_HEADER_SIZE)]
    memory_pool_offset: u64,
    #[br(temp, assert(current_memory_pool_pointer == 0, "unsupported BNTX current memory-pool pointer"))]
    #[bw(calc = 0u64)]
    current_memory_pool_pointer: u64,
    #[br(temp, assert(base_memory_pool_offset == 0, "unsupported BNTX base memory-pool offset"))]
    #[bw(calc = 0u32)]
    base_memory_pool_offset: u32,
    #[br(temp, assert(unknown == 0, "unsupported BNTX NX reserved value"))]
    #[bw(calc = 0u32)]
    unknown: u32,
}

#[binrw]
#[brw(little, magic = b"_STR")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct StringPoolHeader {
    #[br(temp)]
    #[bw(calc = *block_size)]
    next_block_offset: u32,
    #[br(assert(block_size != 0 && block_size == next_block_offset, "BNTX string-pool block size and next offset disagree"))]
    pub block_size: u32,
    #[br(temp, assert(padding == 0, "invalid BNTX string-pool padding"))]
    #[bw(calc = 0u32)]
    padding: u32,
    pub string_count: u32,
    #[br(temp, assert(empty_string == 0, "invalid BNTX empty-string record"))]
    #[bw(calc = 0u32)]
    empty_string: u32,
}

#[binrw]
#[brw(little, magic = b"_DIC")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct DictionaryHeader {
    pub entry_count: u32,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct DictionaryNodeDisk {
    pub reference: u32,
    pub left: u16,
    pub right: u16,
    pub key_offset: u64,
}

#[binrw]
#[brw(little, magic = b"BRTI")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct TextureInfoDisk {
    #[br(temp)]
    #[bw(calc = *block_size)]
    next_block_offset: u32,
    #[br(assert(block_size != 0 && block_size == next_block_offset, "BNTX BRTI block size and next offset disagree"))]
    pub block_size: u32,
    #[br(temp, assert(block_padding == 0, "invalid BNTX BRTI padding"))]
    #[bw(calc = 0u32)]
    block_padding: u32,
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
    #[brw(pad_after = 20)]
    pub texture_layout2: u32,
    pub image_size: u32,
    pub image_alignment: u32,
    pub channel_red: u8,
    pub channel_green: u8,
    pub channel_blue: u8,
    pub channel_alpha: u8,
    pub surface_dimension: u32,
    pub name_offset: u64,
    #[br(temp, assert(container_offset == FILE_HEADER_SIZE, "BNTX texture container pointer does not point to the NX header"))]
    #[bw(calc = FILE_HEADER_SIZE)]
    container_offset: u64,
    pub mip_offsets_offset: u64,
    pub user_data_offset: u64,
    pub texture_pointer: u64,
    pub texture_view_pointer: u64,
    pub descriptor_slot_offset: u64,
    pub user_data_dictionary_offset: u64,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct UserDataDisk {
    pub name_offset: u64,
    pub value_offset: u64,
    pub count: u32,
    pub value_type: u8,
    #[br(temp, assert(padding == [0; 3], "invalid BNTX user-data padding"))]
    #[bw(calc = [0u8; 3])]
    padding: [u8; 3],
}

#[binrw]
#[brw(little, magic = b"BRTD\0\0\0\0")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct TextureDataHeader {
    pub block_size: u64,
}

#[binrw]
#[brw(little, magic = b"_RLT")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct RelocationHeader {
    pub table_offset: u32,
    #[br(temp, assert(section_count == 2, "unsupported BNTX relocation section count"))]
    #[bw(calc = 2u32)]
    section_count: u32,
    #[br(temp, assert(padding == 0, "invalid BNTX relocation-table padding"))]
    #[bw(calc = 0u32)]
    padding: u32,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct RelocationSectionDisk {
    #[br(temp, assert(memory_address == 0, "unsupported BNTX relocation memory address"))]
    #[bw(calc = 0u64)]
    memory_address: u64,
    pub file_offset: u32,
    pub file_size: u32,
    pub first_entry_index: u32,
    pub entry_count: u32,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct RelocationEntryDisk {
    pub offset: u32,
    pub array_count: u16,
    pub pointer_count: u8,
    pub padding_count: u8,
}
