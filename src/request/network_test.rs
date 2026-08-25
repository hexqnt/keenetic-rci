use std::{
    fmt,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    num::{NonZeroU8, NonZeroU16, NonZeroU32, NonZeroU64},
};

use serde::{Serialize, Serializer, ser::SerializeMap};
use thiserror::Error;

use crate::{
    InvalidInterfaceId, InvalidNetworkHost,
    model::network::{InterfaceId, NetworkHost},
};

use super::{NetworkTestRequest, private};

const MIN_PACKET_SIZE: u16 = 28;
const TCP_TRACEROUTE_PACKET_SIZE: u16 = 52;
const DEFAULT_PING_COUNT: NonZeroU16 = NonZeroU16::new(5).unwrap();
const PING_ENDPOINT: &str = "tools/ping";
const PING6_ENDPOINT: &str = "tools/ping6";
const TRACEROUTE_ENDPOINT: &str = "tools/traceroute";
const IPERF3_ENDPOINT: &str = "tools/iperf3";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IpFamily {
    Ipv4,
    Ipv6,
}

impl IpFamily {
    const fn of(address: IpAddr) -> Self {
        match address {
            IpAddr::V4(_) => Self::Ipv4,
            IpAddr::V6(_) => Self::Ipv6,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Ipv4 => "IPv4",
            Self::Ipv6 => "IPv6",
        }
    }

    const fn iperf_version(self) -> IperfIpVersion {
        match self {
            Self::Ipv4 => IperfIpVersion::Ipv4,
            Self::Ipv6 => IperfIpVersion::Ipv6,
        }
    }
}

/// A source address or router interface used by a network test.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum NetworkTestSource {
    /// Bind the test to this source address.
    Address(IpAddr),
    /// Bind the test to this router interface.
    Interface(InterfaceId),
}

impl NetworkTestSource {
    /// Creates an interface source after validating the identifier.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidInterfaceId`] for an invalid interface identifier.
    pub fn interface(name: impl Into<String>) -> Result<Self, InvalidInterfaceId> {
        InterfaceId::new(name).map(Self::Interface)
    }
}

impl From<IpAddr> for NetworkTestSource {
    fn from(address: IpAddr) -> Self {
        Self::Address(address)
    }
}

impl From<Ipv4Addr> for NetworkTestSource {
    fn from(address: Ipv4Addr) -> Self {
        Self::Address(address.into())
    }
}

impl From<Ipv6Addr> for NetworkTestSource {
    fn from(address: Ipv6Addr) -> Self {
        Self::Address(address.into())
    }
}

impl From<InterfaceId> for NetworkTestSource {
    fn from(interface: InterfaceId) -> Self {
        Self::Interface(interface)
    }
}

impl Serialize for NetworkTestSource {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl fmt::Display for NetworkTestSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Address(address) => address.fmt(formatter),
            Self::Interface(interface) => interface.fmt(formatter),
        }
    }
}

