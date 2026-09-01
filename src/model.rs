//! Typed response models and validated domain values.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, de};
use time::{
    OffsetDateTime, PrimitiveDateTime, format_description::well_known::Rfc3339,
    macros::format_description,
};

use crate::model::mobile::MobileInterface;
use crate::model::network::InterfaceId;
use crate::model::system::Mtu;

macro_rules! open_string_enum {
    ($type:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        impl $type {
            /// Returns the value as represented by the router.
            #[must_use]
            pub fn as_str(&self) -> &str {
                match self {
                    $(Self::$variant => $wire,)+
                    Self::Other(value) => value,
                }
            }
        }

        impl From<Box<str>> for $type {
            fn from(value: Box<str>) -> Self {
                match value.as_ref() {
                    $($wire => Self::$variant,)+
                    _ => Self::Other(value),
                }
            }
        }

        impl From<&str> for $type {
            fn from(value: &str) -> Self {
                match value {
                    $($wire => Self::$variant,)+
                    _ => Self::Other(value.into()),
                }
            }
        }

        impl From<String> for $type {
            fn from(value: String) -> Self {
                Self::from(value.into_boxed_str())
            }
        }

        impl std::str::FromStr for $type {
            type Err = std::convert::Infallible;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Ok(Self::from(value))
            }
        }

        impl std::fmt::Display for $type {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl serde::Serialize for $type {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $type {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct Visitor;

                impl serde::de::Visitor<'_> for Visitor {
                    type Value = $type;

                    fn expecting(
                        &self,
                        formatter: &mut std::fmt::Formatter<'_>,
                    ) -> std::fmt::Result {
                        formatter.write_str("a router-defined string classifier")
                    }

                    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
                    where
                        E: serde::de::Error,
                    {
                        Ok($type::from(value))
                    }

                    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
                    where
                        E: serde::de::Error,
                    {
                        Ok($type::from(value))
                    }
                }

                deserializer.deserialize_string(Visitor)
            }
        }
    };
}

macro_rules! impl_slice_collection {
    ($collection:ty, $item:ty, $field:tt, $accessor:ident) => {
        impl std::ops::Deref for $collection {
            type Target = [$item];

            fn deref(&self) -> &Self::Target {
                self.$accessor()
            }
        }

        impl IntoIterator for $collection {
            type Item = $item;
            type IntoIter = std::vec::IntoIter<$item>;

            fn into_iter(self) -> Self::IntoIter {
                self.$field.into_vec().into_iter()
            }
        }

        impl<'a> IntoIterator for &'a $collection {
            type Item = &'a $item;
            type IntoIter = std::slice::Iter<'a, $item>;

            fn into_iter(self) -> Self::IntoIter {
                self.$field.iter()
            }
        }
    };
}

macro_rules! string_identifier {
    ($type:ident, $error:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $type(Box<str>);

        impl $type {
            /// Parses and validates an identifier.
            ///
            /// # Errors
            ///
            #[doc = concat!("Returns [`", stringify!($error), "`] for an empty value or a control character.")]
            pub fn new(value: impl Into<String>) -> Result<Self, $error> {
                Self::try_from(value.into())
            }

            /// Returns the identifier exactly as reported by the router.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<Box<str>> for $type {
            type Error = $error;

            fn try_from(value: Box<str>) -> Result<Self, Self::Error> {
                if value.is_empty() {
                    return Err($error::Empty);
                }
                if value.chars().any(char::is_control) {
                    return Err($error::ControlCharacter);
                }
                Ok(Self(value))
            }
        }

        impl TryFrom<String> for $type {
            type Error = $error;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::try_from(value.into_boxed_str())
            }
        }

        impl std::str::FromStr for $type {
            type Err = $error;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::try_from(Box::<str>::from(value))
            }
        }

        impl AsRef<str> for $type {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl std::borrow::Borrow<str> for $type {
            fn borrow(&self) -> &str {
                self.as_str()
            }
        }

        impl std::ops::Deref for $type {
            type Target = str;

            fn deref(&self) -> &Self::Target {
                self.as_str()
            }
        }

        impl std::fmt::Display for $type {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl serde::Serialize for $type {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> serde::Deserialize<'de> for $type {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                <Box<str> as serde::Deserialize>::deserialize(deserializer)?
                    .try_into()
                    .map_err(serde::de::Error::custom)
            }
        }
    };
}

