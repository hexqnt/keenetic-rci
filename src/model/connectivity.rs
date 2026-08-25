use std::{collections::BTreeMap, net::IpAddr, num::NonZeroU16, time::Duration};

use serde::{Deserialize, Deserializer, de};

use crate::model::{
    network::{InterfaceId, NetworkHost, deserialize_optional_seconds},
    optional_nonempty_string,
};

/// Ping Check profiles returned by `show/ping-check`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct PingCheckProfiles {
    /// Configured and implicit profiles.
    #[serde(rename = "pingcheck", default)]
    profiles: Box<[PingCheckProfile]>,
}

impl PingCheckProfiles {
    /// Returns the profiles in router order.
    #[must_use]
    pub const fn profiles(&self) -> &[PingCheckProfile] {
        &self.profiles
    }

    /// Consumes the response and returns its profiles.
    #[must_use]
    pub fn into_inner(self) -> Box<[PingCheckProfile]> {
        self.profiles
    }
}

impl_slice_collection!(PingCheckProfiles, PingCheckProfile, profiles, profiles);

/// Ping Check probe mode.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum PingCheckMode {
    /// ICMP echo probes.
    Icmp,
    /// TCP connect probes.
    Connect,
    /// A mode introduced by another `KeeneticOS` release.
    Other(Box<str>),
}

/// One Ping Check profile and its per-interface state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct PingCheckProfile {
    /// Profile identifier.
    pub profile: Box<str>,
    /// Probe targets.
    #[serde(rename = "host", default)]
    pub targets: Box<[NetworkHost]>,
    /// Probe port for connect-mode checks.
    pub port: Option<NonZeroU16>,
    /// Period between checks.
    #[serde(
        rename = "update-interval",
        default,
        deserialize_with = "deserialize_optional_seconds"
    )]
    pub update_interval: Option<Duration>,
    /// Per-attempt timeout.
    #[serde(default, deserialize_with = "deserialize_optional_seconds")]
    pub timeout: Option<Duration>,
    /// Consecutive failures required to fail the check.
    #[serde(rename = "max-fails")]
    pub max_failures: Option<u32>,
    /// Probe mode.
    pub mode: Option<PingCheckMode>,
    /// State keyed by interface identifier.
    #[serde(rename = "interface", default)]
    pub interfaces: BTreeMap<InterfaceId, PingCheckInterface>,
}

/// Per-interface Ping Check state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct PingCheckInterface {
    /// Whether failures on this interface are ignored.
    #[serde(rename = "ignore-fail")]
    pub ignore_failure: bool,
    /// Number of successful checks.
    #[serde(rename = "successcount")]
    pub success_count: u64,
    /// Number of failed checks.
    #[serde(rename = "failcount")]
    pub failure_count: u64,
    /// Current state.
    pub status: PingCheckStatus,
    /// Cached resolutions used by the profile.
    #[serde(rename = "ipcache", default)]
    pub ip_cache: Box<[PingCheckCacheEntry]>,
}

/// One cached Ping Check name resolution.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct PingCheckCacheEntry {
    /// Original probe target.
    pub host: NetworkHost,
    /// Resolved addresses.
    pub addresses: Box<[IpAddr]>,
}

open_string_enum!(PingCheckMode {
    Icmp => "icmp",
    Connect => "connect",
});

/// Current per-interface Ping Check status.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum PingCheckStatus {
    /// The check is passing.
    Pass,
    /// The check is failing.
    Fail,
    /// The interface is not ready for checking.
    NotReady,
    /// A status introduced by another `KeeneticOS` release.
    Other(Box<str>),
}

open_string_enum!(PingCheckStatus {
    Pass => "pass",
    Fail => "fail",
    NotReady => "not ready",
});

/// Active upstream DNS servers returned by `show/ip/name-server`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct IpNameServers {
    /// Servers in router priority order.
    #[serde(rename = "server", default)]
    servers: Box<[IpNameServer]>,
}

impl IpNameServers {
    /// Returns the upstream servers.
    #[must_use]
    pub const fn servers(&self) -> &[IpNameServer] {
        &self.servers
    }

    /// Consumes the response and returns its servers.
    #[must_use]
    pub fn into_inner(self) -> Box<[IpNameServer]> {
        self.servers
    }
}

impl_slice_collection!(IpNameServers, IpNameServer, servers, servers);

