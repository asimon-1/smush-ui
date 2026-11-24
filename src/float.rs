use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, MapAccess, Visitor},
    ser::SerializeMap,
};

pub(crate) struct JsonFloat(pub f32);

pub(crate) fn serialize<S: Serializer>(value: &f32, serializer: S) -> Result<S::Ok, S::Error> {
    JsonFloat(*value).serialize(serializer)
}

pub(crate) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f32, D::Error> {
    JsonFloat::deserialize(deserializer).map(|value| value.0)
}

impl Serialize for JsonFloat {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.0.is_finite() {
            serializer.serialize_f32(self.0)
        } else {
            let mut map = serializer.serialize_map(Some(1))?;
            map.serialize_entry("bits", &self.0.to_bits())?;
            map.end()
        }
    }
}

impl<'de> Deserialize<'de> for JsonFloat {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct JsonFloatVisitor;

        impl<'de> Visitor<'de> for JsonFloatVisitor {
            type Value = JsonFloat;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a JSON number or an object containing float bits")
            }

            fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
                Ok(JsonFloat(value as f32))
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(JsonFloat(value as f32))
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(JsonFloat(value as f32))
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut bits = None;
                while let Some(key) = map.next_key::<String>()? {
                    if key == "bits" {
                        bits = Some(map.next_value()?);
                    } else {
                        return Err(de::Error::unknown_field(&key, &["bits"]));
                    }
                }
                bits.map(|value| JsonFloat(f32::from_bits(value)))
                    .ok_or_else(|| de::Error::missing_field("bits"))
            }
        }

        deserializer.deserialize_any(JsonFloatVisitor)
    }
}
