use binrw::binrw;

#[binrw]
#[brw(magic = b"grs1")]
/// Empty marker which follows a `grp1` section.
pub struct GroupStart {
    #[br(assert(size == 8))]
    #[bw(calc = 8)]
    pub size: u32,
}
