use std::{fmt, net::Ipv4Addr, str::FromStr};

use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, IgnoredAny, MapAccess, SeqAccess},
};
use thiserror::Error;

use crate::model::{
    LinkState,
    hardware_id::HardwareId,
    network::{InterfaceId, MacAddress, Uptime},
    optional_nonempty_string,
    reported::Reported,
    system::CpuLoad,
    system_mode::SystemOperatingMode,
    version::{FirmwareChannel, HardwareType, RegionCode},
    wifi::WifiPeerLink,
};

/// Mesh controller state returned by `show/mws/status`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct MwsStatus {
    /// Whether automatic extender updates are enabled.
    #[serde(rename = "auto-update")]
    pub auto_update: bool,
    /// Controller-specific state.
    pub controller: MwsControllerStatus,
}

/// Controller-specific Mesh Wi-Fi System state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct MwsControllerStatus {
    /// Whether an extender update is pending.
    #[serde(rename = "update-pending")]
    pub update_pending: bool,
}

/// Captured extenders returned by `show/mws/member`.
#[derive(Clone, Debug, PartialEq)]
pub struct MwsMembers(Box<[MwsMember]>);

impl MwsMembers {
    /// Returns captured extenders in router order.
    #[must_use]
    pub const fn members(&self) -> &[MwsMember] {
        &self.0
    }

    /// Consumes the response and returns captured extenders.
    #[must_use]
    pub fn into_inner(self) -> Box<[MwsMember]> {
        self.0
    }
}

impl<'de> Deserialize<'de> for MwsMembers {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = MwsMembers;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an extender array or an empty object")
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut members = Vec::with_capacity(sequence.size_hint().unwrap_or(0));
                while let Some(member) = sequence.next_element()? {
                    members.push(member);
                }
                Ok(MwsMembers(members.into_boxed_slice()))
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                if map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {
                    return Err(de::Error::custom("expected an empty object"));
                }
                Ok(MwsMembers(Box::default()))
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

impl_slice_collection!(MwsMembers, MwsMember, 0, members);

/// Operational view of one captured extender.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[non_exhaustive]
pub struct MwsMember {
    /// Controller-assigned member identifier.
    pub cid: Box<str>,
    /// Displayed model name.
    pub model: Box<str>,
    /// Base MAC address.
    pub mac: MacAddress,
    /// Controller-known host name.
    #[serde(
        rename = "known-host",
        default,
        deserialize_with = "optional_nonempty_string"
    )]
    pub known_host: Option<Box<str>>,
    /// Physical port summaries.
    #[serde(rename = "port", default)]
    pub ports: Box<[MwsPort]>,
    /// Wireless hardware summary.
    pub wireless: Option<MwsWireless>,
    /// Management IPv4 address.
    pub ip: Ipv4Addr,
    /// Active operating role.
    pub mode: SystemOperatingMode,
    /// Hardware role reported by the member.
    pub hw_type: Option<HardwareType>,
    /// Parsed hardware identifier when recognized.
    #[serde(default)]
    pub hw_id: Option<Reported<HardwareId>>,
    /// Current displayed firmware version.
    #[serde(rename = "fw", default, deserialize_with = "optional_nonempty_string")]
    pub firmware: Option<Box<str>>,
    /// Available displayed firmware version.
    #[serde(
        rename = "fw-available",
        default,
        deserialize_with = "optional_nonempty_string"
    )]
    pub firmware_available: Option<Box<str>>,
    /// Current exact firmware release.
    #[serde(
        rename = "fw-release",
        default,
        deserialize_with = "optional_nonempty_string"
    )]
    pub firmware_release: Option<Box<str>>,
    /// Available exact firmware release.
    #[serde(
        rename = "fw-release-available",
        default,
        deserialize_with = "optional_nonempty_string"
    )]
    pub firmware_release_available: Option<Box<str>>,
    /// Selected update channel.
    #[serde(rename = "fw-update-sandbox")]
    pub firmware_channel: Option<FirmwareChannel>,
    /// Region code when recognized.
    #[serde(default)]
    pub region: Option<Reported<RegionCode>>,
    /// Number of client associations served by the member.
    pub associations: Option<u32>,
    /// Whether the member reports internet availability.
    #[serde(rename = "internet-available")]
    pub internet_available: Option<bool>,
    /// Runtime snapshot, absent for an unavailable member.
    pub system: Option<MwsMemberSystem>,
    /// Backhaul snapshot, absent while disconnected.
    pub backhaul: Option<MwsBackhaul>,
    /// RCI health summary.
    pub rci: Option<MwsRciStatus>,
}

/// One physical extender port.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct MwsPort {
    /// Display label.
    pub label: Box<str>,
    /// Router-defined appearance token.
    #[serde(default, deserialize_with = "optional_nonempty_string")]
    pub appearance: Option<Box<str>>,
    /// Current link state.
    pub link: LinkState,
}

/// Wireless hardware summary for an extender.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct MwsWireless {
    /// Radio bands exposed by the member.
    #[serde(rename = "band", default)]
    pub bands: Box<[MwsBand]>,
}

/// One extender radio index.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct MwsBand {
    /// Zero-based radio index.
    #[serde(deserialize_with = "deserialize_u8_string")]
    pub index: u8,
}

/// Runtime snapshot reported by an extender.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct MwsMemberSystem {
    /// CPU utilization.
    #[serde(rename = "cpuload")]
    pub cpu_load: CpuLoad,
    /// Free and total memory.
    pub memory: MwsMemoryUsage,
    /// Member uptime.
    pub uptime: Uptime,
}

