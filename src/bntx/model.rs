use super::format::{
    ChannelSelector, ImageFormat, Platform, SurfaceDimension, TextureDimension, TextureLayout,
    TileMode,
};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, MapAccess, Visitor},
    ser::SerializeMap,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Version {
    pub major: u16,
    pub minor: u8,
    pub micro: u8,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Bntx {
    pub version: Version,
    pub alignment_exponent: u8,
    pub target_address_size: u8,
    pub flag: u16,
    pub name: String,
    pub platform: Platform,
    pub textures: Vec<Texture>,
    #[serde(skip)]
    pub(crate) source_string_order: Vec<String>,
}

impl PartialEq for Bntx {
    fn eq(&self, other: &Self) -> bool {
        self.version == other.version
            && self.alignment_exponent == other.alignment_exponent
            && self.target_address_size == other.target_address_size
            && self.flag == other.flag
            && self.name == other.name
            && self.platform == other.platform
            && self.textures == other.textures
    }
}

/// One texture and its exact raw swizzled image payload.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Texture {
    pub name: String,
    pub info: TextureInfo,
    pub mip_offsets: Vec<u64>,
    pub user_data: Vec<UserData>,
    pub image_data: Vec<u8>,
}

/// Semantic BRTI fields which are not file offsets or derived block bookkeeping.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TextureInfo {
    pub flags: u8,
    pub image_dimension: TextureDimension,
    pub tile_mode: TileMode,
    pub swizzle: u16,
    pub sample_count: u32,
    pub format: ImageFormat,
    pub gpu_access: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub array_count: u32,
    pub texture_layout: TextureLayout,
    pub texture_layout2: u32,
    pub reserved: [u8; 20],
    pub image_alignment: u32,
    pub channels: [ChannelSelector; 4],
    pub surface_dimension: SurfaceDimension,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct UserData {
    pub name: String,
    pub value: UserDataValue,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum UserDataValue {
    Int32(Vec<i32>),
    Single(Vec<FloatValue>),
    String(Vec<String>),
    Byte(Vec<u8>),
    WString(Vec<String>),
}

/// An f32 value which retains its exact bits in JSON for non-finite values.
#[derive(Clone, Copy, PartialEq)]
pub struct FloatValue(u32);

impl FloatValue {
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    pub const fn to_bits(self) -> u32 {
        self.0
    }

    pub const fn value(self) -> f32 {
        f32::from_bits(self.0)
    }
}

impl std::fmt::Debug for FloatValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value().fmt(formatter)
    }
}

impl Serialize for FloatValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let value = self.value();
        if value.is_finite() {
            serializer.serialize_f32(value)
        } else {
            let mut map = serializer.serialize_map(Some(1))?;
            map.serialize_entry("bits", &self.0)?;
            map.end()
        }
    }
}

impl<'de> Deserialize<'de> for FloatValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct FloatValueVisitor;

        impl<'de> Visitor<'de> for FloatValueVisitor {
            type Value = FloatValue;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a number or an object containing exact f32 bits")
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(FloatValue((value as f32).to_bits()))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(FloatValue((value as f32).to_bits()))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(FloatValue((value as f32).to_bits()))
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut bits = None;
                while let Some(key) = map.next_key::<String>()? {
                    if key == "bits" {
                        bits = Some(map.next_value()?);
                    } else {
                        return Err(de::Error::unknown_field(&key, &["bits"]));
                    }
                }
                bits.map(FloatValue)
                    .ok_or_else(|| de::Error::missing_field("bits"))
            }
        }

        deserializer.deserialize_any(FloatValueVisitor)
    }
}

impl Bntx {
    /// Required file alignment in bytes.
    pub fn alignment(&self) -> Option<usize> {
        1usize.checked_shl(u32::from(self.alignment_exponent))
    }
}
