use std::str::FromStr;

use serde::Deserialize;
use thiserror::Error;

use super::hardware_id::HardwareVendor;
use super::text::{InlineAscii, InvalidInlineAscii};

/// The number of decimal digits in a hardware version.
pub const HARDWARE_VERSION_LENGTH: usize = 8;

/// The number of ASCII letters in a region code.
pub const REGION_CODE_LENGTH: usize = 2;

/// A hardware version failed structural validation.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum ParseHardwareVersionError {
    /// The value has an unexpected byte length.
    #[error(
        "a hardware version must contain exactly {HARDWARE_VERSION_LENGTH} digits, got {actual}"
    )]
    InvalidLength {
        /// Actual byte length of the supplied value.
        actual: usize,
    },
    /// The value contains a byte outside the ASCII decimal digit range.
    #[error("a hardware version contains a non-decimal digit at byte index {index}")]
    InvalidDigit {
        /// Zero-based byte index of the invalid digit.
        index: usize,
    },
}

/// A region code failed structural validation.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum ParseRegionCodeError {
    /// The value has an unexpected byte length.
    #[error("a region code must contain exactly {REGION_CODE_LENGTH} letters, got {actual}")]
    InvalidLength {
        /// Actual byte length of the supplied value.
        actual: usize,
    },
    /// The value contains a byte outside the uppercase ASCII letter range.
    #[error("a region code contains a non-uppercase ASCII letter at byte index {index}")]
    InvalidCharacter {
        /// Zero-based byte index of the invalid character.
        index: usize,
    },
}

/// Processor architecture reported by `KeeneticOS`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum Architecture {
    /// 32-bit MIPS architecture.
    Mips,
    /// 64-bit ARM architecture.
    Aarch64,
    /// An architecture introduced by a different firmware or device.
    Other(Box<str>),
}

open_string_enum!(Architecture {
    Mips => "mips",
    Aarch64 => "aarch64",
});

/// Firmware release channel reported in the `sandbox` field.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum FirmwareChannel {
    /// Stable firmware channel.
    Stable,
    /// A channel introduced by a different firmware version.
    Other(Box<str>),
}

open_string_enum!(FirmwareChannel {
    Stable => "stable",
});

/// Hardware role reported by `KeeneticOS`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum HardwareType {
    /// A router or internet center.
    Router,
    /// A Mesh Wi-Fi System extender.
    Extender,
    /// A role introduced by a different device or firmware version.
    Other(Box<str>),
}

open_string_enum!(HardwareType {
    Router => "router",
    Extender => "extender",
});

/// An eight-digit hardware revision identifier.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct HardwareVersion(InlineAscii<HARDWARE_VERSION_LENGTH>);

impl HardwareVersion {
    /// Parses an eight-digit hardware revision identifier.
    ///
    /// # Errors
    ///
    /// Returns [`ParseHardwareVersionError`] unless the value contains exactly
    /// eight ASCII decimal digits.
    pub fn parse(value: &str) -> Result<Self, ParseHardwareVersionError> {
        value.parse()
    }

    /// Returns the hardware revision as a string slice.
    ///
    /// # Panics
    ///
    /// This can only panic if the type's private ASCII-only representation is
    /// corrupted. Values created through the public API always uphold it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Returns the fixed-size ASCII representation.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; HARDWARE_VERSION_LENGTH] {
        self.0.as_array()
    }
}

impl FromStr for HardwareVersion {
    type Err = ParseHardwareVersionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        InlineAscii::digits(value, HARDWARE_VERSION_LENGTH)
            .map(Self)
            .map_err(|error| match error {
                InvalidInlineAscii::Length(actual) => {
                    ParseHardwareVersionError::InvalidLength { actual }
                }
                InvalidInlineAscii::Character(index) => {
                    ParseHardwareVersionError::InvalidDigit { index }
                }
            })
    }
}

impl_string_value!(
    HardwareVersion,
    ParseHardwareVersionError,
    "an eight-digit hardware version"
);

/// A two-letter uppercase device region code.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RegionCode(InlineAscii<REGION_CODE_LENGTH>);

impl RegionCode {
    /// Parses a two-letter uppercase ASCII region code.
    ///
    /// # Errors
    ///
    /// Returns [`ParseRegionCodeError`] unless the value contains exactly two
    /// uppercase ASCII letters.
    pub fn parse(value: &str) -> Result<Self, ParseRegionCodeError> {
        value.parse()
    }

    /// Returns the region code as a string slice.
    ///
    /// # Panics
    ///
    /// This can only panic if the type's private ASCII-only representation is
    /// corrupted. Values created through the public API always uphold it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Returns the fixed-size ASCII representation.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; REGION_CODE_LENGTH] {
        self.0.as_array()
    }
}

