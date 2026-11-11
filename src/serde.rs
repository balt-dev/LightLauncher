use chrono::FixedOffset;
use serde::{Deserializer, Serializer, de::Visitor};

pub mod url {
    use super::*;

    pub fn serialize<S>(url: &::url::Url, ser: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ser.serialize_str(url.as_str())
    }

    pub fn deserialize<'de, D>(de: D) -> Result<::url::Url, D::Error>
    where
        D: Deserializer<'de>,
    {
        de.deserialize_string(UrlVisitor)
    }

    struct UrlVisitor;

    impl<'de> Visitor<'de> for UrlVisitor {
        type Value = ::url::Url;

        fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            ::url::Url::parse(v).map_err(|e| E::custom(e))
        }

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(formatter, "a valid URL")
        }
    }
}

pub mod time {
    use super::*;

    pub fn serialize<S>(time: &chrono::DateTime<FixedOffset>, ser: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ser.serialize_str(&time.to_rfc3339())
    }

    pub fn deserialize<'de, D>(de: D) -> Result<chrono::DateTime<FixedOffset>, D::Error>
    where
        D: Deserializer<'de>,
    {
        de.deserialize_string(TimeVisitor)
    }

    pub(super) struct TimeVisitor;

    impl<'de> Visitor<'de> for TimeVisitor {
        type Value = chrono::DateTime<FixedOffset>;

        fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            chrono::DateTime::parse_from_rfc3339(v).map_err(|e| E::custom(e))
        }

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(formatter, "a valid RFC3339 timestamp")
        }
    }
}

pub mod time_opt {
    use super::*;

    pub fn serialize<S>(
        time: &Option<chrono::DateTime<FixedOffset>>,
        ser: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match time {
            None => ser.serialize_none(),
            Some(t) => ser.serialize_some(&t.to_rfc3339()),
        }
    }

    pub fn deserialize<'de, D>(de: D) -> Result<Option<chrono::DateTime<FixedOffset>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        de.deserialize_option(OptTimeVisitor)
    }

    struct OptTimeVisitor;

    impl<'de> Visitor<'de> for OptTimeVisitor {
        type Value = Option<chrono::DateTime<FixedOffset>>;

        fn visit_some<D>(self, de: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            de.deserialize_str(super::time::TimeVisitor).map(Some)
        }
        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(formatter, "a valid RFC3339 timestamp, or null")
        }
    }
}
