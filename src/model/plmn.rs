use std::str::FromStr;

use thiserror::Error;

use super::text::{InlineAscii, InvalidInlineAscii, deserialize_optional_from_str};

/// Minimum number of decimal digits in a PLMN identifier.
pub const MIN_PLMN_LENGTH: usize = 5;

/// Maximum number of decimal digits in a PLMN identifier.
pub const MAX_PLMN_LENGTH: usize = 6;

/// A PLMN identifier failed structural validation.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum ParsePlmnError {
    /// The value does not contain five or six bytes.
    #[error("a PLMN must contain exactly 5 or 6 digits, got {actual}")]
    InvalidLength {
        /// Actual byte length of the supplied value.
        actual: usize,
    },
    /// The value contains a byte outside the ASCII decimal digit range.
    #[error("a PLMN contains a non-decimal digit at byte index {index}")]
    InvalidDigit {
        /// Zero-based byte index of the invalid digit.
        index: usize,
    },
}

/// A valid Public Land Mobile Network identifier.
///
/// A PLMN consists of a three-digit Mobile Country Code (MCC) followed by a
/// two- or three-digit Mobile Network Code (MNC). It is stored inline as ASCII
/// digits so leading zeroes are preserved without a heap allocation.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Plmn(InlineAscii<MAX_PLMN_LENGTH>);

impl Plmn {
    /// Parses and validates a PLMN identifier.
    ///
    /// # Errors
    ///
    /// Returns [`ParsePlmnError`] unless the value contains exactly five or
    /// six ASCII decimal digits.
    pub fn parse(value: &str) -> Result<Self, ParsePlmnError> {
        value.parse()
    }

    /// Returns the complete PLMN identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Returns the three-digit Mobile Country Code.
    #[must_use]
    pub fn mcc(&self) -> &str {
        &self.as_str()[..3]
    }

    /// Returns the two- or three-digit Mobile Network Code.
    ///
    /// # Panics
    ///
    /// This can only panic if the type's private ASCII-only representation is
    /// corrupted. Values created through the public API always uphold it.
    #[must_use]
    pub fn mnc(&self) -> &str {
        &self.as_str()[3..]
    }
}

impl FromStr for Plmn {
    type Err = ParsePlmnError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        InlineAscii::digits(value, MIN_PLMN_LENGTH)
            .map(Self)
            .map_err(|error| match error {
                InvalidInlineAscii::Length(actual) => ParsePlmnError::InvalidLength { actual },
                InvalidInlineAscii::Character(index) => ParsePlmnError::InvalidDigit { index },
            })
    }
}

impl_string_value!(Plmn, ParsePlmnError, "a five- or six-digit PLMN identifier");

pub(super) fn deserialize_optional<'de, D>(deserializer: D) -> Result<Option<Plmn>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserialize_optional_from_str(deserializer)
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::{ParsePlmnError, Plmn};

    #[test]
    fn parses_both_mnc_lengths_and_preserves_leading_zeroes() {
        let five = Plmn::parse("00101").unwrap();
        assert_eq!(five.as_str(), "00101");
        assert_eq!(five.mcc(), "001");
        assert_eq!(five.mnc(), "01");

        let six = Plmn::parse("001001").unwrap();
        assert_eq!(six.as_str(), "001001");
        assert_eq!(six.mcc(), "001");
        assert_eq!(six.mnc(), "001");
    }

    #[test]
    fn rejects_invalid_length_and_non_ascii_digits() {
        assert_eq!(
            "2501".parse::<Plmn>(),
            Err(ParsePlmnError::InvalidLength { actual: 4 })
        );
        assert_eq!(
            "2501234".parse::<Plmn>(),
            Err(ParsePlmnError::InvalidLength { actual: 7 })
        );
        assert_eq!(
            "250x1".parse::<Plmn>(),
            Err(ParsePlmnError::InvalidDigit { index: 3 })
        );
        assert_eq!(
            "25é1".parse::<Plmn>(),
            Err(ParsePlmnError::InvalidDigit { index: 2 })
        );
    }

    #[test]
    fn serializes_and_deserializes_as_a_string() {
        let plmn = Plmn::parse("250011").unwrap();

        assert_eq!(serde_json::to_string(&plmn).unwrap(), r#""250011""#);
        assert_eq!(serde_json::from_str::<Plmn>(r#""250011""#).unwrap(), plmn);
        assert!(serde_json::from_str::<Plmn>("250011").is_err());
        assert!(serde_json::from_str::<Plmn>(r#""""#).is_err());
    }

    #[test]
    fn optional_rci_value_maps_empty_string_and_null_to_none() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct Wire {
            #[serde(default, deserialize_with = "super::deserialize_optional")]
            plmn: Option<Plmn>,
        }

        assert_eq!(
            serde_json::from_str::<Wire>(r#"{"plmn":""}"#).unwrap().plmn,
            None
        );
        assert_eq!(
            serde_json::from_str::<Wire>(r#"{"plmn":null}"#)
                .unwrap()
                .plmn,
            None
        );
        assert_eq!(serde_json::from_str::<Wire>("{}").unwrap().plmn, None);
        assert_eq!(
            serde_json::from_str::<Wire>(r#"{"plmn":"25011"}"#)
                .unwrap()
                .plmn
                .unwrap()
                .as_str(),
            "25011"
        );
        assert!(serde_json::from_str::<Wire>(r#"{"plmn":"2501x"}"#).is_err());
    }
}
