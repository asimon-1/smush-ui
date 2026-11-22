mod binary;
mod creation;
mod dictionary;
pub mod format;
pub mod fs;
mod image;
mod input;
mod inspect;
mod model;
mod parser;
mod relocation;
mod swizzle;
mod validation;
mod writer;

use binrw::{BinResult, Error as BinError};

pub use self::creation::{
    AddTextureOptions, NewTextureFormat, TextureProfile, add_texture, create_bntx,
};
pub use self::format::{
    ChannelFormat, ChannelSelector, ImageFormat, Platform, SurfaceDimension, TextureDimension,
    TextureLayout, TileMode, TypeFormat,
};
pub use self::image::{
    ChannelMode, EncodedImage, RgbaImage, decode_texture, encode_new_texture, encode_texture,
    read_png, supports_image_format, write_png,
};
pub use self::inspect::{
    BntxDiff, BntxInspection, BntxTextureChange, BntxTextureRecord, diff_bntx, inspect_bntx,
};
pub use self::model::{Bntx, FloatValue, Texture, TextureInfo, UserData, UserDataValue, Version};
pub use self::swizzle::{SurfaceLayout, derive_block_height_exponent};

impl Bntx {
    pub fn from_bytes(bytes: &[u8]) -> BinResult<Self> {
        parser::parse(bytes)
    }

    pub fn to_bytes(&self) -> BinResult<Vec<u8>> {
        writer::write(self)
    }
}

pub(crate) fn bntx_error(position: u64, message: impl Into<String>) -> BinError {
    BinError::AssertFail {
        pos: position,
        message: message.into(),
    }
}

pub(crate) fn zeroed_bytes(size: usize, label: &str) -> BinResult<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size)
        .map_err(|_| bntx_error(0, format!("BNTX {label} is too large")))?;
    bytes.resize(size, 0);
    Ok(bytes)
}
