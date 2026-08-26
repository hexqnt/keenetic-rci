//! Typed requests supported by the crate.

use serde::de::DeserializeOwned;

use crate::model::{
    Interfaces, InternetStatus, ShowInterfaceReply, ShowLteInterfaceReply, Version,
    connectivity::{IpNameServers, NtpStatus, PingCheckProfiles},
    hotspot::IpHotspotHosts,
    identification::Identification,
    interface_stat::InterfaceStat,
    ip::{IpArp, IpDhcpBindings},
    mesh::{MwsMembers, MwsStatus},
    network::{InterfaceId, InvalidInterfaceId},
    routing::{IpRoutes, Ipv6Routes},
    storage::{MediaInventory, UsbDevices},
    system::System,
    system_mode::SystemModeStatus,
    wifi::Associations,
};

pub use network_test::{
    InvalidNetworkTestOption, Iperf3, IperfDirection, IperfIpVersion, IperfLimit, IperfProtocol,
    NetworkTestSource, Ping, PingIpv6, Traceroute, TracerouteProtocol,
};

pub(crate) mod private {
    use serde::Serialize;

    use super::ShowInterfaceStat;

    pub enum Mode {
        Get,
        PostJson(Vec<u8>),
    }

    pub trait Sealed {
        const ENDPOINT: &'static str;

        fn method(&self) -> reqwest::Method;

        fn mode(&self) -> Result<Mode, serde_json::Error>;

        fn query(&self) -> Option<(&'static str, &str)> {
            None
        }
    }

    impl<T> Sealed for &T
    where
        T: Sealed + ?Sized,
    {
        const ENDPOINT: &'static str = T::ENDPOINT;

        fn method(&self) -> reqwest::Method {
            T::method(self)
        }

        fn mode(&self) -> Result<Mode, serde_json::Error> {
            T::mode(self)
        }

        fn query(&self) -> Option<(&'static str, &str)> {
            T::query(self)
        }
    }

    impl Sealed for ShowInterfaceStat {
        const ENDPOINT: &'static str = "show/interface/stat";

        fn method(&self) -> reqwest::Method {
            reqwest::Method::GET
        }

        fn mode(&self) -> Result<Mode, serde_json::Error> {
            Ok(Mode::Get)
        }

        fn query(&self) -> Option<(&'static str, &str)> {
            Some(("name", self.name.as_str()))
        }
    }

    pub trait NetworkTestSealed: Serialize {
        const ENDPOINT: &'static str;

        fn body(&self) -> Result<Vec<u8>, serde_json::Error> {
            serde_json::to_vec(self)
        }
    }

    pub(super) fn show_interface_mode(name: &str) -> Result<Mode, serde_json::Error> {
        #[derive(Serialize)]
        struct Command<'a> {
            show: Show<'a>,
        }

        #[derive(Serialize)]
        struct Show<'a> {
            interface: Interface<'a>,
        }

        #[derive(Serialize)]
        struct Interface<'a> {
            name: &'a str,
        }

        serde_json::to_vec(&Command {
            show: Show {
                interface: Interface { name },
            },
        })
        .map(Mode::PostJson)
    }
}

mod network_test;

macro_rules! get_requests {
    ($($request:ident => ($response:ty, $endpoint:literal);)+) => {
        $(
            #[doc = concat!("`GET /rci/", $endpoint, "`.")]
            #[derive(Clone, Copy, Debug, Default)]
            pub struct $request;

            impl private::Sealed for $request {
                const ENDPOINT: &'static str = $endpoint;

                fn method(&self) -> reqwest::Method {
                    reqwest::Method::GET
                }

                fn mode(&self) -> Result<private::Mode, serde_json::Error> {
                    Ok(private::Mode::Get)
                }
            }

            impl RciRequest for $request {
                type Response = $response;
            }
        )+
    };
}

get_requests! {
    ShowVersion => (Version, "show/version");
    ShowSystem => (System, "show/system");
    ShowInternetStatus => (InternetStatus, "show/internet/status");
    ShowInterfaces => (Interfaces, "show/interface");
    ShowAssociations => (Associations, "show/associations");
    ShowIpHotspotHosts => (IpHotspotHosts, "show/ip/hotspot");
    ShowIdentification => (Identification, "show/identification");
    ShowSystemMode => (SystemModeStatus, "show/system/mode");
    ShowIpArp => (IpArp, "show/ip/arp");
    ShowIpDhcpBindings => (IpDhcpBindings, "show/ip/dhcp/bindings");
    ShowIpRoute => (IpRoutes, "show/ip/route");
    ShowIpv6Route => (Ipv6Routes, "show/ipv6/route");
    ShowPingCheck => (PingCheckProfiles, "show/ping-check");
    ShowIpNameServers => (IpNameServers, "show/ip/name-server");
    ShowNtpStatus => (NtpStatus, "show/ntp/status");
    ShowMwsStatus => (MwsStatus, "show/mws/status");
    ShowMwsMembers => (MwsMembers, "show/mws/member");
    ShowUsb => (UsbDevices, "show/usb");
    ShowMedia => (MediaInventory, "show/media");
}

/// `GET /rci/show/interface/stat?name=...`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShowInterfaceStat {
    pub(crate) name: InterfaceId,
}

