use std::{
    fmt,
    net::{IpAddr, Ipv4Addr},
    num::NonZeroU8,
    str::FromStr,
    time::Duration,
};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use thiserror::Error;

/// A malformed MAC address.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum ParseMacAddressError {
    /// The textual representation did not contain exactly six octets.
    #[error("a MAC address must contain 17 ASCII characters, got {0}")]
    InvalidLength(usize),
    /// An octet separator was not a colon.
    #[error("a MAC address must contain ':' at byte {0}")]
    InvalidSeparator(usize),
    /// An octet contained a non-hexadecimal digit.
    #[error("a MAC address contains an invalid hexadecimal digit at byte {0}")]
    InvalidHexDigit(usize),
}

/// An invalid router interface identifier.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum InvalidInterfaceId {
    /// The identifier was empty.
    #[error("an interface identifier must not be empty")]
    Empty,
    /// The identifier contained a control character.
    #[error("an interface identifier must not contain control characters")]
    ControlCharacter,
}

/// An IP address or an opaque host name reported by the router.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum NetworkHost {
    /// A parsed IPv4 or IPv6 address.
    Address(IpAddr),
    /// A non-empty router-reported name.
    Name(Box<str>),
}

impl NetworkHost {
    /// Parses an address or validates an opaque host name.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidNetworkHost`] when the value is empty or contains a
    /// control character.
    pub fn new(value: impl AsRef<str>) -> Result<Self, InvalidNetworkHost> {
        value.as_ref().parse()
    }

    /// Returns the parsed address, when the wire value was an IP address.
    #[must_use]
    pub const fn address(&self) -> Option<IpAddr> {
        match self {
            Self::Address(address) => Some(*address),
            Self::Name(_) => None,
        }
    }

    /// Returns the opaque host name, when the wire value was not an IP address.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Address(_) => None,
            Self::Name(name) => Some(name),
        }
    }
}

impl TryFrom<Box<str>> for NetworkHost {
    type Error = InvalidNetworkHost;

    fn try_from(value: Box<str>) -> Result<Self, Self::Error> {
        validate_network_host(&value)?;
        Ok(value
            .parse()
            .map_or_else(|_| Self::Name(value), Self::Address))
    }
}

impl TryFrom<String> for NetworkHost {
    type Error = InvalidNetworkHost;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_from(value.into_boxed_str())
    }
}

impl From<IpAddr> for NetworkHost {
    fn from(address: IpAddr) -> Self {
        Self::Address(address)
    }
}

impl FromStr for NetworkHost {
    type Err = InvalidNetworkHost;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        validate_network_host(value)?;
        Ok(value
            .parse()
            .map_or_else(|_| Self::Name(Box::from(value)), Self::Address))
    }
}

impl Serialize for NetworkHost {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for NetworkHost {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl de::Visitor<'_> for Visitor {
            type Value = NetworkHost;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an IP address or a non-empty opaque host name")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                value.parse().map_err(E::custom)
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                value.try_into().map_err(E::custom)
            }
        }

        deserializer.deserialize_string(Visitor)
    }
}

impl fmt::Display for NetworkHost {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Address(address) => address.fmt(formatter),
            Self::Name(name) => formatter.write_str(name),
        }
    }
}

/// An invalid host value.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum InvalidNetworkHost {
    /// The host was empty.
    #[error("a network host must not be empty")]
    Empty,
    /// The host contained a control character.
    #[error("a network host must not contain control characters")]
    ControlCharacter,
}

/// Remaining DHCP lease lifetime.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LeaseExpiration {
    /// The lease expires after the duration.
    Finite(Duration),
    /// The lease has no expiry.
    Infinite,
}

impl Serialize for LeaseExpiration {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Finite(duration) => serializer.serialize_u64(duration.as_secs()),
            Self::Infinite => serializer.serialize_str("infinity"),
        }
    }
}

impl<'de> Deserialize<'de> for LeaseExpiration {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl de::Visitor<'_> for Visitor {
            type Value = LeaseExpiration;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("whole lease seconds or the string `infinity`")
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
                Ok(LeaseExpiration::Finite(Duration::from_secs(value)))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                u64::try_from(value)
                    .map(Duration::from_secs)
                    .map(LeaseExpiration::Finite)
                    .map_err(E::custom)
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if value == "infinity" {
                    Ok(LeaseExpiration::Infinite)
                } else {
                    Err(E::invalid_value(de::Unexpected::Str(value), &self))
                }
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

/// A six-octet IEEE 802 MAC address.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MacAddress([u8; 6]);

impl MacAddress {
    /// Creates an address from its six octets.
    #[must_use]
    pub const fn from_octets(octets: [u8; 6]) -> Self {
        Self(octets)
    }

