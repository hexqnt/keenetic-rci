use std::{collections::BTreeMap, convert::Infallible, fmt, ops::Deref, str::FromStr};

use serde::{Deserialize, Deserializer};
use thiserror::Error;

use crate::model::units::{Celsius, Db, Dbm, optional_measurement};

use super::Interface;
use super::iccid::Iccid;
use super::imei::Imei;
use super::imsi::Imsi;
use super::optional_nonempty_string;
use super::plmn::{Plmn, deserialize_optional as optional_plmn};
use super::reported::{Reported, deserialize_optional as optional_reported};
use super::text::{FromStrVisitor, deserialize_optional_from_str as optional_from_str};

/// The radio access technology reported for a component carrier.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RadioAccessTechnology {
    /// Second-generation mobile technology.
    G2,
    /// Third-generation mobile technology.
    G3,
    /// Fourth-generation mobile technology (LTE).
    G4,
    /// LTE Advanced with carrier aggregation.
    G4Plus,
    /// Fifth-generation mobile technology (NR).
    G5,
    /// A technology introduced by a different firmware version.
    Other(Box<str>),
}

open_string_enum!(RadioAccessTechnology {
    G2 => "2G",
    G3 => "3G",
    G4 => "4G",
    G4Plus => "4G+",
    G5 => "5G",
});

/// Mobile data connection state reported by the modem.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MobileConnectionState {
    /// The modem is initializing.
    Initializing,
    /// The mobile data connection is established.
    Connected,
    /// A state introduced by a different modem or firmware version.
    Other(Box<str>),
}

impl MobileConnectionState {
    /// Returns the state in its modem representation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Initializing => "Initializing",
            Self::Connected => "Connected",
            Self::Other(value) => value,
        }
    }
}

impl FromStr for MobileConnectionState {
    type Err = Infallible;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(if value.eq_ignore_ascii_case("Initializing") {
            Self::Initializing
        } else if value.eq_ignore_ascii_case("Connected") {
            Self::Connected
        } else {
            Self::Other(value.into())
        })
    }
}

impl fmt::Display for MobileConnectionState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// State of the SIM card reported by the modem.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SimState {
    /// The SIM card is ready for use.
    Ready,
    /// The SIM card is waiting for its PIN.
    PinRequired,
    /// The SIM card is blocked and waiting for its PUK.
    PukRequired,
    /// The modem considers the SIM card invalid.
    Invalid,
    /// A state introduced by a different modem or firmware version.
    Other(Box<str>),
}

impl SimState {
    /// Returns the state in its modem representation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Ready => "READY",
            Self::PinRequired => "SIM PIN",
            Self::PukRequired => "SIM PUK",
            Self::Invalid => "INVALID",
            Self::Other(value) => value,
        }
    }
}

impl FromStr for SimState {
    type Err = Infallible;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(if value.eq_ignore_ascii_case("READY") {
            Self::Ready
        } else if value.eq_ignore_ascii_case("SIM PIN") {
            Self::PinRequired
        } else if value.eq_ignore_ascii_case("SIM PUK") {
            Self::PukRequired
        } else if value.eq_ignore_ascii_case("INVALID") {
            Self::Invalid
        } else {
            Self::Other(value.into())
        })
    }
}

impl fmt::Display for SimState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A cellular frequency-band identifier.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RadioBand {
    /// A numeric band used by a non-NR radio technology.
    Number(u16),
    /// A 5G NR band represented on the wire with an `n` prefix.
    Nr(u16),
    /// A band representation not recognized by this crate version.
    Other(Box<str>),
}

impl FromStr for RadioBand {
    type Err = Infallible;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if let Ok(number) = value.parse() {
            return Ok(Self::Number(number));
        }
        if let Some(number) = value
            .strip_prefix(['n', 'N'])
            .and_then(|number| number.parse().ok())
        {
            return Ok(Self::Nr(number));
        }
        Ok(Self::Other(value.into()))
    }
}

impl<'de> Deserialize<'de> for RadioBand {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(FromStrVisitor::<Self>::new("radio band"))
    }
}

impl fmt::Display for RadioBand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Number(number) => number.fmt(formatter),
            Self::Nr(number) => write!(formatter, "n{number}"),
            Self::Other(value) => formatter.write_str(value),
        }
    }
}

/// Carrier bandwidth in megahertz.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(transparent)]
pub struct CarrierBandwidth(f32);

