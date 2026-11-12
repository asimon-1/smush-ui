use super::{
    bntx_error,
    format::{ChannelFormat, ImageFormat, TypeFormat},
};
use binrw::BinResult;

const GOB_WIDTH_BYTES: u128 = 64;
const GOB_HEIGHT: u128 = 8;
const GOB_SIZE: u128 = 512;
const MAX_NEW_BLOCK_HEIGHT: u32 = 16;

/// The block geometry and Tegra X1 block-linear layout of one mip surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurfaceLayout {
    width: u32,
    height: u32,
    bytes_per_block: u8,
    block_height_exponent: u8,
}

impl SurfaceLayout {
    /// Creates a layout for a supported BNTX image format.
    pub fn new(
        format: ImageFormat,
        width: u32,
        height: u32,
        block_height_exponent: u8,
    ) -> BinResult<Self> {
        if width == 0 || height == 0 {
            return Err(bntx_error(0, "BNTX surface dimensions must be nonzero"));
        }
        if block_height_exponent > 5 {
            return Err(bntx_error(
                0,
                "BNTX block-height exponent exceeds the Tegra X1 limit",
            ));
        }
        let bytes_per_block = bytes_per_block(format)?;
        Ok(Self {
            width,
            height,
            bytes_per_block,
            block_height_exponent,
        })
    }

    /// Creates a mip-zero layout using the canonical block height for a new texture.
    pub fn for_new_texture(format: ImageFormat, width: u32, height: u32) -> BinResult<Self> {
        let block_height_exponent = derive_block_height_exponent(format, width, height)?;
        Self::new(format, width, height, block_height_exponent)
    }