    /// Returns the address octets.
    #[must_use]
    pub const fn octets(self) -> [u8; 6] {
        self.0
    }
}

impl From<[u8; 6]> for MacAddress {
    fn from(octets: [u8; 6]) -> Self {
        Self::from_octets(octets)
    }
}

impl FromStr for MacAddress {
    type Err = ParseMacAddressError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != 17 {
            return Err(ParseMacAddressError::InvalidLength(value.len()));
        }

        let bytes = value.as_bytes();
        let mut octets = [0_u8; 6];
        for (index, octet) in octets.iter_mut().enumerate() {
            let offset = index * 3;
            if index != 5 && bytes[offset + 2] != b':' {
                return Err(ParseMacAddressError::InvalidSeparator(offset + 2));
            }
            let high =
                decode_hex(bytes[offset]).ok_or(ParseMacAddressError::InvalidHexDigit(offset))?;
            let low = decode_hex(bytes[offset + 1])
                .ok_or(ParseMacAddressError::InvalidHexDigit(offset + 1))?;
            *octet = (high << 4) | low;
        }
        Ok(Self(octets))
    }
}

impl Serialize for MacAddress {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for MacAddress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl de::Visitor<'_> for Visitor {
            type Value = MacAddress;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a colon-separated six-octet MAC address")
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

impl fmt::Display for MacAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:02x}", self.0[0])?;
        for octet in &self.0[1..] {
            write!(formatter, ":{octet:02x}")?;
        }
        Ok(())
    }
}

string_identifier!(
    InterfaceId,
    InvalidInterfaceId,
    "A validated router interface identifier used in responses and requests."
);

/// Time elapsed since an entity became active.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Uptime(Duration);

impl Uptime {
    /// Creates an uptime from whole seconds.
    #[must_use]
    pub const fn from_secs(seconds: u64) -> Self {
        Self(Duration::from_secs(seconds))
    }

    /// Returns the duration.
    #[must_use]
    pub const fn get(self) -> Duration {
        self.0
    }
}

impl<'de> Deserialize<'de> for Uptime {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl de::Visitor<'_> for Visitor {
            type Value = Uptime;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("whole uptime seconds or `<days> days, HH:MM:SS`")
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
                Ok(Uptime::from_secs(value))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                u64::try_from(value)
                    .map(Uptime::from_secs)
                    .map_err(E::custom)
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                value
                    .parse::<u64>()
                    .ok()
                    .map(Uptime::from_secs)
                    .or_else(|| parse_human_uptime(value).map(Uptime))
                    .ok_or_else(|| E::invalid_value(de::Unexpected::Str(value), &self))
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

impl Serialize for Uptime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.0.as_secs())
    }
}

/// A cumulative byte counter.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[repr(transparent)]
#[serde(transparent)]
pub struct ByteCount(u64);

impl ByteCount {
    /// Creates a byte count.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the number of bytes.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl From<u64> for ByteCount {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

/// A normalized data rate in bits per second.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[repr(transparent)]
#[serde(transparent)]
pub struct DataRate(u64);

impl DataRate {
    /// Creates a rate from bits per second.
    #[must_use]
    pub const fn from_bits_per_second(value: u64) -> Self {
        Self(value)
    }

    /// Creates a rate from kilobits per second.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidDataRate`] when normalization overflows.
    pub const fn from_kilobits_per_second(value: u64) -> Result<Self, InvalidDataRate> {
        match value.checked_mul(1_000) {
            Some(value) => Ok(Self(value)),
            None => Err(InvalidDataRate),
        }
    }

    /// Creates a rate from megabits per second.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidDataRate`] when normalization overflows.
    pub const fn from_megabits_per_second(value: u64) -> Result<Self, InvalidDataRate> {
        match value.checked_mul(1_000_000) {
            Some(value) => Ok(Self(value)),
            None => Err(InvalidDataRate),
        }
    }

    /// Returns bits per second.
    #[must_use]
    pub const fn bits_per_second(self) -> u64 {
        self.0
    }
}

/// A data rate that cannot be normalized into `u64` bits per second.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("the normalized data rate exceeds u64 bits per second")]
pub struct InvalidDataRate;

/// An IEEE 802.11 modulation and coding scheme index.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[repr(transparent)]
#[serde(transparent)]
pub struct McsIndex(u8);

impl McsIndex {
    /// Creates an MCS index.
    #[must_use]
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    /// Returns the index.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl From<u8> for McsIndex {
    fn from(value: u8) -> Self {
        Self::new(value)
    }
}

/// A Wi-Fi channel width in megahertz.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[repr(transparent)]
#[serde(transparent)]
pub struct ChannelWidth(u16);

impl ChannelWidth {
    /// Creates a channel width in megahertz.
    #[must_use]
    pub const fn from_megahertz(value: u16) -> Self {
        Self(value)
    }