macro_rules! impl_string_value {
    ($type:ident, $error:ty, $expected:literal) => {
        impl TryFrom<&str> for $type {
            type Error = $error;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                value.parse()
            }
        }

        impl AsRef<str> for $type {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl serde::Serialize for $type {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> serde::Deserialize<'de> for $type {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                deserializer
                    .deserialize_str($crate::model::text::FromStrVisitor::<Self>::new($expected))
            }
        }

        impl std::fmt::Display for $type {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl std::fmt::Debug for $type {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter
                    .debug_tuple(stringify!($type))
                    .field(&self.as_str())
                    .finish()
            }
        }
    };
}

macro_rules! impl_map_collection {
    ($collection:ident, $key:ty, $value:ty, $field:tt, $item:literal, $map:literal) => {
        impl $collection {
            #[doc = concat!("Returns ", $item, " by its response key.")]
            #[must_use]
            pub fn get(&self, key: &str) -> Option<&$value> {
                self.$field.get(key)
            }

            #[doc = concat!("Consumes the response and returns ", $map, ".")]
            #[must_use]
            pub fn into_inner(self) -> std::collections::BTreeMap<$key, $value> {
                self.$field
            }
        }

        impl std::ops::Deref for $collection {
            type Target = std::collections::BTreeMap<$key, $value>;

            fn deref(&self) -> &Self::Target {
                &self.$field
            }
        }

        impl IntoIterator for $collection {
            type Item = ($key, $value);
            type IntoIter = std::collections::btree_map::IntoIter<$key, $value>;

            fn into_iter(self) -> Self::IntoIter {
                self.$field.into_iter()
            }
        }

        impl<'a> IntoIterator for &'a $collection {
            type Item = (&'a $key, &'a $value);
            type IntoIter = std::collections::btree_map::Iter<'a, $key, $value>;

            fn into_iter(self) -> Self::IntoIter {
                self.$field.iter()
            }
        }
    };
}

pub use version::{Version, VersionBuild, VersionCapabilities};

pub mod clients;
pub mod connectivity;
pub mod hardware_id;
pub mod hotspot;
pub mod iccid;
pub mod identification;
pub mod imei;
pub mod imsi;
pub mod interface_stat;
pub mod ip;
pub mod mesh;
pub mod mobile;
pub mod network;
pub mod plmn;
pub mod reported;
pub mod routing;
pub mod storage;
pub mod system;
pub mod system_mode;
mod text;
pub mod units;
pub mod version;
pub mod wifi;

const ROUTER_DATETIME_FORMAT: &[time::format_description::FormatItem<'static>] = format_description!(
    "[weekday repr:short] [month repr:short] [day padding:space] [hour]:[minute]:[second] [year]"
);

/// Link state reported for an interface.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum LinkState {
    /// The interface link is up.
    Up,
    /// The interface link is down.
    Down,
    /// A link state introduced by a different firmware version.
    Other(Box<str>),
}

/// Administrative state reported for an interface.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum InterfaceState {
    /// The interface is administratively enabled.
    Up,
    /// The interface is administratively disabled.
    Down,
    /// A state introduced by a different firmware version.
    Other(Box<str>),
}

/// Runtime state reported for an interface layer.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum InterfaceLayerState {
    /// The layer is running.
    Running,
    /// The layer is waiting for a prerequisite or activation.
    Pending,
    /// The layer is disabled.
    Disabled,
    /// A state introduced by a different firmware version.
    Other(Box<str>),
}

/// Connectivity summary returned by `show/internet/status`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
#[allow(clippy::struct_excessive_bools)]
pub struct InternetStatus {
    /// Whether internet checking is enabled.
    pub enabled: bool,
    /// Whether the router considers internet access available.
    pub internet: bool,
    /// Whether the latest result is considered reliable.
    pub reliable: bool,
    /// Local time of the last check as reported by the router.
    ///
    /// The RCI response does not include a UTC offset.
    #[serde(deserialize_with = "deserialize_router_datetime")]
    pub checked: PrimitiveDateTime,
    /// Whether the gateway check succeeded.
    #[serde(rename = "gateway-accessible")]
    pub gateway_accessible: bool,
    /// Whether the DNS check succeeded.
    #[serde(rename = "dns-accessible")]
    pub dns_accessible: bool,
    /// Whether the captive portal check succeeded.
    #[serde(rename = "captive-accessible")]
    pub captive_accessible: bool,
}

