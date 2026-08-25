use std::{
    fmt,
    net::{Ipv4Addr, Ipv6Addr},
    ops::Deref,
};

use ipnet::{Ipv4Net, Ipv6Net};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::model::network::InterfaceId;

/// IPv4 routes returned by `show/ip/route`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(transparent)]
pub struct IpRoutes(Box<[Ipv4Route]>);

impl IpRoutes {
    /// Returns the routes in router order.
    #[must_use]
    pub const fn routes(&self) -> &[Ipv4Route] {
        &self.0
    }

    /// Consumes the response and returns its routes.
    #[must_use]
    pub fn into_inner(self) -> Box<[Ipv4Route]> {
        self.0
    }
}

impl_slice_collection!(IpRoutes, Ipv4Route, 0, routes);

/// IPv6 routes returned by `show/ipv6/route`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ipv6Routes(Box<[Ipv6Route]>);

impl Ipv6Routes {
    /// Returns the routes in router order.
    #[must_use]
    pub const fn routes(&self) -> &[Ipv6Route] {
        &self.0
    }

    /// Consumes the response and returns its routes.
    #[must_use]
    pub fn into_inner(self) -> Box<[Ipv6Route]> {
        self.0
    }
}

impl<'de> Deserialize<'de> for Ipv6Routes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            #[serde(default)]
            route6: Box<[Ipv6Route]>,
        }

        Wire::deserialize(deserializer).map(|wire| Self(wire.route6))
    }
}

impl_slice_collection!(Ipv6Routes, Ipv6Route, 0, routes);

/// Routing protocol that installed an entry.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum RouteProtocol {
    /// Route installed during interface or system startup.
    Boot,
    /// Route installed directly by the kernel.
    Kernel,
    /// Route installed by a static-route provider.
    Static,
    /// A protocol introduced by another `KeeneticOS` release.
    Other(Box<str>),
}

/// One IPv4 routing-table entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct Ipv4Route {
    /// Destination network.
    pub destination: Ipv4Net,
    /// Next-hop address, or `None` for an on-link route.
    #[serde(deserialize_with = "optional_ipv4_gateway")]
    pub gateway: Option<Ipv4Addr>,
    /// Egress interface.
    pub interface: InterfaceId,
    /// Route metric.
    pub metric: u32,
    /// Compact flags reported by the routing stack.
    pub flags: RouteFlags,
    /// Whether this is a rejecting route.
    pub rejecting: bool,
    /// Origin of the route.
    pub proto: RouteProtocol,
    /// Whether the route is floating.
    pub floating: bool,
    /// Whether the route was statically configured.
    #[serde(rename = "static")]
    pub static_route: bool,
}

/// One IPv6 routing-table entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct Ipv6Route {
    /// Destination network.
    pub destination: Ipv6Net,
    /// Next-hop address, or `None` for an on-link route.
    #[serde(deserialize_with = "optional_ipv6_gateway")]
    pub gateway: Option<Ipv6Addr>,
    /// Egress interface.
    pub interface: InterfaceId,
    /// Route metric.
    pub metric: u32,
    /// Compact flags reported by the routing stack.
    pub flags: RouteFlags,
    /// Whether this is a rejecting route.
    pub rejecting: bool,
    /// Origin of the route.
    pub proto: RouteProtocol,
    /// Whether the route is floating.
    pub floating: bool,
    /// Whether the route was statically configured.
    #[serde(rename = "static")]
    pub static_route: bool,
}

/// Compact route flags preserved exactly as reported by the router.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
#[serde(transparent)]
pub struct RouteFlags(Box<str>);

impl RouteFlags {
    /// Returns the raw compact flag string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for RouteFlags {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Deref for RouteFlags {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl Serialize for RouteFlags {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl fmt::Display for RouteFlags {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

open_string_enum!(RouteProtocol {
    Boot => "boot",
    Kernel => "kernel",
    Static => "static",
});

fn optional_ipv4_gateway<'de, D>(deserializer: D) -> Result<Option<Ipv4Addr>, D::Error>
where
    D: Deserializer<'de>,
{
    Ipv4Addr::deserialize(deserializer)
        .map(|address| (!address.is_unspecified()).then_some(address))
}

fn optional_ipv6_gateway<'de, D>(deserializer: D) -> Result<Option<Ipv6Addr>, D::Error>
where
    D: Deserializer<'de>,
{
    Ipv6Addr::deserialize(deserializer)
        .map(|address| (!address.is_unspecified()).then_some(address))
}

#[cfg(test)]
mod tests {
    use super::{IpRoutes, Ipv6Routes, RouteProtocol};

    #[test]
    fn parses_both_route_families_and_normalizes_on_link_gateways() {
        let ipv4: IpRoutes =
            serde_json::from_str(include_str!("../../tests/fixtures/show_ip_route.json")).unwrap();
        let ipv6: Ipv6Routes =
            serde_json::from_str(include_str!("../../tests/fixtures/show_ipv6_route.json"))
                .unwrap();

        assert_eq!(ipv4.len(), 3);
        assert_eq!(ipv4[1].gateway, None);
        assert!(matches!(
            &ipv4[2].proto,
            RouteProtocol::Other(protocol) if protocol.as_ref() == "future-protocol"
        ));
        assert_eq!(ipv4[2].flags.as_str(), "UX");
        assert_eq!(ipv6.len(), 2);
        assert_eq!(ipv6[0].gateway, None);
    }

    #[test]
    fn empty_ipv6_object_is_an_empty_route_table() {
        assert!(serde_json::from_str::<Ipv6Routes>("{}").unwrap().is_empty());
        assert!(serde_json::from_str::<Ipv6Routes>(r#"{"unexpected": []}"#).is_err());
    }

    #[test]
    fn malformed_prefix_is_rejected_at_the_wire_boundary() {
        let fixture = include_str!("../../tests/fixtures/show_ip_route.json").replacen(
            "0.0.0.0/0",
            "192.0.2.0/99",
            1,
        );
        assert!(serde_json::from_str::<IpRoutes>(&fixture).is_err());
    }
}
