use std::num::NonZeroU16;

use serde::Deserialize;
use thiserror::Error;

use crate::model::network::{ByteCount, Uptime};

/// Core runtime metrics returned by `show/system`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct System {
    /// Configured hostname.
    pub hostname: Box<str>,
    /// Configured domain name.
    #[serde(rename = "domainname")]
    pub domain_name: Box<str>,
    /// Human-readable uptime.
    pub uptime: Uptime,
    /// CPU utilization as an integer percentage.
    #[serde(rename = "cpuload")]
    pub cpu_load: CpuLoad,
    /// Total memory in bytes.
    #[serde(rename = "memtotal")]
    pub memory_total: ByteCount,
    /// Free memory in bytes.
    #[serde(rename = "memfree")]
    pub memory_free: ByteCount,
    /// Maximum tracked connections.
    #[serde(rename = "conntotal")]
    pub connection_capacity: u64,
    /// Available tracked connections.
    #[serde(rename = "connfree")]
    pub connections_available: u64,
}

/// CPU utilization represented as an integer percentage.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
#[serde(try_from = "u8")]
pub struct CpuLoad(u8);

impl CpuLoad {
    /// Creates a CPU utilization value.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidCpuLoad`] when `value` exceeds 100 percent.
    pub const fn new(value: u8) -> Result<Self, InvalidCpuLoad> {
        if value <= 100 {
            Ok(Self(value))
        } else {
            Err(InvalidCpuLoad(value))
        }
    }

    #[must_use]
    /// Returns the CPU utilization as an integer percentage.
    pub const fn percent(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for CpuLoad {
    type Error = InvalidCpuLoad;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// A CPU utilization percentage outside the supported range.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("CPU load must be between 0 and 100 percent, got {0}")]
pub struct InvalidCpuLoad(u8);

/// A network maximum transmission unit in bytes.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
#[serde(try_from = "u16")]
pub struct Mtu(NonZeroU16);

impl Mtu {
    /// Smallest MTU accepted by the model.
    pub const MIN: u16 = 64;
    /// Largest MTU representable by the model.
    pub const MAX: u16 = u16::MAX;

    /// Creates an MTU value.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidMtu`] when `value` is smaller than [`Mtu::MIN`].
    pub const fn new(value: u16) -> Result<Self, InvalidMtu> {
        match NonZeroU16::new(value) {
            Some(value) if value.get() >= Self::MIN => Ok(Self(value)),
            _ => Err(InvalidMtu(value)),
        }
    }

    /// Returns the MTU in bytes.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0.get()
    }
}

impl TryFrom<u16> for Mtu {
    type Error = InvalidMtu;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// An MTU below the minimum supported by the model.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("MTU must be at least {minimum} bytes, got {0}", minimum = Mtu::MIN)]
pub struct InvalidMtu(u16);

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{CpuLoad, Mtu, System};

    #[test]
    fn deserializes_system_units_and_uptime() {
        const MIB: u64 = 1024 * 1024;

        let system: System =
            serde_json::from_str(include_str!("../../tests/fixtures/show_system.json")).unwrap();

        assert_eq!(system.uptime.get(), Duration::from_secs(3_723));
        assert_eq!(system.cpu_load.percent(), 7);
        assert_eq!(system.memory_total.get(), 256 * MIB);
        assert_eq!(system.memory_free.get(), 128 * MIB);
    }

    #[test]
    fn constrained_values_reject_out_of_range_input() {
        assert!(CpuLoad::new(100).is_ok());
        assert!(CpuLoad::new(101).is_err());
        assert!(Mtu::new(Mtu::MIN).is_ok());
        assert!(Mtu::new(Mtu::MIN - 1).is_err());
        assert!(serde_json::from_str::<CpuLoad>("101").is_err());
        assert!(serde_json::from_str::<Mtu>("0").is_err());
    }
}