impl ShowInterfaceStat {
    /// Creates a request for one interface's counters and rates.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidInterfaceId`] for an invalid interface identifier.
    pub fn new(name: impl Into<String>) -> Result<Self, InvalidInterfaceId> {
        InterfaceId::new(name).map(Self::from_id)
    }

    /// Creates a request from an already validated interface identifier.
    #[must_use]
    pub const fn from_id(name: InterfaceId) -> Self {
        Self { name }
    }

    /// Returns the requested interface identifier.
    #[must_use]
    pub const fn name(&self) -> &InterfaceId {
        &self.name
    }
}

impl From<InterfaceId> for ShowInterfaceStat {
    fn from(name: InterfaceId) -> Self {
        Self::from_id(name)
    }
}

impl RciRequest for ShowInterfaceStat {
    type Response = InterfaceStat;
}

macro_rules! interface_request {
    ($(#[$meta:meta])* $request:ident => $response:ty) => {
        $(#[$meta])*
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $request {
            name: InterfaceId,
        }

        impl $request {
            /// Creates a request for one interface.
            ///
            /// # Errors
            ///
            /// Returns [`InvalidInterfaceId`] when the name is empty or contains a
            /// control character.
            pub fn new(name: impl Into<String>) -> Result<Self, InvalidInterfaceId> {
                InterfaceId::new(name).map(Self::from_id)
            }

            /// Creates a request from an already validated interface identifier.
            #[must_use]
            pub const fn from_id(name: InterfaceId) -> Self {
                Self { name }
            }

            /// Returns the requested interface name.
            #[must_use]
            pub const fn name(&self) -> &InterfaceId {
                &self.name
            }
        }

        impl From<InterfaceId> for $request {
            fn from(name: InterfaceId) -> Self {
                Self::from_id(name)
            }
        }

        impl private::Sealed for $request {
            const ENDPOINT: &'static str = "";

            fn method(&self) -> reqwest::Method {
                reqwest::Method::POST
            }

            fn mode(&self) -> Result<private::Mode, serde_json::Error> {
                private::show_interface_mode(self.name.as_str())
            }
        }

        impl RciRequest for $request {
            type Response = $response;
        }
    };
}

interface_request!(
    /// `POST /rci/` with a `show interface name ...` JSON command.
    ShowInterface => ShowInterfaceReply
);

interface_request!(
    /// `POST /rci/` with a typed LTE `show interface name ...` JSON command.
    ///
    /// The response must carry a verified `Mobile`, `UsbLte`, or `UsbQmi` trait;
    /// requesting a non-LTE interface produces a response-deserialization error.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use keenetic_rci::{KeeneticClient, request::{ShowInterfaces, ShowLteInterface}};
    ///
    /// # async fn example(client: KeeneticClient) -> Result<(), Box<dyn std::error::Error>> {
    ///     let interfaces = client.execute(ShowInterfaces).await?;
    ///     let lte_name = interfaces
    ///         .lte()
    ///         .next()
    ///         .map(|(name, _)| name.to_owned());
    ///     if let Some(name) = lte_name {
    ///         let reply = client.execute(ShowLteInterface::new(name)?).await?;
    ///         println!("RSRP: {:?}", reply.interface().status().signal.rsrp);
    ///     }
    /// # Ok(())
    /// # }
    /// ```
    ShowLteInterface => ShowLteInterfaceReply
);

/// A sealed typed RCI request.
///
/// This trait cannot be implemented outside this crate. Use the raw methods on
/// [`crate::KeeneticClient`] for endpoints without a typed request.
/// Shared references also implement this trait, allowing a prepared request to
/// be reused without cloning its validated input.
pub trait RciRequest: private::Sealed {
    /// Typed response returned by the endpoint.
    type Response: DeserializeOwned;
}

impl<T> RciRequest for &T
where
    T: RciRequest + ?Sized,
{
    type Response = T::Response;
}

/// A sealed typed request for a continued Network Connection Test operation.
///
/// Use [`crate::KeeneticClient::start_network_test`] to consume output as it
/// arrives, or [`crate::KeeneticClient::run_network_test`] to collect it.
pub trait NetworkTestRequest: private::NetworkTestSealed {}

#[cfg(test)]
mod tests {
    use super::{
        ShowInterface, ShowInterfaceStat, ShowLteInterface, private::Mode, private::Sealed,
    };
    use crate::model::network::InterfaceId;

    #[test]
    fn interface_names_share_the_json_command_shape() {
        for name in ["Bridge0", "WifiMaster0/AccessPoint0"] {
            let request = ShowInterface::new(name).unwrap();
            let Ok(Mode::PostJson(body)) = request.mode() else {
                panic!("unexpected request mode");
            };
            let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(
                value,
                serde_json::json!({"show":{"interface":{"name":name}}})
            );
        }

        let request = ShowLteInterface::new("UsbLte0").unwrap();
        let Ok(Mode::PostJson(body)) = request.mode() else {
            panic!("unexpected request mode");
        };
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
            serde_json::json!({"show":{"interface":{"name":"UsbLte0"}}})
        );

        let id: InterfaceId = "WifiMaster0/AccessPoint0".parse().unwrap();
        let request = ShowInterfaceStat::from(id);
        assert_eq!(request.query(), Some(("name", "WifiMaster0/AccessPoint0")));
    }
}
