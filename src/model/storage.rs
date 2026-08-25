use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Deserializer, de};
use thiserror::Error;

use crate::model::{
    network::{
        ByteCount, deserialize_optional_u8_string_or_number, deserialize_u64_string_or_number,
    },
    optional_nonempty_string,
};

string_identifier!(
    UsbDeviceId,
    InvalidInventoryId,
    "A stable key in the USB inventory."
);
string_identifier!(
    MediaId,
    InvalidInventoryId,
    "A stable key in the media inventory."
);
string_identifier!(
    PartitionId,
    InvalidInventoryId,
    "A stable key in a media partition map."
);
string_identifier!(
    VolumeId,
    InvalidVolumeId,
    "A filesystem UUID or vendor-specific volume identifier."
);

/// An invalid storage inventory identifier.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum InvalidInventoryId {
    /// The identifier was empty.
    #[error("an inventory identifier must not be empty")]
    Empty,
    /// The identifier contained a control character.
    #[error("an inventory identifier must not contain control characters")]
    ControlCharacter,
}

/// An invalid filesystem or vendor-specific volume identifier.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum InvalidVolumeId {
    /// The identifier was empty.
    #[error("a volume identifier must not be empty")]
    Empty,
    /// The identifier contained a control character.
    #[error("a volume identifier must not contain control characters")]
    ControlCharacter,
}

/// USB device subsystem.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum UsbSubsystem {
    /// Network or modem device.
    Network,
    /// Storage device or partition.
    Storage,
    /// Printer device.
    Printer,
    /// A subsystem introduced by another `KeeneticOS` release.
    Other(Box<str>),
}

/// USB specification revision reported by a device descriptor.
#[allow(non_camel_case_types)]
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum UsbVersion {
    /// USB 1.0.
    V1_0,
    /// USB 1.1.
    V1_1,
    /// USB 2.0.
    V2_0,
    /// USB 2.01.
    V2_01,
    /// USB 3.0.
    V3_0,
    /// USB 3.1.
    V3_1,
    /// USB 3.2.
    V3_2,
    /// A revision not recognized by this crate version.
    Other(Box<str>),
}

/// USB devices returned by `show/usb`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct UsbDevices {
    /// Devices keyed by router identifier.
    #[serde(rename = "device", default)]
    devices: BTreeMap<UsbDeviceId, UsbDevice>,
}

impl UsbDevices {
    /// Iterates over devices in lexical key order.
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&UsbDeviceId, &UsbDevice)> {
        self.devices.iter()
    }
}

impl_map_collection!(
    UsbDevices,
    UsbDeviceId,
    UsbDevice,
    devices,
    "a device",
    "the device map"
);

/// One attached USB device.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct UsbDevice {
    /// Kernel parent-device path.
    #[serde(rename = "DEVICE")]
    pub device_path: Option<Box<str>>,
    /// Kernel device path.
    #[serde(rename = "DEVPATH")]
    pub dev_path: Option<Box<str>>,
    /// User-facing alias, preserving an explicitly reported empty value.
    pub alias: Option<Box<str>>,
    /// User-facing storage label.
    #[serde(default, deserialize_with = "optional_nonempty_string")]
    pub label: Option<Box<str>>,
    /// Whether the device has no controllable LED.
    #[serde(
        rename = "no-led",
        default,
        deserialize_with = "deserialize_presence_marker"
    )]
    pub no_led: bool,
    /// Physical USB port number.
    #[serde(default, deserialize_with = "deserialize_optional_u8_string_or_number")]
    pub port: Option<u8>,
    /// Power-control capability.
    #[serde(rename = "power-control")]
    pub power_control: Option<UsbPowerControl>,
    /// Device subsystem.
    pub subsystem: UsbSubsystem,
    /// USB specification revision reported by the device.
    #[serde(rename = "usb-version")]
    pub usb_version: Option<UsbVersion>,
}

open_string_enum!(UsbSubsystem {
    Network => "network",
    Storage => "storage",
    Printer => "printer",
});

open_string_enum!(UsbVersion {
    V1_0 => "1.00",
    V1_1 => "1.10",
    V2_0 => "2.00",
    V2_01 => "2.01",
    V3_0 => "3.00",
    V3_1 => "3.10",
    V3_2 => "3.20",
});

/// Router-reported USB power-control setting.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum UsbPowerControl {
    /// Power control is available or enabled.
    Enabled,
    /// Power control is unavailable or disabled.
    Disabled,
    /// A representation introduced by another `KeeneticOS` release.
    Other(Box<str>),
}