impl FromStr for RegionCode {
    type Err = ParseRegionCodeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        InlineAscii::uppercase(value)
            .map(Self)
            .map_err(|error| match error {
                InvalidInlineAscii::Length(actual) => {
                    ParseRegionCodeError::InvalidLength { actual }
                }
                InvalidInlineAscii::Character(index) => {
                    ParseRegionCodeError::InvalidCharacter { index }
                }
            })
    }
}

impl_string_value!(
    RegionCode,
    ParseRegionCodeError,
    "a two-letter uppercase ASCII region code"
);

/// Firmware and hardware information returned by `show/version`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct Version {
    /// Displayed `KeeneticOS` version.
    pub title: Box<str>,
    /// Exact `KeeneticOS` release identifier.
    pub release: Box<str>,
    /// Processor architecture.
    pub arch: Architecture,
    /// Firmware release channel, when reported by the firmware.
    pub sandbox: Option<FirmwareChannel>,
    /// Device manufacturer.
    pub manufacturer: Box<str>,
    /// Hardware series code.
    pub series: HardwareVendor,
    /// Router model name.
    pub model: Box<str>,
    /// Hardware revision identifier.
    pub hw_version: HardwareVersion,
    /// Hardware role, when reported by the firmware.
    pub hw_type: Option<HardwareType>,
    /// Keenetic or Netcraze hardware identifier.
    pub hw_id: super::hardware_id::HardwareId,
    /// Device sales region.
    pub region: RegionCode,
    /// NDM build metadata.
    pub ndm: VersionBuild,
    /// Installed software components and hardware features.
    pub ndw: VersionCapabilities,
}

/// Build metadata embedded in a version response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct VersionBuild {
    /// Exact build identifier.
    pub exact: Box<str>,
    /// Build date reported by the router.
    pub cdate: Box<str>,
}

/// Capability strings embedded in a version response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct VersionCapabilities {
    /// Comma-separated installed components.
    pub components: Box<str>,
    /// Comma-separated hardware/platform features.
    pub features: Box<str>,
}

#[cfg(test)]
mod tests {
    use super::{
        Architecture, FirmwareChannel, HardwareType, HardwareVersion, ParseHardwareVersionError,
        ParseRegionCodeError, RegionCode, Version,
    };
    use crate::model::hardware_id::HardwareVendor;

    #[test]
    fn parses_fixed_size_version_values_without_allocation() {
        let hardware_version = HardwareVersion::parse("10388000").unwrap();
        let region = RegionCode::parse("EA").unwrap();

        assert_eq!(hardware_version.as_str(), "10388000");
        assert_eq!(region.as_str(), "EA");
        assert_eq!(hardware_version.to_string(), "10388000");
        assert_eq!(region.to_string(), "EA");
    }

    #[test]
    fn rejects_malformed_fixed_size_version_values() {
        assert_eq!(
            HardwareVersion::parse("1038800"),
            Err(ParseHardwareVersionError::InvalidLength { actual: 7 })
        );
        assert_eq!(
            HardwareVersion::parse("10388x00"),
            Err(ParseHardwareVersionError::InvalidDigit { index: 5 })
        );
        assert_eq!(
            RegionCode::parse("EAA"),
            Err(ParseRegionCodeError::InvalidLength { actual: 3 })
        );
        assert_eq!(
            RegionCode::parse("Ea"),
            Err(ParseRegionCodeError::InvalidCharacter { index: 1 })
        );
    }

    #[test]
    fn deserializes_typed_version_fields() {
        let version: Version =
            serde_json::from_str(include_str!("../../tests/fixtures/show_version.json")).unwrap();

        assert_eq!(version.arch, Architecture::Mips);
        assert_eq!(version.sandbox, Some(FirmwareChannel::Stable));
        assert_eq!(version.series, HardwareVendor::Keenetic);
        assert_eq!(version.hw_version.as_str(), "10000000");
        assert_eq!(version.hw_type, Some(HardwareType::Router));
        assert_eq!(version.region.as_str(), "EA");
    }

    #[test]
    fn accepts_firmware_without_optional_classifiers() {
        let mut fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/show_version.json")).unwrap();
        let object = fixture.as_object_mut().unwrap();
        object.remove("sandbox");
        object.remove("hw_type");

        let version: Version = serde_json::from_value(fixture).unwrap();

        assert_eq!(version.sandbox, None);
        assert_eq!(version.hw_type, None);
    }

    #[test]
    fn preserves_unknown_open_classifiers() {
        assert_eq!(
            serde_json::from_str::<Architecture>(r#""riscv64""#).unwrap(),
            Architecture::Other("riscv64".into())
        );
        assert_eq!(
            serde_json::from_str::<FirmwareChannel>(r#""preview""#).unwrap(),
            FirmwareChannel::Other("preview".into())
        );
        assert_eq!(
            serde_json::from_str::<HardwareType>(r#""extender""#).unwrap(),
            HardwareType::Extender
        );
    }
}