/// Interfaces returned by `show/interface`, keyed by router interface name.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(transparent)]
pub struct Interfaces(BTreeMap<InterfaceId, Interface>);

impl Interfaces {
    /// Iterates over response keys and interfaces in lexical order.
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &Interface)> {
        self.0
            .iter()
            .map(|(name, interface)| (name.as_str(), interface))
    }

    /// Iterates over interfaces carrying a verified mobile/LTE trait.
    pub fn lte(&self) -> impl Iterator<Item = (&str, &Interface)> {
        self.iter()
            .filter(|(_, interface)| interface.is_mobile_broadband())
    }
}

fn deserialize_router_datetime<'de, D>(deserializer: D) -> Result<PrimitiveDateTime, D::Error>
where
    D: Deserializer<'de>,
{
    struct Visitor;

    impl de::Visitor<'_> for Visitor {
        type Value = PrimitiveDateTime;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a router date and time")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            PrimitiveDateTime::parse(value, ROUTER_DATETIME_FORMAT)
                .or_else(|_| {
                    OffsetDateTime::parse(value, &Rfc3339)
                        .map(|value| PrimitiveDateTime::new(value.date(), value.time()))
                })
                .map_err(E::custom)
        }
    }

    deserializer.deserialize_str(Visitor)
}

impl_map_collection!(
    Interfaces,
    InterfaceId,
    Interface,
    0,
    "an interface",
    "the interface map"
);

/// Reply returned by a typed LTE `show interface name ...` command.
pub type ShowLteInterfaceReply = ShowInterfaceReply<MobileInterface>;

/// Nested result of a typed LTE `show interface name ...` command.
pub type ShowLteInterfaceResult = ShowInterfaceResult<MobileInterface>;

/// A minimal interface view shared by the verified interface response shapes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct Interface {
    /// Stable router interface identifier.
    pub id: InterfaceId,
    /// Numeric interface index.
    pub index: u64,
    /// Interface name returned in the response body.
    #[serde(rename = "interface-name")]
    pub interface_name: InterfaceId,
    /// Router-defined interface kind.
    #[serde(rename = "type")]
    pub kind: InterfaceKind,
    /// Router-defined interface traits.
    pub traits: Box<[InterfaceTrait]>,
    /// Link state as reported by the router.
    pub link: LinkState,
    /// Whether the interface is only visible to administrators.
    #[serde(rename = "admin-only")]
    pub admin_only: bool,
    /// Layer summary.
    pub summary: InterfaceSummary,
    /// User-facing description when the interface kind supplies one.
    pub description: Option<Box<str>>,
    /// Administrative state when supplied by the interface kind.
    pub state: Option<InterfaceState>,
    /// MTU when supplied by the interface kind.
    pub mtu: Option<Mtu>,
}

impl Interface {
    /// Reports whether the router marked this as a mobile/LTE interface.
    ///
    /// This uses the `Mobile`, `UsbLte`, and `UsbQmi` traits observed in live
    /// `show/interface` responses rather than guessing from the interface name.
    #[must_use]
    pub fn is_mobile_broadband(&self) -> bool {
        self.traits.iter().any(|trait_| {
            matches!(
                trait_,
                InterfaceTrait::Mobile | InterfaceTrait::UsbLte | InterfaceTrait::UsbQmi
            )
        })
    }
}

/// Interface summary container.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct InterfaceSummary {
    /// Per-layer status values.
    pub layer: InterfaceLayerSummary,
}

/// Per-layer states reported for an interface.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct InterfaceLayerSummary {
    /// Configuration-layer status.
    pub conf: InterfaceLayerState,
    /// Link-layer status, when applicable.
    pub link: Option<InterfaceLayerState>,
    /// Control-layer status, when applicable.
    pub ctrl: Option<InterfaceLayerState>,
    /// IPv4-layer status, when applicable.
    pub ipv4: Option<InterfaceLayerState>,
    /// IPv6-layer status, when applicable.
    pub ipv6: Option<InterfaceLayerState>,
}