impl CarrierBandwidth {
    /// Creates a carrier bandwidth measured in megahertz.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidCarrierBandwidth`] when `megahertz` is zero, negative,
    /// NaN, or infinite.
    pub const fn from_megahertz(megahertz: f32) -> Result<Self, InvalidCarrierBandwidth> {
        if megahertz.is_finite() && megahertz > 0.0 {
            Ok(Self(megahertz))
        } else {
            Err(InvalidCarrierBandwidth)
        }
    }

    /// Returns the bandwidth in megahertz.
    #[must_use]
    pub const fn as_megahertz(self) -> f32 {
        self.0
    }
}

impl TryFrom<f32> for CarrierBandwidth {
    type Error = InvalidCarrierBandwidth;

    fn try_from(value: f32) -> Result<Self, Self::Error> {
        Self::from_megahertz(value)
    }
}

/// Error returned for a non-positive or non-finite carrier bandwidth.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("carrier bandwidth must be positive and finite")]
pub struct InvalidCarrierBandwidth;

/// A verified mobile/LTE interface and its modem-specific status.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct MobileInterface {
    interface: Interface,
    status: MobileStatus,
}

impl MobileInterface {
    /// Returns the common interface data.
    #[must_use]
    pub const fn interface(&self) -> &Interface {
        &self.interface
    }

    /// Returns mobile/LTE-specific data.
    #[must_use]
    pub const fn status(&self) -> &MobileStatus {
        &self.status
    }

    /// Consumes the value into its common and LTE-specific parts.
    #[must_use]
    pub fn into_parts(self) -> (Interface, MobileStatus) {
        (self.interface, self.status)
    }
}

impl Deref for MobileInterface {
    type Target = Interface;

    fn deref(&self) -> &Self::Target {
        self.interface()
    }
}

impl<'de> Deserialize<'de> for MobileInterface {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(flatten)]
            interface: Interface,
            #[serde(flatten)]
            status: MobileStatus,
        }

        let wire = Wire::deserialize(deserializer)?;
        if !wire.interface.is_mobile_broadband() {
            return Err(serde::de::Error::custom(
                "the interface is not marked with a mobile/LTE trait",
            ));
        }
        Ok(Self {
            interface: wire.interface,
            status: wire.status,
        })
    }
}

/// Mobile network, SIM, radio, cell, and carrier data for an LTE interface.
///
/// Fields are optional because the router omits them before modem detection and
/// reports some unavailable values as empty strings. Empty strings are decoded
/// as `None`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[non_exhaustive]
pub struct MobileStatus {
    /// Physical modem presence/status as reported by the router.
    #[serde(default, deserialize_with = "optional_nonempty_string")]
    pub plugged: Option<Box<str>>,
    /// Modem connection state.
    #[serde(
        rename = "connection-state",
        default,
        deserialize_with = "optional_from_str"
    )]
    pub connection_state: Option<MobileConnectionState>,
    /// Selected mobile operator.
    #[serde(default, deserialize_with = "optional_nonempty_string")]
    pub operator: Option<Box<str>>,
    /// Selected APN.
    #[serde(default, deserialize_with = "optional_nonempty_string")]
    pub apn: Option<Box<str>>,
    /// SIM status as reported by the modem integration.
    #[serde(rename = "sim", default, deserialize_with = "optional_from_str")]
    pub sim_state: Option<SimState>,
    /// Remaining or current PIN-attempt counter, when supplied.
    #[serde(rename = "pin-attempts")]
    pub pin_attempts: Option<u8>,
    /// Whether the modem is built into the router.
    pub embedded: Option<bool>,
    /// Whether the modem reports roaming.
    pub roaming: Option<bool>,
    /// International Mobile Equipment Identity reported by the modem.
    #[serde(default, deserialize_with = "optional_reported")]
    pub imei: Option<Reported<Imei>>,
    /// International Mobile Subscriber Identity reported by the SIM.
    #[serde(default, deserialize_with = "optional_reported")]
    pub imsi: Option<Reported<Imsi>>,
    /// Integrated Circuit Card Identifier reported by the SIM.
    #[serde(default, deserialize_with = "optional_reported")]
    pub iccid: Option<Reported<Iccid>>,
    /// Subscriber phone number reported by the modem, when available.
    #[serde(
        rename = "phone-number",
        default,
        deserialize_with = "optional_nonempty_string"
    )]
    pub phone_number: Option<Box<str>>,
    /// Signal measurements.
    #[serde(flatten)]
    pub signal: MobileSignal,
    /// Modem temperature, degrees Celsius.
    #[serde(
        rename = "temperature",
        default,
        deserialize_with = "optional_measurement"
    )]
    pub modem_temperature: Option<Celsius>,
    /// Serving cell identifiers.
    #[serde(flatten)]
    pub cell: ServingCell,
    /// Primary serving-carrier parameters, represented by top-level RCI fields.
    #[serde(flatten)]
    pub primary_carrier: ComponentCarrier,
    /// Component carriers reported in the nested `carrier` map.
    ///
    /// Depending on the modem integration, this map can either contain only
    /// secondary carriers or repeat the primary carrier from the top-level
    /// fields. Consumers that need distinct primary and secondary carriers
    /// should normalize the entries using their radio identifiers.
    #[serde(rename = "carrier", default)]
    pub reported_carriers: BTreeMap<Box<str>, ComponentCarrier>,
    /// Modem manufacturer/model/firmware information.
    #[serde(rename = "ati")]
    pub modem: Option<LteModem>,
    /// Non-identifying SIM/provider information.
    #[serde(rename = "uim")]
    pub sim: Option<LteSim>,
}