open_string_enum!(UsbPowerControl {
    Enabled => "yes",
    Disabled => "no",
});

/// Storage bus.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum MediaBus {
    /// USB mass storage.
    Usb,
    /// Internal flash exposed by the MTD subsystem.
    Mtd,
    /// A bus introduced by another `KeeneticOS` release.
    Other(Box<str>),
}

/// Storage media returned by `show/media`, keyed by media identifier.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(transparent)]
pub struct MediaInventory(BTreeMap<MediaId, MediaDevice>);

impl MediaInventory {
    /// Iterates over media in lexical key order.
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&MediaId, &MediaDevice)> {
        self.0.iter()
    }
}

impl_map_collection!(
    MediaInventory,
    MediaId,
    MediaDevice,
    0,
    "a medium",
    "the media map"
);

/// One storage medium and its partitions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct MediaDevice {
    /// Storage bus, when reported.
    pub bus: Option<MediaBus>,
    /// Physical USB port, when applicable.
    #[serde(default, deserialize_with = "deserialize_optional_u8_string_or_number")]
    pub port: Option<u8>,
    /// Whether the medium can be ejected in software.
    pub ejectable: Option<bool>,
    /// Whether the medium is removable.
    pub removable: Option<bool>,
    /// Current medium state.
    pub state: MediaState,
    /// Manufacturer when non-empty.
    #[serde(default, deserialize_with = "optional_nonempty_string")]
    pub manufacturer: Option<Box<str>>,
    /// Product identifier when non-empty.
    #[serde(default, deserialize_with = "optional_nonempty_string")]
    pub product: Option<Box<str>>,
    /// Device serial when non-empty.
    #[serde(default, deserialize_with = "optional_nonempty_string")]
    pub serial: Option<Box<str>>,
    /// Filesystems supported when initializing this medium.
    #[serde(rename = "initialize-supported", default)]
    pub initialize_supported: Box<[FileSystemType]>,
    /// Total medium size.
    #[serde(deserialize_with = "deserialize_byte_count")]
    pub size: ByteCount,
    /// Partitions keyed by router identifier.
    #[serde(rename = "partition", default)]
    pub partitions: BTreeMap<PartitionId, MediaPartition>,
}

/// One storage partition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct MediaPartition {
    /// Partition identifier repeated in the payload, when present.
    pub id: Option<PartitionId>,
    /// Filesystem UUID or vendor-specific volume identifier.
    #[serde(default, deserialize_with = "deserialize_optional_volume_id")]
    pub uuid: Option<VolumeId>,
    /// User-facing volume label.
    #[serde(default, deserialize_with = "optional_nonempty_string")]
    pub label: Option<Box<str>>,
    /// Filesystem type.
    #[serde(rename = "fstype")]
    pub file_system: FileSystemType,
    /// Whether a filesystem check is supported.
    #[serde(rename = "check-supported")]
    pub check_supported: Option<bool>,
    /// Supported formatting filesystems.
    #[serde(rename = "format-supported", default)]
    pub format_supported: Box<[FileSystemType]>,
    /// Current partition state.
    pub state: MediaState,
    /// Total filesystem size.
    #[serde(deserialize_with = "deserialize_byte_count")]
    pub total: ByteCount,
    /// Free filesystem space.
    #[serde(deserialize_with = "deserialize_byte_count")]
    pub free: ByteCount,
    /// Services currently using the partition.
    #[serde(rename = "used-by", default)]
    pub used_by: Box<[Box<str>]>,
}

open_string_enum!(MediaBus {
    Usb => "usb",
    Mtd => "mtd",
});

/// Medium or partition state.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum MediaState {
    /// Medium is active.
    Active,
    /// Partition is mounted.
    Mounted,
    /// Partition is not mounted.
    Unmounted,
    /// Medium or partition is unavailable because of an error.
    Error,
    /// A state introduced by another `KeeneticOS` release.
    Other(Box<str>),
}

open_string_enum!(MediaState {
    Active => "ACTIVE",
    Mounted => "MOUNTED",
    Unmounted => "UNMOUNTED",
    Error => "ERROR",
});

/// Filesystem classifier reported by `KeeneticOS`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum FileSystemType {
    /// Linux ext4.
    Ext4,
    /// Microsoft NTFS.
    Ntfs,
    /// FAT32.
    Fat32,
    /// exFAT.
    ExFat,
    /// UBI filesystem used on internal flash.
    Ubifs,
    /// Swap partition.
    Swap,
    /// A filesystem introduced by another `KeeneticOS` release.
    Other(Box<str>),
}

