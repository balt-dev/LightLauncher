use serde::{Deserializer, Serializer, de::Visitor};
use chrono::FixedOffset;

pub mod url {
    use super::*;
    
    pub fn serialize<S>(url: &reqwest::Url, ser: S) -> Result<S::Ok, S::Error> where S: Serializer {
        ser.serialize_str(url.as_str())
    }
    
    pub fn deserialize<'de, D>(de: D) -> Result<reqwest::Url, D::Error> where D: Deserializer<'de> {
        de.deserialize_string(UrlVisitor)
    }
    
    struct UrlVisitor;
    
    impl<'de> Visitor<'de> for UrlVisitor {
        type Value = reqwest::Url;
        
        fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E> where E: serde::de::Error {
            reqwest::Url::parse(v).map_err(|e| E::custom(e))
        }
    
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(formatter, "a valid URL")
        }
    }

}

pub mod time {
    use super::*;
    
    pub fn serialize<S>(time: &chrono::DateTime<FixedOffset>, ser: S) -> Result<S::Ok, S::Error> where S: Serializer {
        ser.serialize_str(&time.to_rfc3339())
    }
    
    pub fn deserialize<'de, D>(de: D) -> Result<chrono::DateTime<FixedOffset>, D::Error> where D: Deserializer<'de> {
        de.deserialize_string(TimeVisitor)
    }
    
    struct TimeVisitor;
    
    impl<'de> Visitor<'de> for TimeVisitor {
        type Value = chrono::DateTime<FixedOffset>;
        
        fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E> where E: serde::de::Error {
            chrono::DateTime::parse_from_rfc3339(v).map_err(|e| E::custom(e))
        }
    
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(formatter, "a valid RFC3339 timestamp")
        }
    }

}