/// A network-test option violates a router command invariant.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum InvalidNetworkTestOption {
    /// The target host is malformed.
    #[error(transparent)]
    Host(#[from] InvalidNetworkHost),
    /// A literal target or source address has the wrong family.
    #[error("{tool} requires an {expected} {option}, got {actual}")]
    AddressFamily {
        /// Command name.
        tool: &'static str,
        /// Address-bearing option.
        option: &'static str,
        /// Required address family.
        expected: &'static str,
        /// Supplied address family.
        actual: &'static str,
    },
    /// An integer option is outside the router-supported inclusive range.
    #[error("{option} must be in the range {min}..={max}, got {value}")]
    OutOfRange {
        /// Wire option name.
        option: &'static str,
        /// Inclusive minimum.
        min: u64,
        /// Inclusive maximum.
        max: u64,
        /// Rejected value.
        value: u64,
    },
    /// TCP traceroute has a fixed packet size.
    #[error("TCP traceroute packet size must be {TCP_TRACEROUTE_PACKET_SIZE}, got {0}")]
    TcpTraceroutePacketSize(u16),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PingParameters {
    host: NetworkHost,
    count: NonZeroU16,
    packet_size: Option<u16>,
    sequence_id: Option<u16>,
    source: Option<NetworkTestSource>,
    tos: Option<u8>,
    ttl: Option<NonZeroU8>,
}

impl PingParameters {
    fn new(
        host: impl AsRef<str>,
        tool: &'static str,
        family: IpFamily,
    ) -> Result<Self, InvalidNetworkTestOption> {
        Self::from_host(NetworkHost::new(host)?, tool, family)
    }

    fn from_host(
        host: NetworkHost,
        tool: &'static str,
        family: IpFamily,
    ) -> Result<Self, InvalidNetworkTestOption> {
        ensure_target_family(&host, tool, family)?;
        Ok(Self {
            host,
            count: DEFAULT_PING_COUNT,
            packet_size: None,
            sequence_id: None,
            source: None,
            tos: None,
            ttl: None,
        })
    }

    fn set_count(&mut self, count: u16) -> Result<(), InvalidNetworkTestOption> {
        self.count =
            NonZeroU16::new(count).ok_or_else(|| InvalidNetworkTestOption::OutOfRange {
                option: "count",
                min: 1,
                max: u64::from(u16::MAX),
                value: 0,
            })?;
        Ok(())
    }

    fn set_packet_size(&mut self, packet_size: u16) -> Result<(), InvalidNetworkTestOption> {
        ensure_range(
            "packetsize",
            u64::from(packet_size),
            u64::from(MIN_PACKET_SIZE),
            u64::from(u16::MAX),
        )?;
        self.packet_size = Some(packet_size);
        Ok(())
    }
}

impl Serialize for PingParameters {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("host", &self.host)?;
        map.serialize_entry("count", &self.count)?;
        if let Some(value) = self.packet_size {
            map.serialize_entry("packetsize", &value)?;
        }
        if let Some(value) = self.sequence_id {
            map.serialize_entry("sequence-id", &value)?;
        }
        if let Some(value) = &self.source {
            map.serialize_entry("source", value)?;
        }
        if let Some(value) = self.tos {
            map.serialize_entry("tos", &value)?;
        }
        if let Some(value) = self.ttl {
            map.serialize_entry("ttl", &value)?;
        }
        map.end()
    }
}

fn ensure_range(
    option: &'static str,
    value: u64,
    min: u64,
    max: u64,
) -> Result<(), InvalidNetworkTestOption> {
    if (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(InvalidNetworkTestOption::OutOfRange {
            option,
            min,
            max,
            value,
        })
    }
}

fn ensure_target_family(
    host: &NetworkHost,
    tool: &'static str,
    expected: IpFamily,
) -> Result<(), InvalidNetworkTestOption> {
    if let Some(actual) = host.address().map(IpFamily::of)
        && expected != actual
    {
        return Err(InvalidNetworkTestOption::AddressFamily {
            tool,
            option: "target",
            expected: expected.label(),
            actual: actual.label(),
        });
    }
    Ok(())
}

fn ensure_source_family(
    source: &NetworkTestSource,
    tool: &'static str,
    family: IpFamily,
) -> Result<(), InvalidNetworkTestOption> {
    let NetworkTestSource::Address(address) = source else {
        return Ok(());
    };
    let actual = IpFamily::of(*address);
    if actual == family {
        Ok(())
    } else {
        Err(InvalidNetworkTestOption::AddressFamily {
            tool,
            option: "source address",
            expected: family.label(),
            actual: actual.label(),
        })
    }
}