/// Reply returned by the JSON form of `show interface name ...`.
///
/// The interface payload defaults to [`Interface`]. [`ShowLteInterfaceReply`]
/// specializes it to the verified mobile interface representation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct ShowInterfaceReply<T = Interface> {
    /// Command result.
    pub show: ShowInterfaceResult<T>,
}

impl<T> ShowInterfaceReply<T> {
    /// Returns the requested interface.
    #[must_use]
    pub const fn interface(&self) -> &T {
        &self.show.interface
    }

    /// Consumes the reply and returns the requested interface.
    #[must_use]
    pub fn into_interface(self) -> T {
        self.show.interface
    }
}

/// Nested result of the `show interface` JSON command.
///
/// The payload type defaults to [`Interface`].
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct ShowInterfaceResult<T = Interface> {
    /// The requested interface.
    pub interface: T,
}

fn optional_nonempty_string<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: AsRef<str> + Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
        .map(|value| value.filter(|text| !text.as_ref().is_empty()))
}

fn optional_f32<'de, D>(deserializer: D) -> Result<Option<f32>, D::Error>
where
    D: Deserializer<'de>,
{
    struct OptionalF32Visitor;

    impl de::Visitor<'_> for OptionalF32Visitor {
        type Value = Option<f32>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a number, a numeric string, an empty string, or null")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        #[allow(clippy::cast_possible_truncation)]
        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E> {
            Ok(Some(value as f32))
        }

        #[allow(clippy::cast_precision_loss)]
        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            i32::try_from(value)
                .map(|value| value as f32)
                .map(Some)
                .map_err(de::Error::custom)
        }

        #[allow(clippy::cast_precision_loss)]
        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            u32::try_from(value)
                .map(|value| value as f32)
                .map(Some)
                .map_err(de::Error::custom)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if value.is_empty() {
                Ok(None)
            } else {
                value.parse().map(Some).map_err(de::Error::custom)
            }
        }
    }

    deserializer.deserialize_any(OptionalF32Visitor)
}

open_string_enum!(LinkState {
    Up => "up",
    Down => "down",
});

open_string_enum!(InterfaceState {
    Up => "up",
    Down => "down",
});

open_string_enum!(InterfaceLayerState {
    Running => "running",
    Pending => "pending",
    Disabled => "disabled",
});

/// Router-defined interface kind.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum InterfaceKind {
    /// A network bridge.
    Bridge,
    /// A generic port.
    Port,
    /// A Gigabit Ethernet interface.
    GigabitEthernet,
    /// A Wi-Fi interface.
    Wifi,
    /// A USB LTE interface.
    UsbLte,
    /// A USB QMI interface.
    UsbQmi,
    /// An interface kind introduced by a different firmware version.
    Other(Box<str>),
}

open_string_enum!(InterfaceKind {
    Bridge => "Bridge",
    Port => "Port",
    GigabitEthernet => "GigabitEthernet",
    Wifi => "Wifi",
    UsbLte => "UsbLte",
    UsbQmi => "UsbQmi",
});

/// A capability trait attached to an interface by the router.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum InterfaceTrait {
    /// Mobile broadband capability.
    Mobile,
    /// USB LTE capability.
    UsbLte,
    /// USB QMI capability.
    UsbQmi,
    /// IPv4 capability.
    Ip,
    /// IPv6 capability.
    Ip6,
    /// A trait introduced by a different firmware version.
    Other(Box<str>),
}

open_string_enum!(InterfaceTrait {
    Mobile => "Mobile",
    UsbLte => "UsbLte",
    UsbQmi => "UsbQmi",
    Ip => "Ip",
    Ip6 => "Ip6",
});

