use std::fmt;

use serde::{Deserialize, Deserializer, de};

/// Router operating mode.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum SystemOperatingMode {
    /// Internet router mode.
    Router,
    /// Mesh extender mode.
    Extender,
    /// Standalone access-point mode.
    AccessPoint,
    /// Wireless adapter mode.
    Adapter,
    /// A mode introduced by another firmware version.
    Other(Box<str>),
}

/// Active and configured system operating modes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct SystemModeStatus {
    /// Currently active mode.
    pub active: SystemOperatingMode,
    /// Configured mode selected for startup.
    pub selected: SystemOperatingMode,
    /// Modes supported by this device.
    #[serde(deserialize_with = "deserialize_supported_modes")]
    pub supported: Box<[SystemOperatingMode]>,
    /// Whether hardware controls the mode.
    #[serde(rename = "hw_controlled")]
    pub hardware_controlled: bool,
    /// Whether hardware locks mode changes.
    #[serde(rename = "hw_locked")]
    pub hardware_locked: bool,
}

open_string_enum!(SystemOperatingMode {
    Router => "router",
    Extender => "extender",
    AccessPoint => "access-point",
    Adapter => "adapter",
});

fn deserialize_supported_modes<'de, D>(
    deserializer: D,
) -> Result<Box<[SystemOperatingMode]>, D::Error>
where
    D: Deserializer<'de>,
{
    struct Visitor;

    impl de::Visitor<'_> for Visitor {
        type Value = Box<[SystemOperatingMode]>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a comma-separated list of system modes")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            value
                .split(',')
                .map(str::trim)
                .map(|mode| {
                    if mode.is_empty() {
                        Err(E::custom("system mode must not be empty"))
                    } else {
                        Ok(SystemOperatingMode::from(mode))
                    }
                })
                .collect::<Result<Vec<_>, E>>()
                .map(Vec::into_boxed_slice)
        }
    }

    deserializer.deserialize_str(Visitor)
}

#[cfg(test)]
mod tests {
    use super::{SystemModeStatus, SystemOperatingMode};

    #[test]
    fn parses_supported_csv_and_preserves_unknown_modes() {
        let status: SystemModeStatus =
            serde_json::from_str(include_str!("../../tests/fixtures/show_system_mode.json"))
                .unwrap();

        assert_eq!(status.supported.len(), 3);
        assert!(matches!(
            &status.supported[2],
            SystemOperatingMode::Other(value) if value.as_ref() == "future-mode"
        ));
    }
}
