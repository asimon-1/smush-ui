use binrw::binrw;
use serde::{Deserialize, Serialize};

use super::{MaterialInfo, PaneInfo};

#[derive(Serialize, Deserialize)]
#[binrw]
#[brw(magic = b"pic1")]
pub struct Picture {
    #[serde(skip)]
    #[bw(map = |_: &u32| 0x68 + 32 * material.texture_coordinates.len() as u32)]
    size: u32,
    pub pane: PaneInfo,
    pub material: MaterialInfo,
}

impl Picture { pub fn new(pane: PaneInfo, material: MaterialInfo) -> Self { Self { size: 0, pane, material } } }
