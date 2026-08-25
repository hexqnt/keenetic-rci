use std::time::Duration;

use serde::Deserialize;

use crate::model::network::{ByteCount, DataRate, deserialize_decimal_duration};

/// Cumulative and current statistics for one interface.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct InterfaceStat {
    /// Received packets.
    #[serde(rename = "rxpackets")]
    pub received_packets: u64,
    /// Received multicast packets.
    #[serde(rename = "rx-multicast-packets")]
    pub received_multicast_packets: u64,
    /// Received broadcast packets.
    #[serde(rename = "rx-broadcast-packets")]
    pub received_broadcast_packets: u64,
    /// Received bytes.
    #[serde(rename = "rxbytes")]
    pub received: ByteCount,
    /// Receive errors.
    #[serde(rename = "rxerrors")]
    pub receive_errors: u64,
    /// Dropped receive packets.
    #[serde(rename = "rxdropped")]
    pub receive_dropped: u64,
    /// Transmitted packets.
    #[serde(rename = "txpackets")]
    pub transmitted_packets: u64,
    /// Transmitted multicast packets.
    #[serde(rename = "tx-multicast-packets")]
    pub transmitted_multicast_packets: u64,
    /// Transmitted broadcast packets.
    #[serde(rename = "tx-broadcast-packets")]
    pub transmitted_broadcast_packets: u64,
    /// Transmitted bytes.
    #[serde(rename = "txbytes")]
    pub transmitted: ByteCount,
    /// Transmit errors.
    #[serde(rename = "txerrors")]
    pub transmit_errors: u64,
    /// Dropped transmit packets.
    #[serde(rename = "txdropped")]
    pub transmit_dropped: u64,
    /// Router monotonic timestamp for the sample.
    #[serde(deserialize_with = "deserialize_decimal_duration")]
    pub timestamp: Duration,
    /// Router timestamp of the last counter overflow.
    #[serde(
        rename = "last-overflow",
        deserialize_with = "deserialize_decimal_duration"
    )]
    pub last_overflow: Duration,
    /// Current receive rate in bits per second, when reported.
    #[serde(default, rename = "rxspeed")]
    pub receive_rate: Option<DataRate>,
    /// Current transmit rate in bits per second, when reported.
    #[serde(default, rename = "txspeed")]
    pub transmit_rate: Option<DataRate>,
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::InterfaceStat;

    #[test]
    fn parses_counters_rates_and_fractional_timestamps() {
        let stat: InterfaceStat = serde_json::from_str(include_str!(
            "../../tests/fixtures/show_interface_stat.json"
        ))
        .unwrap();

        assert_eq!(stat.received.get(), 4_096);
        assert_eq!(stat.receive_rate.unwrap().bits_per_second(), 7_158_920);
        assert_eq!(stat.timestamp, Duration::new(76_863, 625_556_000));
    }

    #[test]
    fn accepts_interfaces_without_current_rates() {
        let mut fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/show_interface_stat.json"
        ))
        .unwrap();
        let object = fixture.as_object_mut().unwrap();
        object.remove("rxspeed");
        object.remove("txspeed");

        let stat: InterfaceStat = serde_json::from_value(fixture).unwrap();
        assert_eq!(stat.receive_rate, None);
        assert_eq!(stat.transmit_rate, None);
    }
}