macro_rules! ping_request {
    ($name:ident, $endpoint:expr, $family:expr, $docs:literal) => {
        #[doc = $docs]
        #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
        #[serde(transparent)]
        pub struct $name(PingParameters);

        impl $name {
            /// Creates a finite five-packet test for a host.
            ///
            /// # Errors
            ///
            /// Returns [`InvalidNetworkTestOption`] for an invalid host or a
            /// literal address from the wrong family.
            pub fn new(host: impl AsRef<str>) -> Result<Self, InvalidNetworkTestOption> {
                PingParameters::new(host, $endpoint, $family).map(Self)
            }

            /// Creates a finite five-packet test from an already parsed host.
            ///
            /// # Errors
            ///
            /// Rejects a literal address from the wrong family.
            pub fn from_host(host: NetworkHost) -> Result<Self, InvalidNetworkTestOption> {
                PingParameters::from_host(host, $endpoint, $family).map(Self)
            }

            /// Sets the number of probe packets.
            ///
            /// # Errors
            ///
            /// Returns [`InvalidNetworkTestOption`] when `count` is zero.
            pub fn count(mut self, count: u16) -> Result<Self, InvalidNetworkTestOption> {
                self.0.set_count(count)?;
                Ok(self)
            }

            /// Sets the ICMP data size in bytes.
            ///
            /// # Errors
            ///
            /// Returns [`InvalidNetworkTestOption`] below 28 bytes.
            pub fn packet_size(
                mut self,
                packet_size: u16,
            ) -> Result<Self, InvalidNetworkTestOption> {
                self.0.set_packet_size(packet_size)?;
                Ok(self)
            }

            /// Sets the ICMP sequence identifier.
            #[must_use]
            pub const fn sequence_id(mut self, sequence_id: u16) -> Self {
                self.0.sequence_id = Some(sequence_id);
                self
            }

            /// Selects a source address or interface.
            ///
            /// # Errors
            ///
            /// Rejects a literal source address from the other address family.
            pub fn source(
                mut self,
                source: impl Into<NetworkTestSource>,
            ) -> Result<Self, InvalidNetworkTestOption> {
                let source = source.into();
                ensure_source_family(&source, $endpoint, $family)?;
                self.0.source = Some(source);
                Ok(self)
            }

            /// Sets the Type of Service value.
            ///
            /// # Errors
            ///
            /// Accepts values in `0..=63`.
            pub fn tos(mut self, tos: u8) -> Result<Self, InvalidNetworkTestOption> {
                ensure_range("tos", u64::from(tos), 0, 63)?;
                self.0.tos = Some(tos);
                Ok(self)
            }

            /// Sets the outgoing packet TTL.
            ///
            /// # Errors
            ///
            /// Returns [`InvalidNetworkTestOption`] when `ttl` is zero.
            pub fn ttl(mut self, ttl: u8) -> Result<Self, InvalidNetworkTestOption> {
                self.0.ttl = Some(NonZeroU8::new(ttl).ok_or_else(|| {
                    InvalidNetworkTestOption::OutOfRange {
                        option: "ttl",
                        min: 1,
                        max: u64::from(u8::MAX),
                        value: 0,
                    }
                })?);
                Ok(self)
            }

            /// Returns the target host.
            #[must_use]
            pub const fn host(&self) -> &NetworkHost {
                &self.0.host
            }
        }

        impl private::NetworkTestSealed for $name {
            const ENDPOINT: &'static str = $endpoint;
        }

        impl NetworkTestRequest for $name {}
    };
}

ping_request!(
    Ping,
    PING_ENDPOINT,
    IpFamily::Ipv4,
    "An IPv4 ICMP echo test."
);
ping_request!(
    PingIpv6,
    PING6_ENDPOINT,
    IpFamily::Ipv6,
    "An IPv6 ICMP echo test."
);

/// Transport protocol used by traceroute probes.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum TracerouteProtocol {
    /// TCP probes. `KeeneticOS` requires a 52-byte packet size.
    Tcp,
    /// UDP probes, the router default.
    #[default]
    Udp,
    /// ICMP probes.
    Icmp,
}

/// IP version used by an iPerf3 client.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum IperfIpVersion {
    /// Force IPv4.
    Ipv4,
    /// Force IPv6.
    Ipv6,
}

impl IperfIpVersion {
    const fn family(self) -> IpFamily {
        match self {
            Self::Ipv4 => IpFamily::Ipv4,
            Self::Ipv6 => IpFamily::Ipv6,
        }
    }

    const fn wire_keyword(self) -> &'static str {
        match self {
            Self::Ipv4 => "ipv4",
            Self::Ipv6 => "ipv6",
        }
    }
}

/// Transport protocol used by an iPerf3 client.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum IperfProtocol {
    /// TCP transport.
    #[default]
    Tcp,
    /// UDP transport.
    Udp,
}

/// Data direction for an iPerf3 test.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum IperfDirection {
    /// The router sends data to the server.
    #[default]
    Upload,
    /// The server sends data to the router using iPerf3 reverse mode.
    Download,
}

/// A finite iPerf3 completion condition.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum IperfLimit {
    /// Stop after this many whole seconds.
    Time(NonZeroU32),
    /// Stop after transmitting this many bytes.
    Bytes(NonZeroU64),
}

impl IperfLimit {
    /// Creates a time limit in whole seconds.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidNetworkTestOption`] when `seconds` is zero.
    pub fn time(seconds: u32) -> Result<Self, InvalidNetworkTestOption> {
        NonZeroU32::new(seconds).map(Self::Time).ok_or_else(|| {
            InvalidNetworkTestOption::OutOfRange {
                option: "time",
                min: 1,
                max: u64::from(u32::MAX),
                value: 0,
            }
        })
    }