/// LTE signal measurements.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[non_exhaustive]
pub struct MobileSignal {
    /// Router-defined normalized signal level.
    #[serde(rename = "signal-level")]
    pub level: Option<i64>,
    /// Received signal strength indication, dBm.
    #[serde(default, deserialize_with = "optional_measurement")]
    pub rssi: Option<Dbm>,
    /// Reference signal received power, dBm.
    #[serde(default, deserialize_with = "optional_measurement")]
    pub rsrp: Option<Dbm>,
    /// Reference signal received quality, dB.
    #[serde(default, deserialize_with = "optional_measurement")]
    pub rsrq: Option<Db>,
    /// Carrier-to-interference-plus-noise ratio, dB.
    #[serde(default, deserialize_with = "optional_measurement")]
    pub cinr: Option<Db>,
}

/// Serving-cell identifiers exposed by the modem integration.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct ServingCell {
    /// Public land mobile network identifier.
    #[serde(default, deserialize_with = "optional_plmn")]
    pub plmn: Option<Plmn>,
    /// eNodeB identifier.
    #[serde(
        rename = "enb-id",
        default,
        deserialize_with = "optional_nonempty_string"
    )]
    pub enb_id: Option<Box<str>>,
    /// eNodeB sector identifier.
    #[serde(
        rename = "sector-id",
        default,
        deserialize_with = "optional_nonempty_string"
    )]
    pub sector_id: Option<Box<str>>,
    /// Tracking area code.
    #[serde(default, deserialize_with = "optional_nonempty_string")]
    pub tac: Option<Box<str>>,
}

/// Parameters for a primary or additional LTE component carrier.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[non_exhaustive]
pub struct ComponentCarrier {
    /// Whether an additional component carrier is active.
    pub active: Option<bool>,
    /// Radio access technology such as the observed `4G` value.
    #[serde(rename = "mobile", default, deserialize_with = "optional_from_str")]
    pub network: Option<RadioAccessTechnology>,
    /// Cellular frequency-band identifier.
    #[serde(default, deserialize_with = "optional_from_str")]
    pub band: Option<RadioBand>,
    /// E-UTRA absolute radio-frequency channel number.
    pub earfcn: Option<u64>,
    /// Downlink frequency in router-defined units.
    #[serde(rename = "dl-freq")]
    pub downlink_frequency: Option<u64>,
    /// Uplink frequency in router-defined units.
    #[serde(rename = "ul-freq")]
    pub uplink_frequency: Option<u64>,
    /// Carrier bandwidth in megahertz.
    #[serde(default, deserialize_with = "optional_measurement")]
    pub bandwidth: Option<CarrierBandwidth>,
    /// Physical cell identifier for this component carrier.
    #[serde(
        rename = "phy-cell-id",
        default,
        deserialize_with = "optional_nonempty_string"
    )]
    pub physical_cell_id: Option<Box<str>>,
}

/// Non-identifying modem information from the RCI `ati` object.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct LteModem {
    /// Modem manufacturer.
    #[serde(default, deserialize_with = "optional_nonempty_string")]
    pub manufacturer: Option<Box<str>>,
    /// Modem model.
    #[serde(default, deserialize_with = "optional_nonempty_string")]
    pub model: Option<Box<str>>,
    /// Modem firmware revision.
    #[serde(default, deserialize_with = "optional_nonempty_string")]
    pub revision: Option<Box<str>>,
}

/// Non-identifying information from the RCI `uim` object.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct LteSim {
    /// SIM service-provider name.
    #[serde(rename = "spn", default, deserialize_with = "optional_nonempty_string")]
    pub service_provider: Option<Box<str>>,
}

