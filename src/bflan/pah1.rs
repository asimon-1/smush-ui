use binrw::{BinResult, binrw};
use serde::{Deserialize, Serialize};

use crate::bflyt::sections::common::{fixed_string_to_string, string_to_fixed_string};

use super::{checked_u32, invalid};

#[binrw]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SharedAnimation {
    #[br(try_map = fixed_string_to_string::<25>)]
    #[bw(try_map = string_to_fixed_string::<25>)]
    pub source_pane: String,
    #[br(try_map = fixed_string_to_string::<25>)]
    #[bw(try_map = string_to_fixed_string::<25>)]
    #[brw(pad_after = 2)]
    pub target_group: String,
}

#[binrw]
#[brw(little, magic = b"pah1")]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pah1 {
    #[br(temp)]
    #[bw(calc = pah1_size(shares.len())?)]
    size: u32,
    #[brw(magic = 16u32)]
    #[br(temp, assert(size == pah1_size(share_count as usize)?, "PAH1 size does not match share count"))]
    #[bw(calc = u16::try_from(shares.len()).map_err(|_| invalid(12, "too many PAH1 shares"))?)]
    #[brw(pad_after = 2)]
    share_count: u16,
    #[br(count = share_count)]
    pub shares: Vec<SharedAnimation>,
}

fn pah1_size(count: usize) -> BinResult<u32> {
    let bytes = count
        .checked_mul(52)
        .and_then(|length| length.checked_add(16))
        .ok_or_else(|| invalid(4, "PAH1 size overflow"))?;
    checked_u32(bytes, 4)
}