open_string_enum!(FileSystemType {
    Ext4 => "ext4",
    Ntfs => "ntfs",
    Fat32 => "fat32",
    ExFat => "exfat",
    Ubifs => "ubifs",
    Swap => "swap",
});

fn deserialize_byte_count<'de, D>(deserializer: D) -> Result<ByteCount, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_u64_string_or_number(deserializer).map(ByteCount::new)
}

fn deserialize_optional_volume_id<'de, D>(deserializer: D) -> Result<Option<VolumeId>, D::Error>
where
    D: Deserializer<'de>,
{
    optional_nonempty_string::<_, Box<str>>(deserializer)?
        .map(VolumeId::try_from)
        .transpose()
        .map_err(de::Error::custom)
}

fn deserialize_presence_marker<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    struct PresenceMarkerVisitor;

    impl de::Visitor<'_> for PresenceMarkerVisitor {
        type Value = bool;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an empty string presence marker")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if value.is_empty() {
                Ok(true)
            } else {
                Err(E::invalid_value(de::Unexpected::Str(value), &self))
            }
        }
    }

    deserializer.deserialize_str(PresenceMarkerVisitor)
}

#[cfg(test)]
mod tests {
    use super::{
        FileSystemType, InvalidInventoryId, MediaId, MediaInventory, MediaState, UsbDevices,
        UsbPowerControl, UsbVersion, VolumeId,
    };

    #[test]
    fn parses_empty_and_mixed_usb_inventories() {
        let empty: UsbDevices = serde_json::from_str("{}").unwrap();
        let devices: UsbDevices =
            serde_json::from_str(include_str!("../../tests/fixtures/show_usb.json")).unwrap();

        assert!(empty.is_empty());
        assert_eq!(devices.len(), 2);
        let modem = devices.get("UsbLte0").unwrap();
        assert_eq!(modem.port, Some(1));
        assert_eq!(modem.power_control, Some(UsbPowerControl::Enabled));
        assert_eq!(modem.usb_version, Some(UsbVersion::V3_0));
        assert!(modem.no_led);
        let volume = devices.get("FixtureVolume").unwrap();
        assert_eq!(volume.usb_version, Some(UsbVersion::V2_0));
        assert!(!volume.no_led);
        assert_eq!((&devices).into_iter().count(), 2);
    }

    #[test]
    fn preserves_unknown_usb_versions() {
        let response = r#"{"device":{"Usb0":{"subsystem":"network","usb-version":"4.00"}}}"#;
        let devices: UsbDevices = serde_json::from_str(response).unwrap();

        assert!(matches!(
            &devices["Usb0"].usb_version,
            Some(UsbVersion::Other(version)) if version.as_ref() == "4.00"
        ));
    }

    #[test]
    fn rejects_nonempty_usb_presence_marker() {
        let response = r#"{"device":{"Usb0":{"no-led":"unexpected","subsystem":"network"}}}"#;

        assert!(serde_json::from_str::<UsbDevices>(response).is_err());
    }

    #[test]
    fn parses_media_byte_counts_partitions_and_open_filesystems() {
        let media: MediaInventory =
            serde_json::from_str(include_str!("../../tests/fixtures/show_media.json")).unwrap();

        assert_eq!(media.len(), 2);
        let disk = media.get("Media0").unwrap();
        assert_eq!(disk.size.get(), 1_000_202_043_392);
        assert!(matches!(
            &disk.initialize_supported[1],
            FileSystemType::Other(value) if value.as_ref() == "future-fs"
        ));
        assert_eq!(
            disk.partitions["Partition1"]
                .uuid
                .as_ref()
                .map(VolumeId::as_str),
            Some("11111111-2222-3333-4444-555555555555")
        );
        let swap = &disk.partitions["Partition2"];
        assert_eq!(swap.uuid, None);
        assert_eq!(swap.label, None);
        assert_eq!(swap.state, MediaState::Unmounted);
        assert_eq!(swap.free.get(), 1_081_077_760);
        assert!(media.get("FlashStorage").unwrap().partitions.is_empty());
        assert_eq!((&media).into_iter().count(), 2);
    }

    #[test]
    fn inventory_identifiers_parse_and_serialize_as_validated_strings() {
        let id: MediaId = "Media0".parse().unwrap();

        assert_eq!(id.as_ref(), "Media0");
        assert_eq!(serde_json::to_string(&id).unwrap(), r#""Media0""#);
        assert_eq!("".parse::<MediaId>(), Err(InvalidInventoryId::Empty));
    }
}
