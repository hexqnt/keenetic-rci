use std::{
    collections::BTreeMap,
    net::{Ipv4Addr, Ipv6Addr},
};

use crate::model::{
    hotspot::{HotspotHost, InterfaceReference, IpHotspotHosts},
    ip::{ArpEntry, DhcpBinding, IpArp, IpDhcpBindings},
    network::{InterfaceId, MacAddress},
    wifi::{Association, Associations},
};

/// Activity classification derived without treating ARP cache presence as connectivity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ClientActivity {
    /// An association or active hotspot record proves current activity.
    Active,
    /// A hotspot record exists but carries no active evidence.
    Inactive,
    /// The client is only known through ARP or DHCP.
    Unknown,
}

/// Borrowed records for one MAC address.
#[derive(Debug)]
#[non_exhaustive]
pub struct ConnectedClient<'a> {
    mac: MacAddress,
    association: Option<&'a Association>,
    hotspot: Option<&'a HotspotHost>,
    arp: Vec<&'a ArpEntry>,
    dhcp: Vec<&'a DhcpBinding>,
}

impl<'a> ConnectedClient<'a> {
    /// Returns the shared MAC address key.
    #[must_use]
    pub const fn mac(&self) -> MacAddress {
        self.mac
    }

    /// Returns the wireless association, when present.
    #[must_use]
    pub const fn association(&self) -> Option<&'a Association> {
        self.association
    }

    /// Returns the hotspot host, when present.
    #[must_use]
    pub const fn hotspot(&self) -> Option<&'a HotspotHost> {
        self.hotspot
    }

    /// Returns every ARP record for the MAC address.
    #[must_use]
    pub fn arp(&self) -> &[&'a ArpEntry] {
        &self.arp
    }

    /// Returns every DHCP lease for the MAC address.
    #[must_use]
    pub fn dhcp(&self) -> &[&'a DhcpBinding] {
        &self.dhcp
    }

    /// Classifies current activity using association and hotspot evidence.
    #[must_use]
    pub const fn activity(&self) -> ClientActivity {
        if self.association.is_some() || matches!(self.hotspot, Some(host) if host.active) {
            ClientActivity::Active
        } else if self.hotspot.is_some() {
            ClientActivity::Inactive
        } else {
            ClientActivity::Unknown
        }
    }

    /// Returns all observed IPv4 addresses without applying source precedence.
    pub fn ipv4_addresses(&self) -> impl Iterator<Item = Ipv4Addr> + '_ {
        self.hotspot
            .and_then(|host| host.ip)
            .into_iter()
            .chain(self.arp.iter().map(|entry| entry.ip))
            .chain(self.dhcp.iter().map(|binding| binding.ip))
    }

    /// Returns all hotspot IPv6 addresses.
    pub fn ipv6_addresses(&self) -> impl Iterator<Item = Ipv6Addr> + '_ {
        self.hotspot
            .into_iter()
            .flat_map(|host| host.ip6.iter().copied())
    }

    /// Returns all non-empty names without applying source precedence.
    pub fn names(&self) -> impl Iterator<Item = &str> + '_ {
        self.hotspot
            .into_iter()
            .flat_map(|host| [host.name.as_deref(), host.hostname.as_deref()])
            .flatten()
            .chain(self.arp.iter().filter_map(|entry| entry.name.as_deref()))
            .chain(
                self.dhcp
                    .iter()
                    .flat_map(|lease| [lease.name.as_deref(), lease.hostname.as_deref()])
                    .flatten(),
            )
    }

    /// Returns the structured hotspot interface reference, when present.
    #[must_use]
    pub fn hotspot_interface(&self) -> Option<&InterfaceReference> {
        self.hotspot.and_then(|host| host.interface.as_ref())
    }

    /// Returns every referenced interface identifier without applying precedence.
    pub fn interface_ids(&self) -> impl Iterator<Item = &InterfaceId> + '_ {
        self.association
            .map(|association| &association.ap)
            .into_iter()
            .chain(
                self.hotspot
                    .and_then(|host| host.interface.as_ref().map(|interface| &interface.id)),
            )
            .chain(self.hotspot.and_then(|host| host.ap.as_ref()))
            .chain(self.arp.iter().map(|entry| &entry.interface))
            .chain(self.dhcp.iter().filter_map(|lease| lease.device.as_ref()))
    }
}

