use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
pub mod manager;
pub use manager::*;

pub mod system;
pub use system::*;

pub mod bios;
pub mod boot;
pub use bios::*;

pub mod oem;
pub mod secure_boot;

// power/thermal/storage not currently used
pub mod power;
pub mod software_inventory;
pub mod storage;
pub mod task;
pub mod thermal;
pub mod network_device_function;
pub mod chassis;
pub mod port;
pub mod ethernet_interface;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ODataLinks {
    #[serde(rename = "@odata.context")]
    pub odata_context: Option<String>,
    #[serde(rename = "@odata.id")]
    pub odata_id: String,
    #[serde(rename = "@odata.type")]
    pub odata_type: String,
    #[serde(rename = "links")]
    pub links: Option<LinkType>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Href {
    pub href: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExtRef {
    pub extref: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged, rename_all = "PascalCase")]
pub enum LinkType {
    SelfLink {
        #[serde(rename = "self")]
        self_url: Href,
    },
    HpLink {
        fast_power_meter: Href,
        federated_group_capping: Href,
        power_meter: Href,
    },
    OemHpLink {
        active_health_system: Href,
        date_time_service: Href,
        embedded_media_service: Href,
        federation_dispatch: ExtRef,
        federation_groups: Href,
        federation_peers: Href,
        license_service: Href,
        security_service: Href,
        update_service: Href,
        #[serde(rename = "VSPLogLocation")]
        vsp_log_location: ExtRef,
    },
    SerdeJson {
        #[serde(rename = "links")]
        links: serde_json::Value,
    },
    EnclosuresLinks {
        member: Vec<Href>,
        #[serde(rename = "self")]
        self_url: Href,
    },
    ManagerLink {
        #[serde(rename = "EthernetNICs")]
        ethernet_nics: Href,
        logs: Href,
        manager_for_chassis: Vec<Href>,
        manager_for_servers: Vec<Href>,
        network_service: Href,
        virtual_media: Href,
        #[serde(rename = "self")]
        self_url: Href,
    },
    StorageLink {
        logical_drives: Href,
        physical_drives: Href,
        storage_enclosures: Href,
        unconfigured_drives: Href,
        #[serde(rename = "self")]
        self_url: Href,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ODataId {
    #[serde(rename = "@odata.id")]
    pub odata_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ODataContext {
    #[serde(rename = "@odata.context")]
    pub odata_context: String,
    #[serde(rename = "links")]
    pub links: LinkType,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, Eq, PartialEq)]
pub enum EnabledDisabled {
    Enabled,
    Disabled,
}

impl EnabledDisabled {
    pub fn is_enabled(self) -> bool {
        self == EnabledDisabled::Enabled
    }
}

impl fmt::Display for EnabledDisabled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl FromStr for EnabledDisabled {
    type Err = InvalidValueError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Enabled" => Ok(Self::Enabled),
            "Disabled" => Ok(Self::Disabled),
            x => Err(InvalidValueError(format!(
                "Invalid EnabledDisabled value: {x}"
            ))),
        }
    }
}

#[derive(Debug)]
pub struct InvalidValueError(String);

impl std::error::Error for InvalidValueError {}

impl fmt::Display for InvalidValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, Eq, PartialEq)]
pub enum OnOff {
    On,
    Off,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, Eq, PartialEq)]
pub enum LinkStatus {
    LinkUp,
    NoLink,
    LinkDown,
}

impl fmt::Display for OnOff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FirmwareCurrent {
    #[serde(rename = "VersionString")]
    pub version: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Firmware {
    pub current: FirmwareCurrent,
}

pub trait StatusVec {
    fn get_vec(&self) -> Vec<ResourceStatus>;
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct ResourceStatus {
    pub health: Option<ResourceHealth>,
    pub health_rollup: Option<ResourceHealth>,
    pub state: Option<ResourceState>,
}

/// Health and State of a disk drive, fan, power supply, etc
/// Defined in Resource_v1.xml
#[derive(Debug, Serialize, Deserialize, Copy, Clone, Default)]
pub enum ResourceHealth {
    #[serde(rename = "OK")]
    #[default]
    Ok,
    Warning,
    Critical,
    Informational, // HP only, non-standard
}

impl fmt::Display for ResourceHealth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

// Defined in Resource_v1.xml
#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
pub enum ResourceState {
    Enabled,
    Disabled,
    StandbyOffline,
    StandbySpare,
    InTest,
    Starting,
    Absent,
    UnavailableOffline,
    Deferring,
    Quiesced,
    Updating,
    Qualified,
}

impl fmt::Display for ResourceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
