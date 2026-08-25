use std::{
    net::{Ipv4Addr, Ipv6Addr},
    time::Duration,
};

use serde::Deserialize;

use crate::model::{
    LinkState,
    network::{
        ByteCount, ChannelWidth, DataRate, GuardInterval, InterfaceId, LeaseExpiration, MacAddress,
        McsIndex, SpatialStreams, Uptime, deserialize_kbps, deserialize_optional_ipv4,
        deserialize_optional_mbps, deserialize_optional_seconds,
        deserialize_optional_u8_string_or_number,
    },
    optional_nonempty_string,
    units::{Dbm, optional_measurement},
    wifi::{WifiMode, WifiSecurity},
};

/// Hosts known to the Keenetic hotspot subsystem.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[non_exhaustive]
pub struct IpHotspotHosts {
    /// Known host records.
    #[serde(rename = "host")]
    hosts: Box<[HotspotHost]>,
}

impl IpHotspotHosts {
    /// Returns the known host records.
    #[must_use]
    pub const fn hosts(&self) -> &[HotspotHost] {
        &self.hosts
    }

    /// Consumes the response and returns its host records.
    #[must_use]
    pub fn into_inner(self) -> Box<[HotspotHost]> {
        self.hosts
    }
}

impl_slice_collection!(IpHotspotHosts, HotspotHost, hosts, hosts);

/// Hotspot access decision.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum HotspotAccess {
    /// Network access is allowed.
    Permit,
    /// Network access is denied.
    Deny,
    /// A decision introduced by another firmware version.
    Other(Box<str>),
}

/// A host record enriched by the router's hotspot subsystem.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[non_exhaustive]
pub struct HotspotHost {
    /// Client MAC address.
    pub mac: MacAddress,
    /// Mesh path or directly observed MAC address.
    pub via: MacAddress,
    /// Current IPv4 address; `0.0.0.0` is represented as `None`.
    #[serde(deserialize_with = "deserialize_optional_ipv4")]
    pub ip: Option<Ipv4Addr>,
    /// Current IPv6 addresses.
    #[serde(default)]
    pub ip6: Box<[Ipv6Addr]>,
    /// DHCP hostname when non-empty.
    #[serde(default, deserialize_with = "optional_nonempty_string")]
    pub hostname: Option<Box<str>>,
    /// User-facing registered name when non-empty.
    #[serde(default, deserialize_with = "optional_nonempty_string")]
    pub name: Option<Box<str>>,
    /// Logical network interface when known.
    pub interface: Option<InterfaceReference>,
    /// DHCP lease information when present.
    pub dhcp: Option<HotspotDhcp>,
    /// Whether the host is registered in the router configuration.
    pub registered: bool,
    /// Configured access decision.
    pub access: HotspotAccess,
    /// Optional access schedule name.
    #[serde(default, deserialize_with = "optional_nonempty_string")]
    pub schedule: Option<Box<str>>,
    /// Optional connection policy name.
    #[serde(default, deserialize_with = "optional_nonempty_string")]
    pub policy: Option<Box<str>>,
    /// Host priority assigned by the router.
    pub priority: u8,
    /// Whether the router currently considers the host active.
    #[serde(default)]
    pub active: bool,
    /// Bytes received by the router from the host.
    #[serde(rename = "rxbytes")]
    pub received: ByteCount,
    /// Bytes transmitted by the router to the host.
    #[serde(rename = "txbytes")]
    pub transmitted: ByteCount,
    /// Time the current connection has been active.
    pub uptime: Uptime,
    /// Time elapsed since the host was first seen in the current observation window.
    #[serde(
        rename = "first-seen",
        default,
        deserialize_with = "deserialize_optional_seconds"
    )]
    pub first_seen: Option<Duration>,
    /// Time elapsed since the host was last seen.
    #[serde(
        rename = "last-seen",
        default,
        deserialize_with = "deserialize_optional_seconds"
    )]
    pub last_seen: Option<Duration>,
    /// Physical or wireless link state.
    pub link: Option<LinkState>,
    /// Wired auto-negotiation state.
    #[serde(rename = "auto-negotiation")]
    pub auto_negotiation: Option<bool>,
    /// Wired full-duplex state.
    pub duplex: Option<bool>,
    /// Zero-based physical Ethernet port number.
    #[serde(default, deserialize_with = "deserialize_optional_u8_string_or_number")]
    pub port: Option<u8>,
    /// Wired link rate.
    #[serde(default, deserialize_with = "deserialize_optional_mbps")]
    pub speed: Option<DataRate>,
    /// Wireless network name.
    #[serde(default, deserialize_with = "optional_nonempty_string")]
    pub ssid: Option<Box<str>>,
    /// Access-point interface for wireless clients.
    pub ap: Option<InterfaceId>,
    /// Wireless power-save state.
    pub psm: Option<bool>,
    /// Wireless authentication state.
    pub authenticated: Option<bool>,
    /// Current wireless transmit PHY rate.
    #[serde(
        rename = "txrate",
        default,
        deserialize_with = "deserialize_optional_mbps"
    )]
    pub transmit_rate: Option<DataRate>,
    /// Wireless channel width.
    #[serde(rename = "ht")]
    pub channel_width: Option<ChannelWidth>,
    /// Wireless PHY generation.
    pub mode: Option<WifiMode>,
    /// Wireless guard interval.
    #[serde(rename = "gi")]
    pub guard_interval: Option<GuardInterval>,
    /// Wireless received signal strength.
    #[serde(default, deserialize_with = "optional_measurement")]
    pub rssi: Option<Dbm>,
    /// Wireless modulation and coding scheme index.
    pub mcs: Option<McsIndex>,
    /// Number of transmit spatial streams.
    #[serde(rename = "txss")]
    pub transmit_spatial_streams: Option<SpatialStreams>,
    /// Explicit beamforming state.
    pub ebf: Option<bool>,
    /// Wireless security suite.
    pub security: Option<WifiSecurity>,
    /// Configured traffic limits.
    #[serde(rename = "traffic-shape")]
    pub traffic_shape: TrafficShape,
}

