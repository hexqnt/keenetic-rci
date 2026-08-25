use serde::Deserializer;
use thiserror::Error;

use crate::model::optional_f32;

/// A signal-power measurement in decibel-milliwatts.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(transparent)]
pub struct Dbm(f32);

/// A relative signal measurement in decibels.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(transparent)]
pub struct Db(f32);

/// A temperature measurement in degrees Celsius.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(transparent)]
pub struct Celsius(f32);

/// Error returned when a numeric measurement is not finite.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("a measurement must be finite")]
pub struct InvalidMeasurement;

macro_rules! impl_measurement {
    ($type:ty) => {
        impl $type {
            /// Creates a finite numeric measurement.
            ///
            /// # Errors
            ///
            /// Returns [`InvalidMeasurement`] for NaN or infinity.
            pub const fn new(value: f32) -> Result<Self, InvalidMeasurement> {
                if value.is_finite() {
                    Ok(Self(value))
                } else {
                    Err(InvalidMeasurement)
                }
            }

            /// Returns the numeric measurement value.
            #[must_use]
            pub const fn get(self) -> f32 {
                self.0
            }
        }

        impl TryFrom<f32> for $type {
            type Error = InvalidMeasurement;

            fn try_from(value: f32) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }
    };
}

impl_measurement!(Dbm);
impl_measurement!(Db);
impl_measurement!(Celsius);

pub(super) fn optional_measurement<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: TryFrom<f32>,
    T::Error: std::fmt::Display,
{
    optional_f32(deserializer)?
        .map(T::try_from)
        .transpose()
        .map_err(serde::de::Error::custom)
}

pub(super) fn measurement<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: TryFrom<f32>,
    T::Error: std::fmt::Display,
{
    optional_f32(deserializer)?
        .ok_or_else(|| serde::de::Error::custom("missing measurement"))
        .and_then(|value| T::try_from(value).map_err(serde::de::Error::custom))
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::{Celsius, Db, Dbm, InvalidMeasurement, measurement, optional_measurement};

    #[test]
    fn measurements_require_finite_values() {
        assert_eq!(
            Dbm::new(-51.5).unwrap().get().to_bits(),
            (-51.5_f32).to_bits()
        );
        assert_eq!(Dbm::new(f32::NAN), Err(InvalidMeasurement));
        assert_eq!(Db::try_from(f32::INFINITY), Err(InvalidMeasurement));
        assert_eq!(Celsius::new(f32::NEG_INFINITY), Err(InvalidMeasurement));
    }

    #[test]
    fn deserialization_rejects_non_finite_measurements() {
        #[derive(Deserialize)]
        struct Signal {
            #[serde(deserialize_with = "measurement")]
            value: Dbm,
        }

        #[derive(Deserialize)]
        struct OptionalSignal {
            #[serde(default, deserialize_with = "optional_measurement")]
            value: Option<Dbm>,
        }

        assert!(serde_json::from_str::<Signal>(r#"{"value":"NaN"}"#).is_err());
        assert!(serde_json::from_str::<Signal>(r#"{"value":1.7976931348623157e308}"#).is_err());
        assert!(serde_json::from_str::<OptionalSignal>(r#"{"value":"NaN"}"#).is_err());
        assert!(
            serde_json::from_str::<OptionalSignal>("{}")
                .unwrap()
                .value
                .is_none()
        );
        assert_eq!(
            serde_json::from_str::<Signal>(r#"{"value":-79}"#)
                .unwrap()
                .value
                .get()
                .to_bits(),
            (-79.0_f32).to_bits()
        );
    }
}