#[cfg(test)]
mod tests {
    use super::{
        InterfaceKind, InterfaceLayerState, InterfaceLayerSummary, InterfaceState, InterfaceTrait,
        Interfaces, InternetStatus, LinkState, ShowInterfaceReply, Version,
        connectivity::{IpNameServers, NtpStatus, PingCheckProfiles},
        hotspot::IpHotspotHosts,
        identification::Identification,
        interface_stat::InterfaceStat,
        ip::{IpArp, IpDhcpBindings},
        mesh::{MwsMembers, MwsStatus},
        mobile::{MobileSignal, MobileStatus},
        reported::Reported,
        routing::{IpRoutes, Ipv6Routes},
        storage::{MediaInventory, UsbDevices},
        system::System,
        system_mode::SystemModeStatus,
        wifi::Associations,
    };

    #[test]
    fn deserializes_verified_fixtures() {
        serde_json::from_str::<Version>(include_str!("../tests/fixtures/show_version.json"))
            .unwrap();
        serde_json::from_str::<System>(include_str!("../tests/fixtures/show_system.json")).unwrap();
        let internet_status = serde_json::from_str::<InternetStatus>(include_str!(
            "../tests/fixtures/show_internet_status.json"
        ))
        .unwrap();
        assert_eq!(
            internet_status.checked,
            time::macros::datetime!(2026-01-01 0:00)
        );
        serde_json::from_str::<Interfaces>(include_str!("../tests/fixtures/show_interfaces.json"))
            .unwrap();
        serde_json::from_str::<ShowInterfaceReply>(include_str!(
            "../tests/fixtures/show_interface.json"
        ))
        .unwrap();
        serde_json::from_str::<Associations>(include_str!(
            "../tests/fixtures/show_associations.json"
        ))
        .unwrap();
        serde_json::from_str::<IpHotspotHosts>(include_str!(
            "../tests/fixtures/show_ip_hotspot.json"
        ))
        .unwrap();
        serde_json::from_str::<InterfaceStat>(include_str!(
            "../tests/fixtures/show_interface_stat.json"
        ))
        .unwrap();
        serde_json::from_str::<Identification>(include_str!(
            "../tests/fixtures/show_identification.json"
        ))
        .unwrap();
        serde_json::from_str::<SystemModeStatus>(include_str!(
            "../tests/fixtures/show_system_mode.json"
        ))
        .unwrap();
        serde_json::from_str::<IpArp>(include_str!("../tests/fixtures/show_ip_arp.json")).unwrap();
        serde_json::from_str::<IpDhcpBindings>(include_str!(
            "../tests/fixtures/show_ip_dhcp_bindings.json"
        ))
        .unwrap();
        serde_json::from_str::<IpRoutes>(include_str!("../tests/fixtures/show_ip_route.json"))
            .unwrap();
        serde_json::from_str::<Ipv6Routes>(include_str!("../tests/fixtures/show_ipv6_route.json"))
            .unwrap();
        serde_json::from_str::<PingCheckProfiles>(include_str!(
            "../tests/fixtures/show_ping_check.json"
        ))
        .unwrap();
        serde_json::from_str::<IpNameServers>(include_str!(
            "../tests/fixtures/show_ip_name_server.json"
        ))
        .unwrap();
        serde_json::from_str::<NtpStatus>(include_str!("../tests/fixtures/show_ntp_status.json"))
            .unwrap();
        serde_json::from_str::<MwsStatus>(include_str!("../tests/fixtures/show_mws_status.json"))
            .unwrap();
        serde_json::from_str::<MwsMembers>(include_str!("../tests/fixtures/show_mws_member.json"))
            .unwrap();
        serde_json::from_str::<UsbDevices>(include_str!("../tests/fixtures/show_usb.json"))
            .unwrap();
        serde_json::from_str::<MediaInventory>(include_str!("../tests/fixtures/show_media.json"))
            .unwrap();
    }

    #[test]
    fn deserializes_rfc3339_internet_check_time_from_owned_json() {
        let mut fixture: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/show_internet_status.json"))
                .unwrap();
        fixture["checked"] = "2026-01-01T03:00:00+03:00".into();

        let internet_status = serde_json::from_value::<InternetStatus>(fixture).unwrap();

        assert_eq!(
            internet_status.checked,
            time::macros::datetime!(2026-01-01 3:00)
        );
    }