    /// Returns megahertz.
    #[must_use]
    pub const fn megahertz(self) -> u16 {
        self.0
    }
}

/// A Wi-Fi guard interval in nanoseconds.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[repr(transparent)]
#[serde(transparent)]
pub struct GuardInterval(u16);

impl GuardInterval {
    /// Creates a guard interval in nanoseconds.
    #[must_use]
    pub const fn from_nanoseconds(value: u16) -> Self {
        Self(value)
    }

    /// Returns nanoseconds.
    #[must_use]
    pub const fn nanoseconds(self) -> u16 {
        self.0
    }
}

/// A non-zero number of Wi-Fi spatial streams.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[repr(transparent)]
#[serde(transparent)]
pub struct SpatialStreams(NonZeroU8);

impl SpatialStreams {
    /// Creates a non-zero spatial-stream count.
    #[must_use]
    pub const fn new(value: u8) -> Option<Self> {
        match NonZeroU8::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Returns the stream count.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0.get()
    }
}

struct OptionalU8Visitor;

impl<'de> de::Visitor<'de> for OptionalU8Visitor {
    type Value = Option<u8>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("null, an unsigned 8-bit integer, or a decimal integer string")
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(U8Visitor).map(Some)
    }
}

struct U8Visitor;

impl de::Visitor<'_> for U8Visitor {
    type Value = u8;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an unsigned 8-bit integer or a decimal integer string")
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        value.try_into().map_err(E::custom)
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
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

impl From<InterfaceId> for Box<str> {
    fn from(value: InterfaceId) -> Self {
        value.0
    }
}

const fn decode_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn validate_network_host(value: &str) -> Result<(), InvalidNetworkHost> {
    if value.is_empty() {
        return Err(InvalidNetworkHost::Empty);
    }
    if value.chars().any(char::is_control) {
        return Err(InvalidNetworkHost::ControlCharacter);
    }
    Ok(())
}

fn parse_human_uptime(value: &str) -> Option<Duration> {
    let (days, time) = value
        .split_once(" days, ")
        .or_else(|| value.split_once(" day, "))?;
    let mut parts = time.split(':');
    let hours = parts.next()?.parse::<u64>().ok()?;
    let minutes = parts.next()?.parse::<u64>().ok()?;
    let seconds = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() || hours >= 24 || minutes >= 60 || seconds >= 60 {
        return None;
    }
    let seconds = days
        .parse::<u64>()
        .ok()?
        .checked_mul(24)?
        .checked_add(hours)?
        .checked_mul(60)?
        .checked_add(minutes)?
        .checked_mul(60)?
        .checked_add(seconds)?;
    Some(Duration::from_secs(seconds))
}

pub(super) fn deserialize_mbps<'de, D>(deserializer: D) -> Result<DataRate, D::Error>
where
    D: Deserializer<'de>,
{
    DataRate::from_megabits_per_second(u64::deserialize(deserializer)?).map_err(de::Error::custom)
}

pub(super) fn deserialize_optional_mbps<'de, D>(
    deserializer: D,
) -> Result<Option<DataRate>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<u64>::deserialize(deserializer)?
        .map(DataRate::from_megabits_per_second)
        .transpose()
        .map_err(de::Error::custom)
}

pub(super) fn deserialize_kbps<'de, D>(deserializer: D) -> Result<DataRate, D::Error>
where
    D: Deserializer<'de>,
{
    DataRate::from_kilobits_per_second(u64::deserialize(deserializer)?).map_err(de::Error::custom)
}

pub(super) fn deserialize_optional_ipv4<'de, D>(
    deserializer: D,
) -> Result<Option<Ipv4Addr>, D::Error>
where
    D: Deserializer<'de>,
{
    let address = Ipv4Addr::deserialize(deserializer)?;
    Ok((!address.is_unspecified()).then_some(address))
}

pub(super) fn deserialize_optional_seconds<'de, D>(
    deserializer: D,
) -> Result<Option<Duration>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<u64>::deserialize(deserializer).map(|value| value.map(Duration::from_secs))
}

pub(super) fn deserialize_seconds<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    u64::deserialize(deserializer).map(Duration::from_secs)
}

pub(super) fn deserialize_u64_string_or_number<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    struct U64Visitor;

    impl de::Visitor<'_> for U64Visitor {
        type Value = u64;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a non-negative integer or decimal integer string")
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
            Ok(value)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            u64::try_from(value).map_err(E::custom)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            value.parse().map_err(E::custom)
        }
    }

    deserializer.deserialize_any(U64Visitor)
}

pub(super) fn deserialize_optional_u8_string_or_number<'de, D>(
    deserializer: D,
) -> Result<Option<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_option(OptionalU8Visitor)
}

