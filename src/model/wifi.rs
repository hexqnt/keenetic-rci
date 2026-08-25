use serde::Deserialize;

use crate::model::{
    network::{
        ByteCount, ChannelWidth, DataRate, GuardInterval, InterfaceId, MacAddress, McsIndex,
        SpatialStreams, Uptime, deserialize_mbps,
    },
    units::{Dbm, measurement},
};

/// Wireless stations returned by `show/associations`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[non_exhaustive]
pub struct Associations {
    /// Currently associated stations.
    #[serde(rename = "station")]
    stations: Box<[Association]>,
}

impl Associations {
    /// Returns the associated stations.
    #[must_use]
    pub const fn stations(&self) -> &[Association] {
        &self.stations
    }

    /// Consumes the response and returns its stations.
    #[must_use]
    pub fn into_inner(self) -> Box<[Association]> {
        self.stations
    }
}

impl_slice_collection!(Associations, Association, stations, stations);

/// Wi-Fi PHY generation reported by the router.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum WifiMode {
    /// IEEE 802.11b.
    B,
    /// IEEE 802.11g.
    G,
    /// IEEE 802.11n (Wi-Fi 4).
    N,
    /// IEEE 802.11ac (Wi-Fi 5).
    Ac,
    /// IEEE 802.11ax (Wi-Fi 6/6E).
    Ax,
    /// A mode introduced by another firmware version.
    Other(Box<str>),
}

/// Wi-Fi peer link shared by station and mesh-backhaul responses.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[non_exhaustive]
pub struct WifiPeerLink {
    /// Access-point interface serving the peer.
    pub ap: InterfaceId,
    /// Whether the peer completed authentication.
    pub authenticated: bool,
    /// Current transmit PHY rate.
    #[serde(rename = "txrate", deserialize_with = "deserialize_mbps")]
    pub transmit_rate: DataRate,
    /// Time since the link was established.
    pub uptime: Uptime,
    /// Negotiated channel width.
    #[serde(rename = "ht")]
    pub channel_width: ChannelWidth,
    /// Negotiated Wi-Fi generation.
    pub mode: WifiMode,
    /// Guard interval.
    #[serde(rename = "gi")]
    pub guard_interval: GuardInterval,
    /// Received signal strength.
    #[serde(deserialize_with = "measurement")]
    pub rssi: Dbm,
    /// Modulation and coding scheme index.
    pub mcs: McsIndex,
    /// Number of transmit spatial streams.
    #[serde(rename = "txss")]
    pub transmit_spatial_streams: SpatialStreams,
    /// Link security suite.
    pub security: WifiSecurity,
}

/// One wireless station association.
///
/// Common link fields are available through [`Association::link`] and directly
/// through [`Deref`](std::ops::Deref).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[non_exhaustive]
pub struct Association {
    /// Station MAC address.
    pub mac: MacAddress,
    /// Common Wi-Fi peer link.
    #[serde(flatten)]
    pub link: WifiPeerLink,
    /// Whether power-save mode is active when reported.
    pub psm: Option<bool>,
    /// Bytes transmitted by the router to the station.
    #[serde(rename = "txbytes")]
    pub transmitted: ByteCount,
    /// Bytes received by the router from the station.
    #[serde(rename = "rxbytes")]
    pub received: ByteCount,
    /// Explicit beamforming state when reported.
    pub ebf: Option<bool>,
    /// Downlink multi-user MIMO state when reported.
    #[serde(rename = "dl-mu")]
    pub downlink_multi_user: Option<bool>,
    /// Uplink multi-user MIMO state when reported.
    #[serde(rename = "ul-mu")]
    pub uplink_multi_user: Option<bool>,
    /// Downlink OFDMA state when reported.
    #[serde(rename = "dl-ofdma")]
    pub downlink_ofdma: Option<bool>,
    /// Uplink OFDMA state when reported.
    #[serde(rename = "ul-ofdma")]
    pub uplink_ofdma: Option<bool>,
}

impl std::ops::Deref for Association {
    type Target = WifiPeerLink;

    fn deref(&self) -> &Self::Target {
        &self.link
    }
}

open_string_enum!(WifiMode {
    B => "11b",
    G => "11g",
    N => "11n",
    Ac => "11ac",
    Ax => "11ax",
});

/// Wireless security suite reported for an association.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum WifiSecurity {
    /// An open network.
    Open,
    /// WPA2 personal mode.
    Wpa2Psk,
    /// WPA3 personal mode.
    Wpa3Psk,
    /// A mixed WPA2/WPA3 personal mode.
    Wpa2Wpa3Psk,
    /// A suite introduced by another firmware version.
    Other(Box<str>),
}

open_string_enum!(WifiSecurity {
    Open => "open",
    Wpa2Psk => "wpa2-psk",
    Wpa3Psk => "wpa3-psk",
    Wpa2Wpa3Psk => "wpa2-wpa3-psk",
});

#[cfg(test)]
mod tests {
    use super::{Associations, WifiSecurity};

    #[test]
    fn parses_associations_and_preserves_future_security() {
        let associations: Associations =
            serde_json::from_str(include_str!("../../tests/fixtures/show_associations.json"))
                .unwrap();

        assert_eq!(associations.len(), 2);
        assert_eq!(associations[0].transmit_rate.bits_per_second(), 150_000_000);
        assert!(matches!(
            &associations[1].security,
            WifiSecurity::Other(value) if value.as_ref() == "future-suite"
        ));
    }
}
