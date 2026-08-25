use std::str::FromStr;

use thiserror::Error;

use super::text::{InlineAscii, InvalidInlineAscii};

/// The number of decimal digits in an IMEI.
pub const IMEI_LENGTH: usize = 15;

/// An IMEI failed structural or checksum validation.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum ParseImeiError {
    /// The value does not contain exactly 15 bytes.
    #[error("an IMEI must contain exactly {IMEI_LENGTH} digits, got {actual}")]
    InvalidLength {
        /// Actual byte length of the supplied value.
        actual: usize,
    },
    /// The value contains a byte outside the ASCII decimal digit range.
    #[error("an IMEI contains a non-decimal digit at byte index {index}")]
    InvalidDigit {
        /// Zero-based byte index of the invalid digit.
        index: usize,
    },
    /// The final digit does not satisfy the Luhn checksum.
    #[error("an IMEI has an invalid Luhn check digit")]
    InvalidChecksum,
}

/// A valid International Mobile Equipment Identity.
///
/// The value is stored inline as ASCII digits and does not allocate. Parsing
/// verifies both the fixed length and the Luhn check digit.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Imei(InlineAscii<IMEI_LENGTH>);

impl Imei {
    /// Parses and validates an IMEI.
    ///
    /// # Errors
    ///
    /// Returns [`ParseImeiError`] when the value is not exactly 15 ASCII
    /// digits or its Luhn check digit is invalid.
    pub fn parse(value: &str) -> Result<Self, ParseImeiError> {
        value.parse()
    }

    /// Returns the IMEI as a string slice.
    ///
    /// # Panics
    ///
    /// This can only panic if the type's private ASCII-only representation is
    /// corrupted. Values created through the public API always uphold it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Returns the IMEI as its fixed-size ASCII representation.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; IMEI_LENGTH] {
        self.0.as_array()
    }
}

impl FromStr for Imei {
    type Err = ParseImeiError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let encoded = InlineAscii::digits(value, IMEI_LENGTH).map_err(|error| match error {
            InvalidInlineAscii::Length(actual) => ParseImeiError::InvalidLength { actual },
            InvalidInlineAscii::Character(index) => ParseImeiError::InvalidDigit { index },
        })?;
        let mut checksum = 0_u8;

        for (index, byte) in encoded.as_bytes().iter().enumerate() {
            let digit = *byte - b'0';
            checksum += if index % 2 == 0 {
                digit
            } else {
                let doubled = digit * 2;
                if doubled > 9 { doubled - 9 } else { doubled }
            };
        }

        if !checksum.is_multiple_of(10) {
            return Err(ParseImeiError::InvalidChecksum);
        }

        Ok(Self(encoded))
    }
}

impl_string_value!(
    Imei,
    ParseImeiError,
    "a 15-digit IMEI with a valid Luhn check digit"
);

#[cfg(test)]
mod tests {
    use super::{Imei, ParseImeiError};

    const VALID: &str = "490154203237518";

    #[test]
    fn parses_without_allocating_and_preserves_leading_zeroes() {
        let imei = Imei::parse("000000000000000").unwrap();

        assert_eq!(imei.as_str(), "000000000000000");
        assert_eq!(imei.as_bytes(), b"000000000000000");
        assert_eq!(imei.to_string(), "000000000000000");
    }

    #[test]
    fn rejects_invalid_length_digit_and_checksum() {
        assert_eq!(
            "49015420323751".parse::<Imei>(),
            Err(ParseImeiError::InvalidLength { actual: 14 })
        );
        assert_eq!(
            "49015420323751x".parse::<Imei>(),
            Err(ParseImeiError::InvalidDigit { index: 14 })
        );
        assert_eq!(
            "490154203237519".parse::<Imei>(),
            Err(ParseImeiError::InvalidChecksum)
        );
    }

    #[test]
    fn serializes_and_deserializes_as_a_string() {
        let imei = VALID.parse::<Imei>().unwrap();

        assert_eq!(
            serde_json::to_string(&imei).unwrap(),
            format!("\"{VALID}\"")
        );
        assert_eq!(
            serde_json::from_str::<Imei>(&format!("\"{VALID}\"")).unwrap(),
            imei
        );
        assert!(serde_json::from_str::<Imei>("\"\"").is_err());
    }
}