pub(super) fn deserialize_decimal_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    struct Visitor;

    impl de::Visitor<'_> for Visitor {
        type Value = Duration;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("non-negative decimal seconds")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            parse_decimal_duration(value).map_err(E::custom)
        }
    }

    deserializer.deserialize_str(Visitor)
}

fn parse_decimal_duration(value: &str) -> Result<Duration, &'static str> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    let seconds = whole.parse::<u64>().map_err(|_| "invalid whole seconds")?;
    if fraction.len() > 9 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("invalid fractional seconds");
    }
    let nanos = if fraction.is_empty() {
        0
    } else {
        let parsed = fraction
            .parse::<u32>()
            .map_err(|_| "invalid fractional seconds")?;
        parsed * 10_u32.pow(u32::try_from(9 - fraction.len()).expect("fraction length is bounded"))
    };
    Ok(Duration::new(seconds, nanos))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        DataRate, InterfaceId, LeaseExpiration, MacAddress, NetworkHost, SpatialStreams, Uptime,
        parse_decimal_duration,
    };

    #[test]
    fn parses_and_normalizes_mac_addresses() {
        let lower: MacAddress = "aa:bb:0c:0d:ee:ff".parse().unwrap();
        let upper: MacAddress = "AA:BB:0C:0D:EE:FF".parse().unwrap();
        assert_eq!(lower, upper);
        assert_eq!(lower.to_string(), "aa:bb:0c:0d:ee:ff");
        assert!("aa-bb-cc-dd-ee-ff".parse::<MacAddress>().is_err());
    }

    #[test]
    fn validates_interface_identifiers() {
        let id: InterfaceId = "Bridge0".parse().unwrap();
        assert_eq!(serde_json::to_string(&id).unwrap(), r#""Bridge0""#);
        assert!(InterfaceId::new("").is_err());
        assert!(InterfaceId::new("Bridge\n0").is_err());
    }

    #[test]
    fn parses_addresses_and_validates_opaque_network_hosts() {
        let address: NetworkHost = "2001:db8::1".parse().unwrap();
        let name: NetworkHost = "time.example.invalid".parse().unwrap();

        assert_eq!(address.address().unwrap().to_string(), "2001:db8::1");
        assert_eq!(name.name(), Some("time.example.invalid"));
        assert_eq!(
            serde_json::to_string(&name).unwrap(),
            r#""time.example.invalid""#
        );
        assert!(NetworkHost::new("").is_err());
        assert!(NetworkHost::new("bad\nhost").is_err());
    }

    #[test]
    fn uptime_accepts_both_wire_forms() {
        let numeric: Uptime = serde_json::from_str("3723").unwrap();
        let human: Uptime = serde_json::from_str(r#""0 days, 01:02:03""#).unwrap();
        let numeric_string: Uptime = serde_json::from_str(r#""3723""#).unwrap();
        let singular: Uptime = serde_json::from_str(r#""1 day, 01:02:03""#).unwrap();
        assert_eq!(numeric, human);
        assert_eq!(numeric, numeric_string);
        assert_eq!(numeric.get(), Duration::from_secs(3_723));
        assert_eq!(singular.get(), Duration::from_secs(90_123));
        assert_eq!(serde_json::to_string(&numeric).unwrap(), "3723");
    }

    #[test]
    fn rates_normalize_and_check_overflow() {
        assert_eq!(
            DataRate::from_megabits_per_second(150)
                .unwrap()
                .bits_per_second(),
            150_000_000
        );
        assert!(DataRate::from_megabits_per_second(u64::MAX).is_err());
    }

    #[test]
    fn lease_expiry_accepts_seconds_and_infinity() {
        let finite: LeaseExpiration = serde_json::from_str("60").unwrap();
        let infinite: LeaseExpiration = serde_json::from_str(r#""infinity""#).unwrap();
        assert_eq!(finite, LeaseExpiration::Finite(Duration::from_secs(60)));
        assert_eq!(infinite, LeaseExpiration::Infinite);
        assert_eq!(serde_json::to_string(&finite).unwrap(), "60");
        assert_eq!(serde_json::to_string(&infinite).unwrap(), r#""infinity""#);
    }

    #[test]
    fn constrained_and_decimal_values_reject_invalid_input() {
        assert_eq!(SpatialStreams::new(1).map(SpatialStreams::get), Some(1));
        assert_eq!(SpatialStreams::new(0), None);
        assert_eq!(
            parse_decimal_duration("1.25").unwrap(),
            Duration::new(1, 250_000_000)
        );
        assert!(parse_decimal_duration("1.1234567890").is_err());
        assert!(parse_decimal_duration("-1").is_err());
    }
}