    /// Creates a transferred-byte limit.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidNetworkTestOption`] when `bytes` is zero.
    pub fn bytes(bytes: u64) -> Result<Self, InvalidNetworkTestOption> {
        NonZeroU64::new(bytes).map(Self::Bytes).ok_or({
            InvalidNetworkTestOption::OutOfRange {
                option: "bytes",
                min: 1,
                max: u64::MAX,
                value: 0,
            }
        })
    }
}

/// A traceroute request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Traceroute {
    host: NetworkHost,
    count: Option<u8>,
    interval: Option<u8>,
    wait_time: Option<u8>,
    packet_size: Option<u16>,
    max_ttl: Option<NonZeroU8>,
    port: Option<NonZeroU16>,
    source: Option<NetworkTestSource>,
    protocol: TracerouteProtocol,
    tos: Option<u8>,
}

impl Traceroute {
    /// Creates a traceroute using finite router defaults.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidNetworkTestOption`] for an invalid host.
    pub fn new(host: impl AsRef<str>) -> Result<Self, InvalidNetworkTestOption> {
        Ok(Self::from_host(NetworkHost::new(host)?))
    }

    /// Creates a traceroute from an already parsed host.
    #[must_use]
    pub const fn from_host(host: NetworkHost) -> Self {
        Self {
            host,
            count: None,
            interval: None,
            wait_time: None,
            packet_size: None,
            max_ttl: None,
            port: None,
            source: None,
            protocol: TracerouteProtocol::Udp,
            tos: None,
        }
    }

    /// Sets the number of probes per hop.
    ///
    /// # Errors
    ///
    /// Accepts values in `1..=10`.
    pub fn count(mut self, count: u8) -> Result<Self, InvalidNetworkTestOption> {
        ensure_range("count", u64::from(count), 1, 10)?;
        self.count = Some(count);
        Ok(self)
    }

    /// Sets the interval between probes in whole seconds.
    ///
    /// # Errors
    ///
    /// Accepts values in `0..=15`.
    pub fn interval(mut self, seconds: u8) -> Result<Self, InvalidNetworkTestOption> {
        ensure_range("interval", u64::from(seconds), 0, 15)?;
        self.interval = Some(seconds);
        Ok(self)
    }

    /// Sets the per-probe response wait time in whole seconds.
    ///
    /// # Errors
    ///
    /// Accepts values in `1..=15`.
    pub fn wait_time(mut self, seconds: u8) -> Result<Self, InvalidNetworkTestOption> {
        ensure_range("wait-time", u64::from(seconds), 1, 15)?;
        self.wait_time = Some(seconds);
        Ok(self)
    }

    /// Sets the probe packet size.
    ///
    /// # Errors
    ///
    /// TCP accepts exactly 52 bytes. UDP and ICMP accept `28..=65535`.
    pub fn packet_size(mut self, packet_size: u16) -> Result<Self, InvalidNetworkTestOption> {
        validate_traceroute_packet_size(self.protocol, packet_size)?;
        self.packet_size = Some(packet_size);
        Ok(self)
    }

    /// Sets the maximum number of hops.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidNetworkTestOption`] when `max_ttl` is zero.
    pub fn max_ttl(mut self, max_ttl: u8) -> Result<Self, InvalidNetworkTestOption> {
        self.max_ttl =
            Some(
                NonZeroU8::new(max_ttl).ok_or_else(|| InvalidNetworkTestOption::OutOfRange {
                    option: "max-ttl",
                    min: 1,
                    max: u64::from(u8::MAX),
                    value: 0,
                })?,
            );
        Ok(self)
    }

    /// Sets the destination port.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidNetworkTestOption`] when `port` is zero.
    pub fn port(mut self, port: u16) -> Result<Self, InvalidNetworkTestOption> {
        self.port = Some(nonzero_u16("port", port)?);
        Ok(self)
    }

    /// Selects a source address or interface.
    ///
    /// # Errors
    ///
    /// When the target is a literal address, rejects a literal source from the
    /// other address family.
    pub fn source(
        mut self,
        source: impl Into<NetworkTestSource>,
    ) -> Result<Self, InvalidNetworkTestOption> {
        let source = source.into();
        if let Some(address) = self.host.address() {
            ensure_source_family(&source, TRACEROUTE_ENDPOINT, IpFamily::of(address))?;
        }
        self.source = Some(source);
        Ok(self)
    }

    /// Selects the probe protocol.
    ///
    /// # Errors
    ///
    /// Rejects an already configured packet size incompatible with `protocol`.
    pub fn protocol(
        mut self,
        protocol: TracerouteProtocol,
    ) -> Result<Self, InvalidNetworkTestOption> {
        if let Some(packet_size) = self.packet_size {
            validate_traceroute_packet_size(protocol, packet_size)?;
        }
        self.protocol = protocol;
        Ok(self)
    }

