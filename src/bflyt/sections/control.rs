use binrw::binrw;

/// `cnt1` control-data section, represented as its raw payload.
#[binrw]
#[brw(magic = b"cnt1")]
pub struct Control {
    #[bw(calc = 8 + data.len() as u32)]
    pub size: u32,
    #[br(count = size - 8)]
    pub data: Vec<u8>,
}