/// A borrowed index joining client records by parsed MAC address.
#[derive(Debug)]
pub struct ClientIndex<'a>(BTreeMap<MacAddress, ConnectedClient<'a>>);

impl<'a> ClientIndex<'a> {
    /// Builds a union of four response snapshots without cloning their records.
    #[must_use]
    pub fn new(
        associations: &'a Associations,
        hotspot: &'a IpHotspotHosts,
        arp: &'a IpArp,
        dhcp: &'a IpDhcpBindings,
    ) -> Self {
        let mut clients = BTreeMap::new();
        for association in associations {
            entry(&mut clients, association.mac).association = Some(association);
        }
        for host in hotspot {
            entry(&mut clients, host.mac).hotspot = Some(host);
        }
        for arp_entry in arp {
            entry(&mut clients, arp_entry.mac).arp.push(arp_entry);
        }
        for lease in dhcp {
            entry(&mut clients, lease.mac).dhcp.push(lease);
        }
        Self(clients)
    }

    /// Returns a client by MAC address.
    #[must_use]
    pub fn get(&self, mac: &MacAddress) -> Option<&ConnectedClient<'a>> {
        self.0.get(mac)
    }

    /// Iterates over clients in MAC-address order.
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &ConnectedClient<'a>> {
        self.0.values()
    }

    /// Returns the number of distinct MAC addresses.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Reports whether the index is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<'index, 'source> IntoIterator for &'index ClientIndex<'source> {
    type Item = &'index ConnectedClient<'source>;
    type IntoIter =
        std::collections::btree_map::Values<'index, MacAddress, ConnectedClient<'source>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.values()
    }
}

fn entry<'index, 'source>(
    clients: &'index mut BTreeMap<MacAddress, ConnectedClient<'source>>,
    mac: MacAddress,
) -> &'index mut ConnectedClient<'source> {
    clients.entry(mac).or_insert_with(|| ConnectedClient {
        mac,
        association: None,
        hotspot: None,
        arp: Vec::new(),
        dhcp: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::{ClientActivity, ClientIndex};
    use crate::model::{
        hotspot::IpHotspotHosts,
        ip::{IpArp, IpDhcpBindings},
        wifi::Associations,
    };

    #[test]
    fn joins_union_without_losing_multiple_records() {
        let associations: Associations =
            serde_json::from_str(include_str!("../../tests/fixtures/show_associations.json"))
                .unwrap();
        let hotspot: IpHotspotHosts =
            serde_json::from_str(include_str!("../../tests/fixtures/show_ip_hotspot.json"))
                .unwrap();
        let arp: IpArp =
            serde_json::from_str(include_str!("../../tests/fixtures/show_ip_arp.json")).unwrap();
        let dhcp: IpDhcpBindings = serde_json::from_str(include_str!(
            "../../tests/fixtures/show_ip_dhcp_bindings.json"
        ))
        .unwrap();
        let index = ClientIndex::new(&associations, &hotspot, &arp, &dhcp);

        assert_eq!(index.len(), 6);
        let shared = index.get(&"02:11:22:33:44:55".parse().unwrap()).unwrap();
        assert_eq!(shared.activity(), ClientActivity::Active);
        assert_eq!(shared.arp().len(), 2);
        assert_eq!(shared.dhcp().len(), 2);
        assert!(
            shared
                .ipv4_addresses()
                .any(|address| address == Ipv4Addr::new(192, 0, 2, 11))
        );
        assert_eq!(shared.interface_ids().count(), 5);

        let inactive = index.get(&"02:de:ad:be:ef:00".parse().unwrap()).unwrap();
        assert_eq!(inactive.activity(), ClientActivity::Inactive);
        let unknown = index.get(&"02:00:00:00:00:40".parse().unwrap()).unwrap();
        assert_eq!(unknown.activity(), ClientActivity::Unknown);
    }
}