/// One active upstream DNS server.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct IpNameServer {
    /// Resolver address.
    pub address: IpAddr,
    /// Domain suffix for a split-DNS resolver.
    #[serde(default, deserialize_with = "optional_nonempty_string")]
    pub domain: Option<Box<str>>,
    /// Global priority, with wire value zero represented as `None`.
    #[serde(rename = "global", deserialize_with = "optional_nonzero_u16")]
    pub global_priority: Option<NonZeroU16>,
    /// Router service that supplied the resolver.
    #[serde(default, deserialize_with = "optional_nonempty_string")]
    pub service: Option<Box<str>>,
    /// Interface that supplied or scopes the resolver.
    #[serde(default, deserialize_with = "optional_interface_id")]
    pub interface: Option<InterfaceId>,
}

/// NTP synchronization state returned by `show/ntp/status`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
#[allow(clippy::struct_excessive_bools)]
pub struct NtpStatus {
    /// Time elapsed since the current synchronization epoch.
    #[serde(deserialize_with = "crate::model::network::deserialize_seconds")]
    pub elapsed: Duration,
    /// Selected NTP server.
    pub server: NetworkHost,
    /// Whether the clock is considered accurate.
    pub accurate: bool,
    /// Whether NTP synchronization has completed.
    pub synchronized: bool,
    /// Router-reported NDSS time-source flag.
    #[serde(rename = "ndsstime")]
    pub ndss_time: bool,
    /// Router-reported user time-source flag.
    #[serde(rename = "usertime")]
    pub user_time: bool,
}

fn optional_nonzero_u16<'de, D>(deserializer: D) -> Result<Option<NonZeroU16>, D::Error>
where
    D: Deserializer<'de>,
{
    u16::deserialize(deserializer).map(NonZeroU16::new)
}

fn optional_interface_id<'de, D>(deserializer: D) -> Result<Option<InterfaceId>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<Box<str>>::deserialize(deserializer)?
        .filter(|value| !value.is_empty())
        .map(TryInto::try_into)
        .transpose()
        .map_err(de::Error::custom)
}

#[cfg(test)]
mod tests {
    use std::{net::IpAddr, time::Duration};

    use super::{IpNameServers, NtpStatus, PingCheckMode, PingCheckProfiles, PingCheckStatus};
    use crate::model::network::NetworkHost;

    #[test]
    fn parses_configured_and_implicit_ping_check_profiles() {
        let profiles: PingCheckProfiles =
            serde_json::from_str(include_str!("../../tests/fixtures/show_ping_check.json"))
                .unwrap();

        assert_eq!(profiles.len(), 3);
        assert_eq!(profiles[0].mode, None);
        assert!(matches!(profiles[1].mode, Some(PingCheckMode::Icmp)));
        assert_eq!(profiles[1].update_interval, Some(Duration::from_secs(10)));
        let state = &profiles[1].interfaces["GigabitEthernet0/Vlan2"];
        assert_eq!(state.status, PingCheckStatus::Pass);
        assert_eq!(state.ip_cache[0].addresses.len(), 2);
        assert!(matches!(
            profiles[2].targets[0],
            NetworkHost::Address(IpAddr::V4(_))
        ));
        assert_eq!(profiles[2].port.map(std::num::NonZero::get), Some(443));
    }

    #[test]
    fn rejects_zero_connect_port() {
        let fixture = include_str!("../../tests/fixtures/show_ping_check.json").replacen(
            r#""port": 443"#,
            r#""port": 0"#,
            1,
        );
        assert!(serde_json::from_str::<PingCheckProfiles>(&fixture).is_err());
    }

    #[test]
    fn open_ping_check_classifiers_preserve_future_values() {
        let mode: PingCheckMode = serde_json::from_str(r#""future-mode""#).unwrap();
        let status: PingCheckStatus = serde_json::from_str(r#""warming""#).unwrap();
        assert_eq!(mode.as_str(), "future-mode");
        assert_eq!(status.as_str(), "warming");
    }

    #[test]
    fn parses_dns_priorities_and_ntp_host() {
        let dns: IpNameServers = serde_json::from_str(include_str!(
            "../../tests/fixtures/show_ip_name_server.json"
        ))
        .unwrap();
        let ntp: NtpStatus =
            serde_json::from_str(include_str!("../../tests/fixtures/show_ntp_status.json"))
                .unwrap();

        assert_eq!(dns.len(), 2);
        assert_eq!(dns[0].global_priority, None);
        assert_eq!(dns[0].domain, None);
        assert_eq!(dns[0].interface, None);
        assert_eq!(
            dns[1].global_priority.map(std::num::NonZero::get),
            Some(61_481)
        );
        assert_eq!(ntp.elapsed, Duration::from_secs(240_849));
        assert_eq!(ntp.server.name(), Some("time.example.invalid"));
    }
}