/// Free and total extender memory in kibibytes.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MwsMemoryUsage {
    free_kib: u64,
    total_kib: u64,
}

impl MwsMemoryUsage {
    /// Returns free memory in kibibytes.
    #[must_use]
    pub const fn free_kib(self) -> u64 {
        self.free_kib
    }

    /// Returns total memory in kibibytes.
    #[must_use]
    pub const fn total_kib(self) -> u64 {
        self.total_kib
    }
}

impl FromStr for MwsMemoryUsage {
    type Err = InvalidMwsMemoryUsage;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (free, total) = value.split_once('/').ok_or(InvalidMwsMemoryUsage)?;
        let free_kib = free.parse().map_err(|_| InvalidMwsMemoryUsage)?;
        let total_kib = total.parse().map_err(|_| InvalidMwsMemoryUsage)?;
        if free_kib > total_kib {
            return Err(InvalidMwsMemoryUsage);
        }
        Ok(Self {
            free_kib,
            total_kib,
        })
    }
}

impl Serialize for MwsMemoryUsage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for MwsMemoryUsage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl de::Visitor<'_> for Visitor {
            type Value = MwsMemoryUsage;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a `free/total` extender memory value")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                value.parse().map_err(E::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

impl fmt::Display for MwsMemoryUsage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.free_kib, self.total_kib)
    }
}

/// An invalid `free/total` extender memory value.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("MWS memory usage must be a `free/total` pair with free not exceeding total")]
pub struct InvalidMwsMemoryUsage;

/// Operational wireless backhaul state.
///
/// Common link fields are available through [`MwsBackhaul::link`] and directly
/// through [`Deref`](std::ops::Deref).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[non_exhaustive]
#[allow(clippy::struct_excessive_bools)]
pub struct MwsBackhaul {
    /// Station-side uplink interface.
    pub uplink: InterfaceId,
    /// Spanning-tree root identifier.
    pub root: Box<str>,
    /// Spanning-tree bridge identifier.
    pub bridge: Box<str>,
    /// Backhaul path cost.
    pub cost: u32,
    /// Common Wi-Fi peer link.
    #[serde(flatten)]
    pub link: WifiPeerLink,
    /// Power-save state.
    pub psm: bool,
    /// Multi-link device state.
    pub mld: bool,
    /// Explicit beamforming state.
    pub ebf: bool,
    /// Downlink multi-user MIMO state.
    #[serde(rename = "dl-mu")]
    pub downlink_multi_user: bool,
    /// Uplink multi-user MIMO state.
    #[serde(rename = "ul-mu")]
    pub uplink_multi_user: bool,
    /// Downlink OFDMA state.
    #[serde(rename = "dl-ofdma")]
    pub downlink_ofdma: bool,
    /// Protected management frames state.
    pub pmf: bool,
}

impl std::ops::Deref for MwsBackhaul {
    type Target = WifiPeerLink;

    fn deref(&self) -> &Self::Target {
        &self.link
    }
}

/// RCI health summary reported by an extender.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct MwsRciStatus {
    /// Number of RCI errors reported by the member.
    pub errors: u64,
}

fn deserialize_u8_string<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: Deserializer<'de>,
{
    struct Visitor;

    impl de::Visitor<'_> for Visitor {
        type Value = u8;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a radio index integer or decimal integer string")
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            value.try_into().map_err(E::custom)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            value.parse().map_err(E::custom)
        }
    }

    deserializer.deserialize_any(Visitor)
}

#[cfg(test)]
mod tests {
    use super::{MwsMembers, MwsStatus};
    use crate::model::{system_mode::SystemOperatingMode, version::HardwareType};

    #[test]
    fn parses_controller_and_extender_operational_state() {
        let status: MwsStatus =
            serde_json::from_str(include_str!("../../tests/fixtures/show_mws_status.json"))
                .unwrap();
        let members: MwsMembers =
            serde_json::from_str(include_str!("../../tests/fixtures/show_mws_member.json"))
                .unwrap();

        assert!(!status.auto_update);
        assert_eq!(members.len(), 1);
        let member = &members[0];
        assert_eq!(member.mode, SystemOperatingMode::Extender);
        assert_eq!(member.hw_type, Some(HardwareType::Extender));
        assert_eq!(member.system.as_ref().unwrap().memory.total_kib(), 262_144);
        assert_eq!(
            member
                .backhaul
                .as_ref()
                .unwrap()
                .transmit_rate
                .bits_per_second(),
            288_000_000
        );
    }

    #[test]
    fn empty_object_is_an_empty_member_list() {
        assert!(serde_json::from_str::<MwsMembers>("{}").unwrap().is_empty());
        assert!(serde_json::from_str::<MwsMembers>(r#"{"member": []}"#).is_err());
    }

    #[test]
    fn normalizes_empty_optional_strings_and_round_trips_memory_usage() {
        let fixture = include_str!("../../tests/fixtures/show_mws_member.json")
            .replace(r#""known-host": "fixture-extender""#, r#""known-host": """#)
            .replace(r#""appearance": "gray-rj45""#, r#""appearance": """#);
        let members: MwsMembers = serde_json::from_str(&fixture).unwrap();

        assert_eq!(members[0].known_host, None);
        assert_eq!(members[0].ports[0].appearance, None);
        let memory = members[0].system.as_ref().unwrap().memory;
        assert_eq!(memory.to_string(), "170136/262144");
        assert_eq!(
            serde_json::to_string(&memory).unwrap(),
            r#""170136/262144""#
        );
    }
}