    #[test]
    fn parses_known_and_preserves_unknown_interface_classifiers() {
        let interfaces: Interfaces =
            serde_json::from_str(include_str!("../tests/fixtures/show_interfaces.json")).unwrap();

        let bridge = interfaces.get("Bridge0").unwrap();
        assert_eq!(bridge.kind, InterfaceKind::Bridge);
        assert_eq!(bridge.link, LinkState::Up);
        assert_eq!(bridge.state, Some(InterfaceState::Up));
        assert_eq!(
            bridge.traits.as_ref(),
            [InterfaceTrait::Other("Ethernet".into()), InterfaceTrait::Ip,]
        );

        let access_point = interfaces.get("WifiMaster0/AccessPoint0").unwrap();
        assert_eq!(
            access_point.kind,
            InterfaceKind::Other("AccessPoint".into())
        );
        assert_eq!(access_point.kind.as_str(), "AccessPoint");

        let state: LinkState = serde_json::from_str(r#""pending""#).unwrap();
        assert_eq!(state, LinkState::Other("pending".into()));
        assert_eq!(state.as_str(), "pending");

        let state: InterfaceState = serde_json::from_str(r#""blocked""#).unwrap();
        assert_eq!(state, InterfaceState::Other("blocked".into()));
        assert_eq!(state.as_str(), "blocked");
    }

    #[test]
    fn parses_interface_layer_states_and_optional_layers() {
        let summary: InterfaceLayerSummary = serde_json::from_value(serde_json::json!({
            "conf": "running",
            "ipv4": "disabled",
            "ipv6": "future-state"
        }))
        .unwrap();

        assert_eq!(summary.conf, InterfaceLayerState::Running);
        assert_eq!(summary.link, None);
        assert_eq!(summary.ctrl, None);
        assert_eq!(summary.ipv4, Some(InterfaceLayerState::Disabled));
        assert_eq!(
            summary.ipv6,
            Some(InterfaceLayerState::Other("future-state".into()))
        );

        let pending: InterfaceLayerState = serde_json::from_str(r#""pending""#).unwrap();
        assert_eq!(pending, InterfaceLayerState::Pending);
        assert_eq!(pending.as_str(), "pending");
    }

    #[test]
    fn deserializes_active_and_disconnected_lte_fixtures() {
        use super::ShowLteInterfaceReply;

        let active: ShowLteInterfaceReply =
            serde_json::from_str(include_str!("../tests/fixtures/show_lte_interface.json"))
                .unwrap();
        let lte = active.interface();
        assert_eq!(lte.interface_name.as_str(), "UsbLte1");
        assert_eq!(
            lte.status().signal.rsrp.map(super::units::Dbm::get),
            Some(-79.0)
        );
        assert_eq!(
            lte.status().primary_carrier.band,
            Some(super::mobile::RadioBand::Number(7))
        );
        assert_eq!(
            lte.status().primary_carrier.network,
            Some(super::mobile::RadioAccessTechnology::G4)
        );
        assert_eq!(
            lte.status().reported_carriers[&super::mobile::ComponentCarrierId::PRIMARY].band,
            Some(super::mobile::RadioBand::Number(7))
        );
        assert_eq!(lte.status().cell.enb_id, Some(100_001));
        assert_eq!(lte.status().cell.sector_id, Some(1));
        assert_eq!(
            lte.status()
                .cell
                .plmn
                .as_ref()
                .map(super::plmn::Plmn::as_str),
            Some("00101")
        );
        assert_eq!(
            lte.status()
                .modem_temperature
                .map(super::units::Celsius::get),
            Some(42.0)
        );
        assert_eq!(
            lte.status().imei.as_ref().map(Reported::as_str),
            Some("000000000000000")
        );
        assert_eq!(
            lte.status().imsi.as_ref().map(Reported::as_str),
            Some("001010000000001")
        );
        assert_eq!(
            lte.status().iccid.as_ref().map(Reported::as_str),
            Some("8901000000000000001")
        );
        assert_eq!(lte.status().phone_number.as_deref(), Some("+10000000000"));
        assert_eq!(
            lte.status()
                .modem
                .as_ref()
                .and_then(|modem| modem.model.as_deref()),
            Some("FM-1000")
        );

        let mut active_value: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/show_lte_interface.json"))
                .unwrap();
        active_value["show"]["interface"]["active"] = true.into();
        let active_from_value: ShowLteInterfaceReply =
            serde_json::from_value(active_value).unwrap();
        assert_eq!(
            active_from_value
                .interface()
                .status()
                .primary_carrier
                .active,
            Some(true)
        );

        let disconnected: ShowLteInterfaceReply = serde_json::from_str(include_str!(
            "../tests/fixtures/show_lte_interface_disconnected.json"
        ))
        .unwrap();
        let status = disconnected.interface().status();
        assert_eq!(status.connection_state, None);
        assert_eq!(status.operator, None);
        assert_eq!(status.apn, None);
        assert_eq!(status.primary_carrier.network, None);
        assert_eq!(status.primary_carrier.band, None);
        assert_eq!(status.signal.rssi, None);
        assert_eq!(status.modem_temperature, None);
        assert_eq!(status.imei, None);
        assert_eq!(status.imsi, None);
        assert_eq!(status.iccid, None);
        assert_eq!(status.phone_number, None);
        assert!(status.reported_carriers.is_empty());
        assert_eq!(
            status
                .sim
                .as_ref()
                .and_then(|sim| sim.service_provider.as_deref()),
            None
        );
    }

    #[test]
    fn parses_numeric_lte_measurements_at_the_wire_boundary() {
        let signal: MobileSignal = serde_json::from_value(serde_json::json!({
            "rssi": "-51.5",
            "rsrp": -79,
            "rsrq": "",
            "cinr": null
        }))
        .unwrap();

        assert_eq!(signal.rssi.map(super::units::Dbm::get), Some(-51.5));
        assert_eq!(signal.rsrp.map(super::units::Dbm::get), Some(-79.0));
        assert_eq!(signal.rsrq, None);
        assert_eq!(signal.cinr, None);

        let error = serde_json::from_str::<MobileSignal>(r#"{"rssi":"unknown"}"#).unwrap_err();
        assert!(error.to_string().contains("invalid float literal"));
    }

    #[test]
    fn preserves_unrecognized_modem_identifiers() {
        let status: MobileStatus = serde_json::from_value(serde_json::json!({
            "imei": "vendor-specific-imei",
            "imsi": "vendor-specific-imsi",
            "iccid": "890100000000000001"
        }))
        .unwrap();

        assert!(matches!(status.imei, Some(Reported::Unrecognized(_))));
        assert!(matches!(status.imsi, Some(Reported::Unrecognized(_))));
        assert!(matches!(status.iccid, Some(Reported::Unrecognized(_))));
    }

    #[test]
    fn lte_response_rejects_a_non_mobile_interface() {
        use super::ShowLteInterfaceReply;

        let error = serde_json::from_str::<ShowLteInterfaceReply>(include_str!(
            "../tests/fixtures/show_interface.json"
        ))
        .unwrap_err();
        assert!(error.to_string().contains("not marked"));
    }

    #[test]
    fn discovers_lte_interfaces_by_traits() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/show_lte_interface_disconnected.json"
        ))
        .unwrap();
        let interfaces: Interfaces = serde_json::from_value(serde_json::json!({
            "UsbLte0": fixture["show"]["interface"].clone(),
            "Bridge0": serde_json::from_str::<serde_json::Value>(include_str!(
                "../tests/fixtures/show_interfaces.json"
            ))
            .unwrap()["Bridge0"]
                .clone()
        }))
        .unwrap();

        let names: Vec<_> = interfaces.lte().map(|(name, _)| name).collect();
        assert_eq!(names, ["UsbLte0"]);
    }

    #[test]
    fn variable_and_unknown_fields_follow_model_contract() {
        let mut fixture: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/show_interfaces.json")).unwrap();
        let interface = fixture["Bridge0"].as_object_mut().unwrap();
        interface.remove("description");
        interface.insert("future-field".into(), serde_json::json!(true));

        let parsed: Interfaces = serde_json::from_value(fixture).unwrap();
        assert_eq!(parsed["Bridge0"].description, None);
    }

    #[test]
    fn missing_required_field_is_rejected() {
        let mut fixture: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/show_version.json")).unwrap();
        fixture.as_object_mut().unwrap().remove("hw_id");
        let error = serde_json::from_value::<Version>(fixture).unwrap_err();
        assert!(error.to_string().contains("hw_id"));
    }
}
