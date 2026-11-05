use binrw::binrw;

#[binrw]
#[brw(magic = b"pae1")]
/// Empty marker that ends the children of a pane.
pub struct PaneEnd {
    #[br(assert(size == 8))]
    #[bw(calc = 8)]
    pub size: u32,
}
