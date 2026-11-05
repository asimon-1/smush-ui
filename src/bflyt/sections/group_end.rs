use binrw::binrw;

#[binrw]
#[brw(magic = b"gre1")]
/// Empty marker which closes a group.
pub struct GroupEnd {
    #[br(assert(size == 8))]
    #[bw(calc = 8)]
    pub size: u32,
}