    /// Sets the Type of Service value.
    #[must_use]
    pub const fn tos(mut self, tos: u8) -> Self {
        self.tos = Some(tos);
        self
    }

    /// Returns the target host.
    #[must_use]
    pub const fn host(&self) -> &NetworkHost {
        &self.host
    }
}

impl NetworkTestRequest for Traceroute {}

impl Serialize for Traceroute {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("host", &self.host)?;
        serialize_option(&mut map, "count", self.count)?;
        serialize_option(&mut map, "interval", self.interval)?;
        serialize_option(&mut map, "wait-time", self.wait_time)?;
        serialize_option(&mut map, "packetsize", self.packet_size)?;
        serialize_option(&mut map, "max-ttl", self.max_ttl)?;
        serialize_option(&mut map, "port", self.port)?;
        serialize_source(&mut map, self.source.as_ref())?;
        map.serialize_entry("type", &self.protocol)?;
        serialize_option(&mut map, "tos", self.tos)?;
        map.end()
    }
}

impl private::NetworkTestSealed for Traceroute {
    const ENDPOINT: &'static str = TRACEROUTE_ENDPOINT;
}

/// An iPerf3 client request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Iperf3 {
    host: NetworkHost,
    ip_version: IperfIpVersion,
    protocol: IperfProtocol,
    direction: IperfDirection,
    port: Option<NonZeroU16>,
    bitrate: Option<NonZeroU64>,
    streams: Option<NonZeroU16>,
    limit: IperfLimit,
    source: Option<NetworkTestSource>,
}

impl Iperf3 {
    /// Creates a finite iPerf3 client request.
    ///
    /// Literal addresses select their own IP version; host names default to
    /// IPv4. TCP upload mode and the router's default port are used.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidNetworkTestOption`] for an invalid host.
    pub fn new(host: impl AsRef<str>, limit: IperfLimit) -> Result<Self, InvalidNetworkTestOption> {
        Ok(Self::from_host(NetworkHost::new(host)?, limit))
    }

    /// Creates a finite iPerf3 request from an already parsed host.
    #[must_use]
    pub const fn from_host(host: NetworkHost, limit: IperfLimit) -> Self {
        let ip_version = match host.address() {
            Some(address) => IpFamily::of(address).iperf_version(),
            None => IperfIpVersion::Ipv4,
        };
        Self {
            host,
            ip_version,
            protocol: IperfProtocol::Tcp,
            direction: IperfDirection::Upload,
            port: None,
            bitrate: None,
            streams: None,
            limit,
            source: None,
        }
    }

    /// Forces an IP version.
    ///
    /// # Errors
    ///
    /// Rejects a literal target from the other address family.
    pub fn ip_version(
        mut self,
        ip_version: IperfIpVersion,
    ) -> Result<Self, InvalidNetworkTestOption> {
        let expected = ip_version.family();
        ensure_target_family(&self.host, IPERF3_ENDPOINT, expected)?;
        if let Some(source) = &self.source {
            ensure_source_family(source, IPERF3_ENDPOINT, expected)?;
        }
        self.ip_version = ip_version;
        Ok(self)
    }

    /// Selects TCP or UDP transport.
    #[must_use]
    pub const fn protocol(mut self, protocol: IperfProtocol) -> Self {
        self.protocol = protocol;
        self
    }

    /// Selects upload or reverse/download mode.
    #[must_use]
    pub const fn direction(mut self, direction: IperfDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Sets the server port.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidNetworkTestOption`] when `port` is zero.
    pub fn port(mut self, port: u16) -> Result<Self, InvalidNetworkTestOption> {
        self.port = Some(nonzero_u16("port", port)?);
        Ok(self)
    }

    /// Limits the target bitrate in bits per second.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidNetworkTestOption`] when `bitrate` is zero.
    pub fn bitrate(mut self, bitrate: u64) -> Result<Self, InvalidNetworkTestOption> {
        self.bitrate = Some(nonzero_u64("bitrate", bitrate)?);
        Ok(self)
    }

    /// Sets the number of simultaneous streams.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidNetworkTestOption`] when `streams` is zero.
    pub fn streams(mut self, streams: u16) -> Result<Self, InvalidNetworkTestOption> {
        self.streams = Some(nonzero_u16("streams", streams)?);
        Ok(self)
    }

