use binrw::binrw;

#[binrw]
#[brw(magic = b"grp1")]
pub struct Group {
    #[bw(calc = 0x24 + 24 * children.len() as u32)]
    pub size: u32,
    /// NUL-terminated group name in a fixed 24-byte buffer.
    pub name: [u8; 24],
    #[bw(calc = children.len() as u16)]
    pub child_count: u16,
    pub padding: [u8; 2],
    #[br(count = child_count)]
    /// NUL-terminated child names, each in a fixed 24-byte buffer.
    pub children: Vec<[u8; 24]>,
}