/// A compact reference to a router interface.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct InterfaceReference {
    /// Stable interface identifier.
    pub id: InterfaceId,
    /// User-facing interface name.
    pub name: Box<str>,
    /// User-facing interface description.
    pub description: Box<str>,
}

/// DHCP information embedded into a hotspot host record.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct HotspotDhcp {
    /// Remaining lease lifetime.
    pub expires: LeaseExpiration,
}

/// Per-host traffic shaping configuration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct TrafficShape {
    /// Receive limit normalized from kilobits per second.
    #[serde(rename = "rx", deserialize_with = "deserialize_kbps")]
    pub receive: DataRate,
    /// Transmit limit normalized from kilobits per second.
    #[serde(rename = "tx", deserialize_with = "deserialize_kbps")]
    pub transmit: DataRate,
    /// Router-selected shaping key.
    pub mode: TrafficShapeMode,
    /// Optional shaping schedule.
    #[serde(default, deserialize_with = "optional_nonempty_string")]
    pub schedule: Option<Box<str>>,
}

open_string_enum!(HotspotAccess {
    Permit => "permit",
    Deny => "deny",
});

/// Key used for per-host traffic shaping.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum TrafficShapeMode {
    /// Shape by MAC address.
    Mac,
    /// A mode introduced by another firmware version.
    Other(Box<str>),
}

open_string_enum!(TrafficShapeMode {
    Mac => "mac",
});

#[cfg(test)]
mod tests {
    use super::IpHotspotHosts;

    #[test]
    fn parses_wired_wireless_and_inactive_hosts() {
        let hosts: IpHotspotHosts =
            serde_json::from_str(include_str!("../../tests/fixtures/show_ip_hotspot.json"))
                .unwrap();

        assert_eq!(hosts.len(), 3);
        assert_eq!(hosts[2].ip, None);
        assert_eq!(hosts[2].hostname, None);
        assert_eq!(
            hosts[0]
                .transmit_rate
                .map(super::super::network::DataRate::bits_per_second),
            Some(150_000_000)
        );
        assert_eq!(hosts[0].traffic_shape.receive.bits_per_second(), 20_000_000);
        assert_eq!(hosts[1].port, Some(2));
        assert_eq!(
            hosts[1]
                .speed
                .map(super::super::network::DataRate::bits_per_second),
            Some(1_000_000_000)
        );
    }

    #[test]
    fn wired_port_accepts_zero_and_rejects_non_numeric_values() {
        let fixture = include_str!("../../tests/fixtures/show_ip_hotspot.json");

        let zero: IpHotspotHosts =
            serde_json::from_str(&fixture.replace(r#""port": "2""#, r#""port": "0""#)).unwrap();
        assert_eq!(zero[1].port, Some(0));

        let number: IpHotspotHosts =
            serde_json::from_str(&fixture.replace(r#""port": "2""#, r#""port": 2"#)).unwrap();
        assert_eq!(number[1].port, Some(2));

        assert!(
            serde_json::from_str::<IpHotspotHosts>(
                &fixture.replace(r#""port": "2""#, r#""port": "lan""#)
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<IpHotspotHosts>(
                &fixture.replace(r#""port": "2""#, r#""port": -1"#)
            )
            .is_err()
        );
    }
}
