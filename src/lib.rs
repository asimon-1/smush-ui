pub mod bffnt;
pub mod bflan;
pub mod bflim;
pub mod bflyt;
pub mod binary;

pub use bffnt::Bffnt;
pub use bflan::Bflan;
pub use bflim::Bflim;
pub use bflyt::Bflyt;

use serde::{Serialize, de::DeserializeOwned};

pub fn from_yaml_str<T: DeserializeOwned>(input: &str) -> Result<T, yaml_serde::Error> {
    yaml_serde::from_str(input)
}

pub fn to_yaml_string<T: Serialize>(value: &T) -> Result<String, yaml_serde::Error> {
    yaml_serde::to_string(value)
}