#[cfg(test)]
mod tests {
    use super::{
        CarrierBandwidth, ComponentCarrier, MobileConnectionState, MobileStatus,
        RadioAccessTechnology, RadioBand, SimState,
    };

    #[test]
    fn mobile_status_parses_known_unknown_and_empty_connection_states() {
        let connected: MobileStatus =
            serde_json::from_str(r#"{"connection-state":"Connected"}"#).unwrap();
        assert_eq!(
            connected.connection_state,
            Some(MobileConnectionState::Connected)
        );

        let initializing: MobileStatus =
            serde_json::from_str(r#"{"connection-state":"Initializing"}"#).unwrap();
        assert_eq!(
            initializing.connection_state,
            Some(MobileConnectionState::Initializing)
        );

        let unknown: MobileStatus =
            serde_json::from_str(r#"{"connection-state":"Searching"}"#).unwrap();
        assert_eq!(
            unknown.connection_state,
            Some(MobileConnectionState::Other("Searching".into()))
        );

        let empty: MobileStatus = serde_json::from_str(r#"{"connection-state":""}"#).unwrap();
        assert_eq!(empty.connection_state, None);
    }

    #[test]
    fn mobile_status_parses_known_unknown_and_empty_sim_states() {
        let ready: MobileStatus = serde_json::from_str(r#"{"sim":"READY"}"#).unwrap();
        assert_eq!(ready.sim_state, Some(SimState::Ready));

        let pin: MobileStatus = serde_json::from_str(r#"{"sim":"SIM PIN"}"#).unwrap();
        assert_eq!(pin.sim_state, Some(SimState::PinRequired));

        let puk: MobileStatus = serde_json::from_str(r#"{"sim":"SIM PUK"}"#).unwrap();
        assert_eq!(puk.sim_state, Some(SimState::PukRequired));

        let invalid: MobileStatus = serde_json::from_str(r#"{"sim":"INVALID"}"#).unwrap();
        assert_eq!(invalid.sim_state, Some(SimState::Invalid));

        let unknown: MobileStatus = serde_json::from_str(r#"{"sim":"BUSY"}"#).unwrap();
        assert_eq!(unknown.sim_state, Some(SimState::Other("BUSY".into())));

        let empty: MobileStatus = serde_json::from_str(r#"{"sim":""}"#).unwrap();
        assert_eq!(empty.sim_state, None);
    }

    #[test]
    fn component_carrier_parses_known_unknown_and_empty_classifiers() {
        let known: ComponentCarrier =
            serde_json::from_str(r#"{"mobile":"4G+","band":"n78"}"#).unwrap();
        assert_eq!(known.network, Some(RadioAccessTechnology::G4Plus));
        assert_eq!(known.band, Some(RadioBand::Nr(78)));

        let lte: ComponentCarrier = serde_json::from_str(r#"{"mobile":"4G","band":"7"}"#).unwrap();
        assert_eq!(lte.network, Some(RadioAccessTechnology::G4));
        assert_eq!(lte.band, Some(RadioBand::Number(7)));

        let unknown: ComponentCarrier =
            serde_json::from_str(r#"{"mobile":"6G","band":"satellite"}"#).unwrap();
        assert_eq!(
            unknown.network,
            Some(RadioAccessTechnology::Other("6G".into()))
        );
        assert_eq!(unknown.band, Some(RadioBand::Other("satellite".into())));

        let empty: ComponentCarrier = serde_json::from_str(r#"{"mobile":"","band":""}"#).unwrap();
        assert_eq!(empty.network, None);
        assert_eq!(empty.band, None);
    }

    #[test]
    fn component_carrier_parses_bandwidth_as_a_number() {
        let string: ComponentCarrier = serde_json::from_str(r#"{"bandwidth":"20"}"#).unwrap();
        assert_eq!(
            string.bandwidth.map(CarrierBandwidth::as_megahertz),
            Some(20.0)
        );

        let number: ComponentCarrier = serde_json::from_str(r#"{"bandwidth":1.4}"#).unwrap();
        assert_eq!(
            number.bandwidth.map(CarrierBandwidth::as_megahertz),
            Some(1.4)
        );

        let empty: ComponentCarrier = serde_json::from_str(r#"{"bandwidth":""}"#).unwrap();
        assert_eq!(empty.bandwidth, None);
        assert!(serde_json::from_str::<ComponentCarrier>(r#"{"bandwidth":0}"#).is_err());
        assert!(serde_json::from_str::<ComponentCarrier>(r#"{"bandwidth":-1}"#).is_err());
    }
}