    /// Selects a source address or interface.
    ///
    /// For an IPv6 source with a host-name target, select
    /// [`IperfIpVersion::Ipv6`] first.
    ///
    /// # Errors
    ///
    /// Rejects a literal source address from the other address family.
    pub fn source(
        mut self,
        source: impl Into<NetworkTestSource>,
    ) -> Result<Self, InvalidNetworkTestOption> {
        let source = source.into();
        ensure_source_family(&source, IPERF3_ENDPOINT, self.ip_version.family())?;
        self.source = Some(source);
        Ok(self)
    }

    /// Returns the target host.
    #[must_use]
    pub const fn host(&self) -> &NetworkHost {
        &self.host
    }
}

impl NetworkTestRequest for Iperf3 {}

impl Serialize for Iperf3 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("host", &self.host)?;
        map.serialize_entry(self.ip_version.wire_keyword(), &true)?;
        map.serialize_entry(
            match self.protocol {
                IperfProtocol::Tcp => "tcp",
                IperfProtocol::Udp => "udp",
            },
            &true,
        )?;
        if self.direction == IperfDirection::Download {
            map.serialize_entry("reverse", &true)?;
        }
        serialize_option(&mut map, "port", self.port)?;
        serialize_option(&mut map, "bitrate", self.bitrate)?;
        serialize_option(&mut map, "streams", self.streams)?;
        match self.limit {
            IperfLimit::Time(value) => map.serialize_entry("time", &value)?,
            IperfLimit::Bytes(value) => map.serialize_entry("bytes", &value)?,
        }
        serialize_source(&mut map, self.source.as_ref())?;
        map.end()
    }
}

impl private::NetworkTestSealed for Iperf3 {
    const ENDPOINT: &'static str = IPERF3_ENDPOINT;
}

fn validate_traceroute_packet_size(
    protocol: TracerouteProtocol,
    packet_size: u16,
) -> Result<(), InvalidNetworkTestOption> {
    if protocol == TracerouteProtocol::Tcp {
        if packet_size == TCP_TRACEROUTE_PACKET_SIZE {
            Ok(())
        } else {
            Err(InvalidNetworkTestOption::TcpTraceroutePacketSize(
                packet_size,
            ))
        }
    } else {
        ensure_range(
            "packetsize",
            u64::from(packet_size),
            u64::from(MIN_PACKET_SIZE),
            u64::from(u16::MAX),
        )
    }
}

fn nonzero_u16(option: &'static str, value: u16) -> Result<NonZeroU16, InvalidNetworkTestOption> {
    NonZeroU16::new(value).ok_or_else(|| InvalidNetworkTestOption::OutOfRange {
        option,
        min: 1,
        max: u64::from(u16::MAX),
        value: 0,
    })
}

fn nonzero_u64(option: &'static str, value: u64) -> Result<NonZeroU64, InvalidNetworkTestOption> {
    NonZeroU64::new(value).ok_or(InvalidNetworkTestOption::OutOfRange {
        option,
        min: 1,
        max: u64::MAX,
        value: 0,
    })
}

