use std::{fmt, marker::PhantomData, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Visitor};

/// A modem-reported string parsed into a domain type when recognized.
///
/// Modem integrations are not consistent enough to reject an entire RCI
/// response when one identifying value uses a vendor-specific representation.
/// This type preserves such values while keeping recognized ones strongly
/// typed.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum Reported<T> {
    /// The reported value satisfies the domain type's invariants.
    Parsed(T),
    /// The non-empty reported value is not recognized by the domain parser.
    Unrecognized(Box<str>),
}

impl<T> Reported<T> {
    /// Returns the parsed value, or `None` for an unrecognized representation.
    #[must_use]
    pub const fn parsed(&self) -> Option<&T> {
        match self {
            Self::Parsed(value) => Some(value),
            Self::Unrecognized(_) => None,
        }
    }
}

impl<T> Reported<T>
where
    T: AsRef<str>,
{
    /// Returns the exact string representation reported by the modem.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Parsed(value) => value.as_ref(),
            Self::Unrecognized(value) => value,
        }
    }
}

impl<T> From<Box<str>> for Reported<T>
where
    T: FromStr,
{
    fn from(value: Box<str>) -> Self {
        value
            .parse()
            .map_or_else(|_| Self::Unrecognized(value), Self::Parsed)
    }
}

impl<T> Serialize for Reported<T>
where
    T: AsRef<str>,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de, T> Deserialize<'de> for Reported<T>
where
    T: FromStr,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_string(ReportedVisitor(PhantomData))
    }
}

impl<T> fmt::Display for Reported<T>
where
    T: AsRef<str>,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

struct ReportedVisitor<T>(PhantomData<fn() -> T>);

impl<T> Visitor<'_> for ReportedVisitor<T>
where
    T: FromStr,
{
    type Value = Reported<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a modem-reported string")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(value
            .parse()
            .map_or_else(|_| Reported::Unrecognized(value.into()), Reported::Parsed))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(value.parse().map_or_else(
            |_| Reported::Unrecognized(value.into_boxed_str()),
            Reported::Parsed,
        ))
    }
}

struct OptionalReportedVisitor<T>(PhantomData<fn() -> T>);

impl<'de, T> Visitor<'de> for OptionalReportedVisitor<T>
where
    T: FromStr,
{
    type Value = Option<Reported<T>>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("null, an empty string, or a modem-reported string")
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_string(OptionalReportedStringVisitor(PhantomData))
    }
}

struct OptionalReportedStringVisitor<T>(PhantomData<fn() -> T>);

impl<T> Visitor<'_> for OptionalReportedStringVisitor<T>
where
    T: FromStr,
{
    type Value = Option<Reported<T>>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an empty or modem-reported string")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if value.is_empty() {
            Ok(None)
        } else {
            ReportedVisitor(PhantomData).visit_str(value).map(Some)
        }
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if value.is_empty() {
            Ok(None)
        } else {
            ReportedVisitor(PhantomData).visit_string(value).map(Some)
        }
    }
}

pub(super) fn deserialize_optional<'de, D, T>(
    deserializer: D,
) -> Result<Option<Reported<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
{
    deserializer.deserialize_option(OptionalReportedVisitor(PhantomData))
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::Reported;
    use crate::Iccid;

    const STANDARD_ICCID: &str = "8901000000000000001";
    const VENDOR_ICCID: &str = "890100000000000001";

    #[test]
    fn parses_recognized_values_and_preserves_unrecognized_values() {
        let parsed: Reported<Iccid> =
            serde_json::from_str(&format!("\"{STANDARD_ICCID}\"")).unwrap();
        assert!(matches!(parsed, Reported::Parsed(_)));
        assert_eq!(parsed.as_str(), STANDARD_ICCID);

        let unrecognized: Reported<Iccid> =
            serde_json::from_str(&format!("\"{VENDOR_ICCID}\"")).unwrap();
        assert!(matches!(unrecognized, Reported::Unrecognized(_)));
        assert_eq!(unrecognized.as_str(), VENDOR_ICCID);
        assert_eq!(
            serde_json::to_string(&unrecognized).unwrap(),
            format!("\"{VENDOR_ICCID}\"")
        );
    }

    #[test]
    fn optional_wire_value_distinguishes_missing_from_unrecognized() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct Wire {
            #[serde(default, deserialize_with = "super::deserialize_optional")]
            iccid: Option<Reported<Iccid>>,
        }

        assert_eq!(serde_json::from_str::<Wire>("{}").unwrap().iccid, None);
        assert_eq!(
            serde_json::from_str::<Wire>(r#"{"iccid":null}"#)
                .unwrap()
                .iccid,
            None
        );
        assert_eq!(
            serde_json::from_str::<Wire>(r#"{"iccid":""}"#)
                .unwrap()
                .iccid,
            None
        );

        let reported = serde_json::from_str::<Wire>(&format!(r#"{{"iccid":"{VENDOR_ICCID}"}}"#))
            .unwrap()
            .iccid
            .unwrap();
        assert!(matches!(reported, Reported::Unrecognized(_)));
        assert_eq!(reported.as_str(), VENDOR_ICCID);
    }
}
