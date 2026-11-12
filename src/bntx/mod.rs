mod binary;
mod dictionary;
pub mod format;
mod image;
mod input;
mod model;
mod parser;
mod relocation;
mod swizzle;
mod validation;
mod writer;

use binrw::{BinResult, Error as BinError};

pub use self::format::{
    ChannelFormat, ChannelSelector, ImageFormat, Platform, SurfaceDimension, TextureDimension,
    TextureLayout, TileMode, TypeFormat,
};
pub use self::image::{
    ChannelMode, EncodedImage, RgbaImage, decode_texture, encode_new_texture, encode_texture,
    read_png, write_png,
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
