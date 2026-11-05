use binrw::binrw;

#[binrw]
#[brw(magic = b"pas1")]
/// Empty marker that begins the children of a pane.
pub struct PaneStart {
    #[br(assert(size == 8))]
    #[bw(calc = 8)]
    pub size: u32,
}
