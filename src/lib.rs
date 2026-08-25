//! Asynchronous client for the local Keenetic RCI HTTP API.
//!
//! [`KeeneticClient::execute`] is the primary typed interface. The raw methods
//! on [`KeeneticClient`] are escape hatches for RCI endpoints that do not yet
//! have a typed request.
//!
//! # Example
//!
//! ```no_run
//! use keenetic_rci::{KeeneticClient, request::ShowVersion};
//!
//! # async fn example(password: String) -> Result<(), Box<dyn std::error::Error>> {
//!     let client = KeeneticClient::builder()
//!         .base_url("http://192.168.1.1")
//!         .credentials("monitor", password)
//!         .build()?;
//!     let version = client.execute(ShowVersion).await?;
//!     println!("{}", version.title);
//! # Ok(())
//! # }
//! ```

pub use ipnet::{Ipv4Net, Ipv6Net};

pub use client::{KeeneticClient, KeeneticClientBuilder, NetworkTestOutput, NetworkTestSession};
pub use error::{
    AuthenticationError, ConfigError, Error, HttpError, JsonSerializationError,
    MalformedAuthReason, QuerySerializationError, RciError, RciStatusEntry, RequestContext,
    ResponseDeserializationError, ResponseJsonError, TransportError,
};
pub use model::hardware_id::{
    HARDWARE_MODEL_LENGTH, HardwareId, HardwareModel, HardwareVendor, ParseHardwareIdError,
    ParseHardwareModelError, ParseHardwareVendorError,
};
pub use model::iccid::{Iccid, MAX_ICCID_LENGTH, MIN_ICCID_LENGTH, ParseIccidError};
pub use model::imei::{Imei, ParseImeiError};
pub use model::imsi::{Imsi, MAX_IMSI_LENGTH, MIN_IMSI_LENGTH, ParseImsiError};
pub use model::plmn::{ParsePlmnError, Plmn};
pub use model::reported::Reported;
pub use model::version::{
    Architecture, FirmwareChannel, HARDWARE_VERSION_LENGTH, HardwareType, HardwareVersion,
    ParseHardwareVersionError, ParseRegionCodeError, REGION_CODE_LENGTH, RegionCode,
};
pub use model::{
    Interface, InterfaceKind, InterfaceLayerSummary, InterfaceState, InterfaceSummary,
    InterfaceTrait, Interfaces, InternetStatus, LinkState, ShowInterfaceReply, ShowInterfaceResult,
    ShowLteInterfaceReply, ShowLteInterfaceResult, Version, VersionBuild, VersionCapabilities,
    mobile::CarrierBandwidth,
    mobile::ComponentCarrier,
    mobile::InvalidCarrierBandwidth,
    mobile::LteModem,
    mobile::LteSim,
    mobile::MobileConnectionState,
    mobile::MobileInterface,
    mobile::MobileSignal,
    mobile::MobileStatus,
    mobile::RadioAccessTechnology,
    mobile::RadioBand,
    mobile::ServingCell,
    mobile::SimState,
    system::{CpuLoad, InvalidCpuLoad, InvalidMtu, Mtu, System},
    units::Celsius,
    units::Db,
    units::Dbm,
    units::InvalidMeasurement,
};
pub use model::{
    clients::{ClientActivity, ClientIndex, ConnectedClient},
    connectivity::{
        IpNameServer, IpNameServers, NtpStatus, PingCheckCacheEntry, PingCheckInterface,
        PingCheckMode, PingCheckProfile, PingCheckProfiles, PingCheckStatus,
    },
    hotspot::{
        HotspotAccess, HotspotDhcp, HotspotHost, InterfaceReference, IpHotspotHosts, TrafficShape,
        TrafficShapeMode,
    },
    identification::{
        CustomerId, Identification, InvalidCustomerId, InvalidSerialNumber, InvalidServiceTag,
        SerialNumber, ServiceTag,
    },
    interface_stat::InterfaceStat,
    ip::{ArpEntry, ArpState, DhcpBinding, DhcpBindingMode, IpArp, IpDhcpBindings},
    mesh::{
        InvalidMwsMemoryUsage, MwsBackhaul, MwsBand, MwsControllerStatus, MwsMember,
        MwsMemberSystem, MwsMembers, MwsMemoryUsage, MwsPort, MwsRciStatus, MwsStatus, MwsWireless,
    },
    network::{
        ByteCount, ChannelWidth, DataRate, GuardInterval, InterfaceId, InvalidDataRate,
        InvalidInterfaceId, InvalidNetworkHost, LeaseExpiration, MacAddress, McsIndex, NetworkHost,
        ParseMacAddressError, SpatialStreams, Uptime,
    },
    routing::{IpRoutes, Ipv4Route, Ipv6Route, Ipv6Routes, RouteFlags, RouteProtocol},
    storage::{
        FileSystemType, InvalidInventoryId, InvalidVolumeId, MediaBus, MediaDevice, MediaId,
        MediaInventory, MediaPartition, MediaState, PartitionId, UsbDevice, UsbDeviceId,
        UsbDevices, UsbPowerControl, UsbSubsystem, UsbVersion, VolumeId,
    },
    system_mode::{SystemModeStatus, SystemOperatingMode},
    wifi::{Association, Associations, WifiMode, WifiPeerLink, WifiSecurity},
};
pub use path::{CiPath, InvalidPath, PathKind, RciPath};
pub use request::{
    InvalidNetworkTestOption, Iperf3, IperfDirection, IperfIpVersion, IperfLimit, IperfProtocol,
    NetworkTestRequest, NetworkTestSource, Ping, PingIpv6, Traceroute, TracerouteProtocol,
};

mod auth;
mod client;
mod error;
mod model;
mod path;
pub mod request;
