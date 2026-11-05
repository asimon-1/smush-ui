use binrw::binrw;

/// `usd1` user-data section, represented as its raw payload.
#[binrw]
#[brw(magic = b"usd1")]
pub struct User {
    #[bw(calc = 8 + data.len() as u32)]
    pub size: u32,
    #[br(count = size - 8)]
    pub data: Vec<u8>,
}
