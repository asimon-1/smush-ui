use binrw::binrw;

#[binrw]
#[brw(magic = b"mat1")]
pub struct Material {
    #[bw(calc = 8 + data.len() as u32)]
    pub size: u32,
    #[br(count = size - 8)]
    pub data: Vec<u8>,
}
