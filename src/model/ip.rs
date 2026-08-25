use std::net::Ipv4Addr;

use serde::Deserialize;

use crate::model::{
    network::{InterfaceId, LeaseExpiration, MacAddress},
    optional_nonempty_string,
};

/// ARP table returned by `show/ip/arp`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(transparent)]
pub struct IpArp(Box<[ArpEntry]>);

impl IpArp {
    /// Returns the ARP entries.
    #[must_use]
    pub const fn entries(&self) -> &[ArpEntry] {
        &self.0
    }

    /// Consumes the response and returns its entries.
    #[must_use]
    pub fn into_inner(self) -> Box<[ArpEntry]> {
        self.0
    }
}

impl_slice_collection!(IpArp, ArpEntry, 0, entries);

/// ARP neighbor-cache state.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum ArpState {
    /// Address resolution is in progress.
    Incomplete,
    /// The neighbor was recently confirmed.
    Reachable,
    /// The entry is valid but stale.
    Stale,
    /// Reachability confirmation is delayed.
    Delay,
    /// Reachability probes are being sent.
    Probe,
    /// Address resolution failed.
    Failed,
    /// A permanent entry.
    Permanent,
    /// A state introduced by another firmware version.
    Other(Box<str>),
}

/// One IPv4 ARP table entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct ArpEntry {
    /// IPv4 address.
    pub ip: Ipv4Addr,
    /// Link-layer address.
    pub mac: MacAddress,
    /// Interface carrying the neighbor.
    pub interface: InterfaceId,
    /// Optional resolved host name.
    #[serde(default, deserialize_with = "optional_nonempty_string")]
    pub name: Option<Box<str>>,
    /// Neighbor-cache state.
    pub state: ArpState,
}

open_string_enum!(ArpState {
    Incomplete => "INCOMPLETE",
    Reachable => "REACHABLE",
    Stale => "STALE",
    Delay => "DELAY",
    Probe => "PROBE",
    Failed => "FAILED",
    Permanent => "PERMANENT",
});

/// DHCP bindings returned by `show/ip/dhcp/bindings`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct IpDhcpBindings {
    /// DHCP lease records.
    #[serde(rename = "lease")]
    leases: Box<[DhcpBinding]>,
}

impl IpDhcpBindings {
    /// Returns the DHCP lease records.
    #[must_use]
    pub const fn leases(&self) -> &[DhcpBinding] {
        &self.leases
    }

    /// Consumes the response and returns its lease records.
    #[must_use]
    pub fn into_inner(self) -> Box<[DhcpBinding]> {
        self.leases
    }
}

impl_slice_collection!(IpDhcpBindings, DhcpBinding, leases, leases);

/// Router-specific DHCP binding mode.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum DhcpBindingMode {
    /// The lease belongs to a mesh extender.
    Extender,
    /// A mode introduced by another firmware version.
    Other(Box<str>),
}

/// One DHCP lease binding.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct DhcpBinding {
    /// Leased IPv4 address.
    pub ip: Ipv4Addr,
    /// Client MAC address.
    pub mac: MacAddress,
    /// Mesh path or directly observed MAC address.
    pub via: MacAddress,
    /// DHCP hostname, when non-empty.
    #[serde(default, deserialize_with = "optional_nonempty_string")]
    pub hostname: Option<Box<str>>,
    /// User-facing client name, when non-empty.
    #[serde(default, deserialize_with = "optional_nonempty_string")]
    pub name: Option<Box<str>>,
    /// Extender interface identifier when supplied.
    pub device: Option<InterfaceId>,
    /// Extender-specific binding mode.
    pub mode: Option<DhcpBindingMode>,
    /// Remaining lease lifetime.
    pub expires: LeaseExpiration,
}

open_string_enum!(DhcpBindingMode {
    Extender => "extender",
});

#[cfg(test)]
mod tests {
    use crate::model::network::LeaseExpiration;

    use super::{ArpState, IpArp, IpDhcpBindings};

    #[test]
    fn parses_open_arp_states_and_variable_names() {
        let arp: IpArp =
            serde_json::from_str(include_str!("../../tests/fixtures/show_ip_arp.json")).unwrap();

        assert_eq!(arp.len(), 3);
        assert_eq!(arp[1].name, None);
        assert!(matches!(&arp[2].state, ArpState::Other(value) if value.as_ref() == "FUTURE"));
    }

    #[test]
    fn parses_multiple_leases_and_infinite_expiry() {
        let dhcp: IpDhcpBindings = serde_json::from_str(include_str!(
            "../../tests/fixtures/show_ip_dhcp_bindings.json"
        ))
        .unwrap();

        assert_eq!(dhcp.len(), 3);
        assert_eq!(dhcp[1].expires, LeaseExpiration::Infinite);
        assert_eq!(dhcp[1].hostname, None);
    }
}