    /// Creates the layout for a mip, reducing the base block height as the mip shrinks.
    pub fn mip(self, level: u32) -> Self {
        let width = self.width.checked_shr(level).unwrap_or(0).max(1);
        let height = self.height.checked_shr(level).unwrap_or(0).max(1);
        let height_blocks = height.div_ceil(4);
        let mut block_height = 1u32 << self.block_height_exponent;
        while block_height > 1 && height_blocks.div_ceil(8) <= block_height / 2 {
            block_height /= 2;
        }
        Self {
            width,
            height,
            bytes_per_block: self.bytes_per_block,
            block_height_exponent: block_height.trailing_zeros() as u8,
        }
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub const fn bytes_per_block(&self) -> u8 {
        self.bytes_per_block
    }

    pub const fn block_height_exponent(&self) -> u8 {
        self.block_height_exponent
    }

    pub const fn width_blocks(&self) -> u32 {
        self.width.div_ceil(4)
    }

    pub const fn height_blocks(&self) -> u32 {
        self.height.div_ceil(4)
    }

    /// Size of the tightly packed row-major compressed blocks.
    pub fn linear_size(&self) -> BinResult<usize> {
        usize_value(
            u128::from(self.width_blocks())
                * u128::from(self.height_blocks())
                * u128::from(self.bytes_per_block),
            "linear surface size",
        )
    }

    /// Size of the padded Tegra block-linear surface.
    pub fn swizzled_size(&self) -> BinResult<usize> {
        let width_bytes = u128::from(self.width_blocks()) * u128::from(self.bytes_per_block);
        let block_height = 1u128 << self.block_height_exponent;
        let width_bytes = width_bytes.div_ceil(GOB_WIDTH_BYTES) * GOB_WIDTH_BYTES;
        let height_blocks = u128::from(self.height_blocks());
        let height_blocks =
            height_blocks.div_ceil(GOB_HEIGHT * block_height) * GOB_HEIGHT * block_height;
        usize_value(width_bytes * height_blocks, "block-linear surface size")
    }

    /// Converts tightly packed compressed blocks to Tegra block-linear order.
    pub fn swizzle(&self, linear: &[u8]) -> BinResult<Vec<u8>> {
        let linear_size = self.linear_size()?;
        if linear.len() != linear_size {
            return Err(bntx_error(
                0,
                format!(
                    "BNTX linear surface has {} bytes, expected {linear_size}",
                    linear.len()
                ),
            ));
        }
        let mut swizzled = zeroed(self.swizzled_size()?, "block-linear surface")?;
        self.copy_blocks(linear, &mut swizzled, true)?;
        Ok(swizzled)
    }

    /// Converts a Tegra block-linear surface to tightly packed compressed blocks.
    pub fn deswizzle(&self, swizzled: &[u8]) -> BinResult<Vec<u8>> {
        let swizzled_size = self.swizzled_size()?;
        if swizzled.len() != swizzled_size {
            return Err(bntx_error(
                0,
                format!(
                    "BNTX block-linear surface has {} bytes, expected {swizzled_size}",
                    swizzled.len()
                ),
            ));
        }
        let mut linear = zeroed(self.linear_size()?, "linear surface")?;
        self.copy_blocks(swizzled, &mut linear, false)?;
        Ok(linear)
    }

    fn copy_blocks(&self, source: &[u8], destination: &mut [u8], swizzle: bool) -> BinResult<()> {
        let bytes_per_block = usize::from(self.bytes_per_block);
        for y in 0..self.height_blocks() {
            for x in 0..self.width_blocks() {
                let linear =
                    (y as usize * self.width_blocks() as usize + x as usize) * bytes_per_block;
                let block_linear = self.block_linear_address(x, y)?;
                let (source_offset, destination_offset) = if swizzle {
                    (linear, block_linear)
                } else {
                    (block_linear, linear)
                };
                let source_end = source_offset
                    .checked_add(bytes_per_block)
                    .ok_or_else(|| bntx_error(0, "BNTX source block offset overflows"))?;
                let destination_end = destination_offset
                    .checked_add(bytes_per_block)
                    .ok_or_else(|| bntx_error(0, "BNTX destination block offset overflows"))?;
                let source = source
                    .get(source_offset..source_end)
                    .ok_or_else(|| bntx_error(0, "BNTX source block is out of bounds"))?;
                let destination = destination
                    .get_mut(destination_offset..destination_end)
                    .ok_or_else(|| bntx_error(0, "BNTX destination block is out of bounds"))?;
                destination.copy_from_slice(source);
            }
        }
        Ok(())
    }

    fn block_linear_address(&self, x: u32, y: u32) -> BinResult<usize> {
        let bytes_per_block = u128::from(self.bytes_per_block);
        let x_bytes = u128::from(x) * bytes_per_block;
        let y = u128::from(y);
        let block_height = 1u128 << self.block_height_exponent;
        let width_in_gobs =
            (u128::from(self.width_blocks()) * bytes_per_block).div_ceil(GOB_WIDTH_BYTES);
        let gob = y / (GOB_HEIGHT * block_height) * GOB_SIZE * block_height * width_in_gobs
            + x_bytes / GOB_WIDTH_BYTES * GOB_SIZE * block_height
            + y % (GOB_HEIGHT * block_height) / GOB_HEIGHT * GOB_SIZE;
        let address = gob
            + (x_bytes % 64) / 32 * 256
            + (y % 8) / 2 * 64
            + (x_bytes % 32) / 16 * 32
            + y % 2 * 16
            + x_bytes % 16;
        usize_value(address, "block-linear address")
    }
}

/// Chooses the smallest power-of-two GOB height that covers mip zero, capped at 16 GOBs.
pub fn derive_block_height_exponent(format: ImageFormat, width: u32, height: u32) -> BinResult<u8> {
    let layout = SurfaceLayout::new(format, width, height, 0)?;
    let required_gobs = layout.height_blocks().div_ceil(8);
    let block_height = required_gobs
        .checked_next_power_of_two()
        .unwrap_or(MAX_NEW_BLOCK_HEIGHT)
        .clamp(1, MAX_NEW_BLOCK_HEIGHT);
    Ok(block_height.trailing_zeros() as u8)
}

fn bytes_per_block(format: ImageFormat) -> BinResult<u8> {
    match (format.channel_format(), format.type_format()) {
        (ChannelFormat::Bc4, TypeFormat::Unorm) => Ok(8),
        (ChannelFormat::Bc5 | ChannelFormat::Bc7U, TypeFormat::Unorm)
        | (ChannelFormat::Bc7U, TypeFormat::Srgb) => Ok(16),
        _ => Err(bntx_error(
            0,
            format!("unsupported BNTX image format {:#010x}", format.to_raw()),
        )),
    }
}

fn usize_value(value: u128, label: &str) -> BinResult<usize> {
    usize::try_from(value).map_err(|_| bntx_error(0, format!("BNTX {label} overflows")))
}

fn zeroed(size: usize, label: &str) -> BinResult<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size)
        .map_err(|_| bntx_error(0, format!("BNTX {label} is too large")))?;
    bytes.resize(size, 0);
    Ok(bytes)
}