fn serialize_option<M, T>(map: &mut M, key: &'static str, value: Option<T>) -> Result<(), M::Error>
where
    M: SerializeMap,
    T: Serialize,
{
    if let Some(value) = value {
        map.serialize_entry(key, &value)?;
    }
    Ok(())
}

fn serialize_source<M>(map: &mut M, source: Option<&NetworkTestSource>) -> Result<(), M::Error>
where
    M: SerializeMap,
{
    if let Some(source) = source {
        match source {
            NetworkTestSource::Address(address) => {
                map.serialize_entry("source-address", address)?;
            }
            NetworkTestSource::Interface(interface) => {
                map.serialize_entry("source-interface", interface)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    use serde_json::json;

    use super::{
        InvalidNetworkTestOption, Iperf3, IperfDirection, IperfIpVersion, IperfLimit,
        IperfProtocol, NetworkHost, NetworkTestSource, Ping, PingIpv6, Traceroute,
        TracerouteProtocol,
    };

    #[test]
    fn requests_accept_already_parsed_hosts() {
        let name = NetworkHost::try_from(String::from("example.com")).unwrap();
        assert_eq!(Ping::from_host(name.clone()).unwrap().host(), &name);
        assert_eq!(Traceroute::from_host(name.clone()).host(), &name);

        let ipv6 = NetworkHost::from(IpAddr::V6(Ipv6Addr::LOCALHOST));
        let limit = IperfLimit::time(1).unwrap();
        assert_eq!(Iperf3::from_host(ipv6.clone(), limit).host(), &ipv6);
        assert!(Ping::from_host(ipv6).is_err());
    }

    #[test]
    fn serializes_ping_and_ping6_options() {
        let ping = Ping::new("example.com")
            .unwrap()
            .count(3)
            .unwrap()
            .packet_size(84)
            .unwrap()
            .sequence_id(7)
            .source(Ipv4Addr::new(192, 0, 2, 1))
            .unwrap()
            .tos(12)
            .unwrap()
            .ttl(64)
            .unwrap();
        assert_eq!(
            serde_json::to_value(ping).unwrap(),
            json!({
                "host": "example.com",
                "count": 3,
                "packetsize": 84,
                "sequence-id": 7,
                "source": "192.0.2.1",
                "tos": 12,
                "ttl": 64
            })
        );

        let ping6 = PingIpv6::new("2001:db8::1")
            .unwrap()
            .count(6)
            .unwrap()
            .packet_size(128)
            .unwrap()
            .sequence_id(u16::MAX)
            .source(NetworkTestSource::interface("Wireguard0").unwrap())
            .unwrap()
            .tos(63)
            .unwrap()
            .ttl(u8::MAX)
            .unwrap();
        assert_eq!(
            serde_json::to_value(ping6).unwrap(),
            json!({
                "host": "2001:db8::1",
                "count": 6,
                "packetsize": 128,
                "sequence-id": 65535,
                "source": "Wireguard0",
                "tos": 63,
                "ttl": 255
            })
        );
        assert!(matches!(
            Ping::new("2001:db8::1"),
            Err(InvalidNetworkTestOption::AddressFamily { .. })
        ));
        assert!(matches!(
            PingIpv6::new("192.0.2.1"),
            Err(InvalidNetworkTestOption::AddressFamily { .. })
        ));
        assert!(Ping::new("example.com").unwrap().tos(64).is_err());
    }

    #[test]
    fn serializes_all_traceroute_options() {
        let request = Traceroute::new("example.com")
            .unwrap()
            .count(4)
            .unwrap()
            .interval(2)
            .unwrap()
            .wait_time(3)
            .unwrap()
            .packet_size(128)
            .unwrap()
            .max_ttl(20)
            .unwrap()
            .port(33434)
            .unwrap()
            .source(NetworkTestSource::interface("ISP").unwrap())
            .unwrap()
            .protocol(TracerouteProtocol::Icmp)
            .unwrap()
            .tos(32);
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            json!({
                "host": "example.com",
                "count": 4,
                "interval": 2,
                "wait-time": 3,
                "packetsize": 128,
                "max-ttl": 20,
                "port": 33434,
                "source-interface": "ISP",
                "type": "icmp",
                "tos": 32
            })
        );

        for (protocol, wire) in [
            (TracerouteProtocol::Tcp, "tcp"),
            (TracerouteProtocol::Udp, "udp"),
            (TracerouteProtocol::Icmp, "icmp"),
        ] {
            let request = Traceroute::new("192.0.2.2")
                .unwrap()
                .source(Ipv4Addr::new(192, 0, 2, 1))
                .unwrap()
                .protocol(protocol)
                .unwrap();
            assert_eq!(
                serde_json::to_value(request).unwrap(),
                json!({
                    "host": "192.0.2.2",
                    "source-address": "192.0.2.1",
                    "type": wire
                })
            );
        }
    }

    #[test]
    fn traceroute_revalidates_protocol_dependent_packet_size() {
        assert!(
            Traceroute::new("example.com")
                .unwrap()
                .protocol(TracerouteProtocol::Tcp)
                .unwrap()
                .packet_size(53)
                .is_err()
        );
        assert!(
            Traceroute::new("example.com")
                .unwrap()
                .packet_size(100)
                .unwrap()
                .protocol(TracerouteProtocol::Tcp)
                .is_err()
        );
        assert!(Traceroute::new("example.com").unwrap().count(0).is_err());
        assert!(
            Traceroute::new("example.com")
                .unwrap()
                .interval(16)
                .is_err()
        );
    }

    #[test]
    fn numeric_options_enforce_documented_boundaries() {
        let ping = Ping::new("127.0.0.1").unwrap();
        assert!(ping.clone().count(0).is_err());
        assert!(ping.clone().count(1).is_ok());
        assert!(ping.clone().count(u16::MAX).is_ok());
        assert!(ping.clone().packet_size(27).is_err());
        assert!(ping.clone().packet_size(28).is_ok());
        assert!(ping.clone().packet_size(u16::MAX).is_ok());
        assert!(ping.clone().tos(63).is_ok());
        assert!(ping.clone().tos(64).is_err());
        assert!(ping.clone().ttl(0).is_err());
        assert!(ping.ttl(u8::MAX).is_ok());

        let traceroute = Traceroute::new("127.0.0.1").unwrap();
        assert!(traceroute.clone().count(1).is_ok());
        assert!(traceroute.clone().count(10).is_ok());
        assert!(traceroute.clone().count(11).is_err());
        assert!(traceroute.clone().interval(0).is_ok());
        assert!(traceroute.clone().interval(15).is_ok());
        assert!(traceroute.clone().interval(16).is_err());
        assert!(traceroute.clone().wait_time(0).is_err());
        assert!(traceroute.clone().wait_time(15).is_ok());
        assert!(traceroute.clone().wait_time(16).is_err());
        assert!(traceroute.clone().packet_size(27).is_err());
        assert!(traceroute.clone().packet_size(28).is_ok());
        assert!(traceroute.clone().packet_size(u16::MAX).is_ok());
        assert!(traceroute.clone().max_ttl(0).is_err());
        assert!(traceroute.clone().max_ttl(u8::MAX).is_ok());
        assert!(traceroute.clone().port(0).is_err());
        assert!(traceroute.port(u16::MAX).is_ok());

        let iperf = Iperf3::new("127.0.0.1", IperfLimit::time(1).unwrap()).unwrap();
        assert!(iperf.clone().port(0).is_err());
        assert!(iperf.clone().port(u16::MAX).is_ok());
        assert!(iperf.clone().bitrate(0).is_err());
        assert!(iperf.clone().bitrate(u64::MAX).is_ok());
        assert!(iperf.clone().streams(0).is_err());
        assert!(iperf.streams(u16::MAX).is_ok());
        assert!(IperfLimit::time(u32::MAX).is_ok());
        assert!(IperfLimit::bytes(u64::MAX).is_ok());
    }

    #[test]
    fn rejects_incompatible_target_and_source_address_families() {
        assert!(
            Ping::new("127.0.0.1")
                .unwrap()
                .source(Ipv6Addr::LOCALHOST)
                .is_err()
        );
        assert!(
            PingIpv6::new("::1")
                .unwrap()
                .source(Ipv4Addr::LOCALHOST)
                .is_err()
        );
        assert!(
            Traceroute::new("192.0.2.1")
                .unwrap()
                .source(Ipv6Addr::LOCALHOST)
                .is_err()
        );

        let limit = IperfLimit::time(1).unwrap();
        assert!(
            Iperf3::new("192.0.2.1", limit)
                .unwrap()
                .ip_version(IperfIpVersion::Ipv6)
                .is_err()
        );
        assert!(
            Iperf3::new("server.example", limit)
                .unwrap()
                .source(Ipv6Addr::LOCALHOST)
                .is_err()
        );
        assert!(
            Iperf3::new("server.example", limit)
                .unwrap()
                .source(Ipv4Addr::LOCALHOST)
                .unwrap()
                .ip_version(IperfIpVersion::Ipv6)
                .is_err()
        );
    }

    #[test]
    fn serializes_iperf_keywords_limit_and_source() {
        let request = Iperf3::new("2001:db8::2", IperfLimit::bytes(1_000_000).unwrap())
            .unwrap()
            .ip_version(IperfIpVersion::Ipv6)
            .unwrap()
            .protocol(IperfProtocol::Udp)
            .direction(IperfDirection::Download)
            .port(5202)
            .unwrap()
            .bitrate(10_000_000)
            .unwrap()
            .streams(4)
            .unwrap()
            .source(Ipv6Addr::LOCALHOST)
            .unwrap();
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            json!({
                "host": "2001:db8::2",
                "ipv6": true,
                "udp": true,
                "reverse": true,
                "port": 5202,
                "bitrate": 10_000_000,
                "streams": 4,
                "bytes": 1_000_000,
                "source-address": "::1"
            })
        );

        let time = Iperf3::new("server.example", IperfLimit::time(1).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(
                time.source(NetworkTestSource::interface("ISP").unwrap())
                    .unwrap(),
            )
            .unwrap(),
            json!({
                "host": "server.example",
                "ipv4": true,
                "tcp": true,
                "time": 1,
                "source-interface": "ISP"
            })
        );
        assert!(IperfLimit::time(0).is_err());
        assert!(IperfLimit::bytes(0).is_err());
    }
}
