use std::str::FromStr;

use thiserror::Error;

use super::text::{InlineAscii, InvalidInlineAscii};

/// Minimum number of decimal digits in an ICCID.
pub const MIN_ICCID_LENGTH: usize = 19;

/// Maximum number of decimal digits in an ICCID.
pub const MAX_ICCID_LENGTH: usize = 20;

/// An ICCID failed structural validation.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum ParseIccidError {
    /// The value has an invalid number of bytes.
    #[error("an ICCID must contain 19 or 20 digits, got {actual}")]
    InvalidLength {
        /// Actual byte length of the supplied value.
        actual: usize,
    },
    /// The value contains a byte outside the ASCII decimal digit range.
    #[error("an ICCID contains a non-decimal digit at byte index {index}")]
    InvalidDigit {
        /// Zero-based byte index of the invalid digit.
        index: usize,
    },
}

/// A structurally valid Integrated Circuit Card Identifier.
///
/// The value is stored inline as ASCII digits, preserving leading zeroes
/// without a heap allocation.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Iccid(InlineAscii<MAX_ICCID_LENGTH>);

impl Iccid {
    /// Parses and validates an ICCID.
    ///
    /// # Errors
    ///
    /// Returns [`ParseIccidError`] for an invalid length or a non-decimal digit.
    pub fn parse(value: &str) -> Result<Self, ParseIccidError> {
        value.parse()
    }

    /// Returns the ICCID as a string slice.
    ///
    /// # Panics
    ///
    /// This can only panic if the type's private ASCII-only representation is
    /// corrupted. Values created through the public API always uphold it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Returns the ICCID as ASCII digits.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl FromStr for Iccid {
    type Err = ParseIccidError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        InlineAscii::digits(value, MIN_ICCID_LENGTH)
            .map(Self)
            .map_err(|error| match error {
                InvalidInlineAscii::Length(actual) => ParseIccidError::InvalidLength { actual },
                InvalidInlineAscii::Character(index) => ParseIccidError::InvalidDigit { index },
            })
    }
}

impl_string_value!(
    Iccid,
    ParseIccidError,
    "an ICCID containing 19 or 20 decimal digits"
);

#[cfg(test)]
mod tests {
    use super::{Iccid, ParseIccidError};

    const VALID: &str = "8901000000000000001";

    #[test]
    fn parses_without_allocating_and_preserves_leading_zeroes() {
        let iccid = Iccid::parse(VALID).unwrap();

        assert_eq!(iccid.as_str(), VALID);
        assert_eq!(iccid.as_bytes(), VALID.as_bytes());
        assert_eq!(iccid.to_string(), VALID);
    }

    #[test]
    fn rejects_invalid_length_and_non_ascii_digits() {
        assert_eq!(
            Iccid::parse("890100000000000001"),
            Err(ParseIccidError::InvalidLength { actual: 18 })
        );
        assert_eq!(
            Iccid::parse("8901000000000000000x"),
            Err(ParseIccidError::InvalidDigit { index: 19 })
        );
    }

    #[test]
    fn serializes_and_deserializes_as_a_string() {
        let iccid = Iccid::parse(VALID).unwrap();

        assert_eq!(
            serde_json::to_string(&iccid).unwrap(),
            format!("\"{VALID}\"")
        );
        assert_eq!(
            serde_json::from_str::<Iccid>(&format!("\"{VALID}\"")).unwrap(),
            iccid
        );
    }
}
