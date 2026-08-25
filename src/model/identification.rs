use serde::Deserialize;
use thiserror::Error;

use crate::model::{hardware_id::HardwareId, network::MacAddress};

macro_rules! identifier_error {
    ($error:ident, $description:literal) => {
        #[doc = concat!("An invalid ", $description, ".")]
        #[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
        #[non_exhaustive]
        pub enum $error {
            /// The identifier was empty.
            #[error("the identifier must not be empty")]
            Empty,
            /// The identifier contained a control character.
            #[error("the identifier must not contain control characters")]
            ControlCharacter,
        }
    };
}

identifier_error!(InvalidServiceTag, "router service tag");
identifier_error!(InvalidSerialNumber, "router serial number");
identifier_error!(InvalidCustomerId, "router customer identifier");

string_identifier!(ServiceTag, InvalidServiceTag, "A router service tag.");
string_identifier!(SerialNumber, InvalidSerialNumber, "A router serial number.");
string_identifier!(
    CustomerId,
    InvalidCustomerId,
    "A router customer identifier."
);

/// Hardware identification returned by `show/identification`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct Identification {
    /// Router service tag.
    #[serde(rename = "servicetag")]
    pub service_tag: ServiceTag,
    /// Router serial number.
    pub serial: SerialNumber,
    /// Base MAC address.
    pub mac: MacAddress,
    /// Hardware platform identifier.
    #[serde(rename = "hwid")]
    pub hardware_id: HardwareId,
    /// Customer identifier.
    #[serde(rename = "cid")]
    pub customer_id: CustomerId,
}

#[cfg(test)]
mod tests {
    use super::{Identification, SerialNumber};

    #[test]
    fn identifiers_validate_once_and_serialize_without_changing_value() {
        let serial: SerialNumber = "SYNTHETIC0001".parse().unwrap();
        assert_eq!(
            serde_json::to_string(&serial).unwrap(),
            r#""SYNTHETIC0001""#
        );
        assert!("line\nbreak".parse::<SerialNumber>().is_err());

        serde_json::from_str::<Identification>(include_str!(
            "../../tests/fixtures/show_identification.json"
        ))
        .unwrap();
    }
}
