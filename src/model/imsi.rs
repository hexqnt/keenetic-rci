use std::str::FromStr;

use thiserror::Error;

use super::text::{InlineAscii, InvalidInlineAscii};

/// Minimum number of decimal digits in an IMSI.
pub const MIN_IMSI_LENGTH: usize = 6;

/// Maximum number of decimal digits in an IMSI.
pub const MAX_IMSI_LENGTH: usize = 15;

/// An IMSI failed structural validation.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum ParseImsiError {
    /// The value has an invalid number of bytes.
    #[error("an IMSI must contain between 6 and 15 digits, got {actual}")]
    InvalidLength {
        /// Actual byte length of the supplied value.
        actual: usize,
    },
    /// The value contains a byte outside the ASCII decimal digit range.
    #[error("an IMSI contains a non-decimal digit at byte index {index}")]
    InvalidDigit {
        /// Zero-based byte index of the invalid digit.
        index: usize,
    },
}

/// A structurally valid International Mobile Subscriber Identity.
///
/// The value is stored inline as ASCII digits, preserving leading zeroes
/// without a heap allocation.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Imsi(InlineAscii<MAX_IMSI_LENGTH>);

impl Imsi {
    /// Parses and validates an IMSI.
    ///
    /// # Errors
    ///
    /// Returns [`ParseImsiError`] for an invalid length or a non-decimal digit.
    pub fn parse(value: &str) -> Result<Self, ParseImsiError> {
        value.parse()
    }

    /// Returns the IMSI as a string slice.
    ///
    /// # Panics
    ///
    /// This can only panic if the type's private ASCII-only representation is
    /// corrupted. Values created through the public API always uphold it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Returns the IMSI as ASCII digits.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl FromStr for Imsi {
    type Err = ParseImsiError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        InlineAscii::digits(value, MIN_IMSI_LENGTH)
            .map(Self)
            .map_err(|error| match error {
                InvalidInlineAscii::Length(actual) => ParseImsiError::InvalidLength { actual },
                InvalidInlineAscii::Character(index) => ParseImsiError::InvalidDigit { index },
            })
    }
}

impl_string_value!(
    Imsi,
    ParseImsiError,
    "an IMSI containing between 6 and 15 decimal digits"
);

#[cfg(test)]
mod tests {
    use super::{Imsi, ParseImsiError};

    const VALID: &str = "001010000000001";

    #[test]
    fn parses_without_allocating_and_preserves_leading_zeroes() {
        let imsi = Imsi::parse(VALID).unwrap();

        assert_eq!(imsi.as_str(), VALID);
        assert_eq!(imsi.as_bytes(), VALID.as_bytes());
        assert_eq!(imsi.to_string(), VALID);
    }

    #[test]
    fn rejects_invalid_length_and_non_ascii_digits() {
        assert_eq!(
            Imsi::parse("00101"),
            Err(ParseImsiError::InvalidLength { actual: 5 })
        );
        assert_eq!(
            Imsi::parse("00101000000000x"),
            Err(ParseImsiError::InvalidDigit { index: 14 })
        );
    }

    #[test]
    fn serializes_and_deserializes_as_a_string() {
        let imsi = Imsi::parse(VALID).unwrap();

        assert_eq!(
            serde_json::to_string(&imsi).unwrap(),
            format!("\"{VALID}\"")
        );
        assert_eq!(
            serde_json::from_str::<Imsi>(&format!("\"{VALID}\"")).unwrap(),
            imsi
        );
    }
}
