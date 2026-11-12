mod binary;
pub mod format;
mod input;
mod model;
mod parser;

use binrw::{BinResult, Error as BinError};

pub use self::format::{
    ChannelFormat, ChannelSelector, ImageFormat, Platform, SurfaceDimension, TextureDimension,
    TextureLayout, TileMode, TypeFormat,
};
pub use self::model::{Bntx, FloatValue, Texture, TextureInfo, UserData, UserDataValue, Version};

impl Bntx {
    /// Parses the verified little-endian version-4 `NX  ` BNTX profile.
    pub fn from_bytes(bytes: &[u8]) -> BinResult<Self> {
        parser::parse(bytes)
    }
}

pub(crate) fn bntx_error(position: u64, message: impl Into<String>) -> BinError {
    BinError::AssertFail {
        pos: position,
        message: message.into(),
    }
}
