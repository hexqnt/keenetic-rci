use std::{fmt, str::FromStr};

use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, Visitor},
};
use thiserror::Error;

use super::text::{InlineAscii, InvalidInlineAscii};

/// The number of decimal digits in a hardware model code.
pub const HARDWARE_MODEL_LENGTH: usize = 4;

/// A hardware model code failed structural validation.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum ParseHardwareModelError {
    /// The value does not contain exactly four bytes.
    #[error("a hardware model must contain exactly {HARDWARE_MODEL_LENGTH} digits, got {actual}")]
    InvalidLength {
        /// Actual byte length of the supplied value.
        actual: usize,
    },
    /// The value contains a byte outside the ASCII decimal digit range.
    #[error("a hardware model contains a non-decimal digit at byte index {index}")]
    InvalidDigit {
        /// Zero-based byte index of the invalid digit.
        index: usize,
    },
}

/// A hardware identifier failed structural validation.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum ParseHardwareIdError {
    /// The value does not contain exactly seven bytes.
    #[error("a hardware identifier must contain exactly 7 bytes, got {actual}")]
    InvalidLength {
        /// Actual byte length of the supplied value.
        actual: usize,
    },
    /// The vendor and model are not separated by a hyphen.
    #[error("a hardware identifier must contain a hyphen at byte index 2")]
    InvalidSeparator,
    /// The vendor prefix is not recognized.
    #[error(transparent)]
    InvalidVendor(#[from] ParseHardwareVendorError),
    /// The model code is invalid.
    #[error(transparent)]
    InvalidModel(#[from] ParseHardwareModelError),
}

/// Vendor encoded in a hardware identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum HardwareVendor {
    /// Keenetic, encoded as `KN`.
    Keenetic,
    /// Netcraze, encoded as `NC`.
    Netcraze,
}

impl HardwareVendor {
    /// Returns the two-letter vendor code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Keenetic => "KN",
            Self::Netcraze => "NC",
        }
    }
}

impl FromStr for HardwareVendor {
    type Err = ParseHardwareVendorError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "KN" => Ok(Self::Keenetic),
            "NC" => Ok(Self::Netcraze),
            _ => Err(ParseHardwareVendorError),
        }
    }
}

impl Serialize for HardwareVendor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for HardwareVendor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(HardwareVendorVisitor)
    }
}

impl fmt::Display for HardwareVendor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A hardware vendor prefix is not recognized.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("a hardware vendor must be KN or NC")]
pub struct ParseHardwareVendorError;

struct HardwareVendorVisitor;

impl Visitor<'_> for HardwareVendorVisitor {
    type Value = HardwareVendor;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a hardware vendor code KN or NC")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        value.parse().map_err(E::custom)
    }
}

/// A four-digit hardware model code.
///
/// The digits are stored inline so leading zeroes are preserved without a
/// heap allocation.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct HardwareModel(InlineAscii<HARDWARE_MODEL_LENGTH>);

impl HardwareModel {
    /// Parses a four-digit hardware model code.
    ///
    /// # Errors
    ///
    /// Returns [`ParseHardwareModelError`] unless the value contains exactly
    /// four ASCII decimal digits.
    pub fn parse(value: &str) -> Result<Self, ParseHardwareModelError> {
        value.parse()
    }

    /// Returns the model code as a string slice.
    ///
    /// # Panics
    ///
    /// This can only panic if the type's private ASCII-only representation is
    /// corrupted. Values created through the public API always uphold it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Returns the model code as its fixed-size ASCII representation.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; HARDWARE_MODEL_LENGTH] {
        self.0.as_array()
    }
}

impl FromStr for HardwareModel {
    type Err = ParseHardwareModelError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        InlineAscii::digits(value, HARDWARE_MODEL_LENGTH)
            .map(Self)
            .map_err(|error| match error {
                InvalidInlineAscii::Length(actual) => {
                    ParseHardwareModelError::InvalidLength { actual }
                }
                InvalidInlineAscii::Character(index) => {
                    ParseHardwareModelError::InvalidDigit { index }
                }
            })
    }
}

impl_string_value!(
    HardwareModel,
    ParseHardwareModelError,
    "a four-digit hardware model"
);

/// A Keenetic or Netcraze hardware identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct HardwareId {
    /// Hardware vendor encoded by the identifier prefix.
    pub vendor: HardwareVendor,
    /// Four-digit model code.
    pub model: HardwareModel,
}

impl HardwareId {
    /// Parses a hardware identifier in the `(KN|NC)-[0-9]{4}` format.
    ///
    /// # Errors
    ///
    /// Returns [`ParseHardwareIdError`] when the identifier has an invalid
    /// length, separator, vendor, or model code.
    pub fn parse(value: &str) -> Result<Self, ParseHardwareIdError> {
        value.parse()
    }
}

impl FromStr for HardwareId {
    type Err = ParseHardwareIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let bytes = value.as_bytes();
        if bytes.len() != 7 {
            return Err(ParseHardwareIdError::InvalidLength {
                actual: bytes.len(),
            });
        }
        if bytes[2] != b'-' {
            return Err(ParseHardwareIdError::InvalidSeparator);
        }

        Ok(Self {
            vendor: value[..2].parse()?,
            model: value[3..].parse()?,
        })
    }
}

impl TryFrom<&str> for HardwareId {
    type Error = ParseHardwareIdError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl Serialize for HardwareId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for HardwareId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(HardwareIdVisitor)
    }
}

impl fmt::Display for HardwareId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}-{}", self.vendor, self.model)
    }
}

struct HardwareIdVisitor;

impl Visitor<'_> for HardwareIdVisitor {
    type Value = HardwareId;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a hardware identifier matching (KN|NC)-[0-9]{4}")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        value.parse().map_err(E::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::{HardwareId, HardwareVendor, ParseHardwareIdError};

    #[test]
    fn parses_both_vendors_and_preserves_leading_zeroes() {
        let keenetic = HardwareId::parse("KN-3811").unwrap();
        let netcraze = HardwareId::parse("NC-0012").unwrap();

        assert_eq!(keenetic.vendor, HardwareVendor::Keenetic);
        assert_eq!(keenetic.model.as_str(), "3811");
        assert_eq!(netcraze.vendor, HardwareVendor::Netcraze);
        assert_eq!(netcraze.model.as_str(), "0012");
        assert_eq!(netcraze.to_string(), "NC-0012");
    }

    #[test]
    fn rejects_values_outside_the_current_format() {
        assert_eq!(
            HardwareId::parse("KN-381"),
            Err(ParseHardwareIdError::InvalidLength { actual: 6 })
        );
        assert_eq!(
            HardwareId::parse("KN_3811"),
            Err(ParseHardwareIdError::InvalidSeparator)
        );
        assert!(HardwareId::parse("ZY-3811").is_err());
        assert!(HardwareId::parse("NC-18x2").is_err());
    }

    #[test]
    fn serializes_and_deserializes_as_a_string() {
        let id = HardwareId::parse("NC-1812").unwrap();

        assert_eq!(serde_json::to_string(&id).unwrap(), "\"NC-1812\"");
        assert_eq!(
            serde_json::from_str::<HardwareId>("\"NC-1812\"").unwrap(),
            id
        );
    }
}
