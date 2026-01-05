/*
 * SPDX-FileCopyrightText: Copyright (c) 2025 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
 * SPDX-License-Identifier: MIT
 *
 * Permission is hereby granted, free of charge, to any person obtaining a
 * copy of this software and associated documentation files (the "Software"),
 * to deal in the Software without restriction, including without limitation
 * the rights to use, copy, modify, merge, publish, distribute, sublicense,
 * and/or sell copies of the Software, and to permit persons to whom the
 * Software is furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL
 * THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
 * FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
 * DEALINGS IN THE SOFTWARE.
 */
use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
pub mod manager;
pub mod resource;
pub use manager::*;
pub use resource::OData;
pub mod serial_interface;

pub mod system;
pub use system::*;

pub mod bios;
pub mod boot;
pub use bios::*;

use crate::RedfishError;

pub mod oem;
pub mod secure_boot;

pub mod account_service;
pub mod certificate;
pub mod chassis;
pub mod component_integrity;
pub mod error;
pub mod ethernet_interface;
pub mod job;
pub mod manager_network_protocol;
pub mod network_device_function;
pub mod port;
pub mod power;
pub mod sel;
pub mod sensor;
pub mod service_root;
pub mod software_inventory;
pub mod storage;
pub mod task;
pub mod thermal;
pub mod update_service;

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct ODataLinks {
    #[serde(rename = "@odata.context")]
    pub odata_context: Option<String>,
    #[serde(rename = "@odata.id")]
    pub odata_id: String,
    #[serde(rename = "@odata.type")]
    pub odata_type: String,
    #[serde(rename = "@odata.etag")]
    pub odata_etag: Option<String>,
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

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct ODataId {
    #[serde(rename = "@odata.id")]
    pub odata_id: String,
}

impl From<String> for ODataId {
    fn from(item: String) -> Self {
        ODataId { odata_id: item }
    }
}

impl From<&str> for ODataId {
    fn from(item: &str) -> Self {
        ODataId {
            odata_id: item.to_string(),
        }
    }
}

impl ODataId {
    // Gets last portion of the ID, not including uri path
    pub fn odata_id_get(&self) -> Result<&str, RedfishError> {
        self.odata_id
            .split('/')
            .next_back()
            .ok_or_else(|| RedfishError::GenericError {
                error: format!("odata_id have invalid format: {}", self.odata_id),
            })
    }
}

// This is Redfish spec defined object that is required to
// make changes to underlying resource
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct RedfishSettings {
    pub e_tag: Option<String>,
    #[serde(rename = "@odata.type")]
    pub odata_type: Option<String>,
    pub messages: Option<Vec<String>>,
    pub time: Option<String>,
    #[serde(rename = "SettingsObject")]
    pub settings_object: Option<ODataId>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ODataContext {
    #[serde(rename = "@odata.context")]
    pub odata_context: String,
    #[serde(rename = "links")]
    pub links: LinkType,
}

#[derive(Debug, Default, Serialize, Deserialize, Copy, Clone, Eq, PartialEq)]
pub enum EnabledDisabled {
    #[default]
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

impl From<EnabledDisabled> for serde_json::Value {
    fn from(val: EnabledDisabled) -> Self {
        serde_json::Value::String(val.to_string())
    }
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, Eq, PartialEq)]
pub enum EnableDisable {
    Enable,
    Disable,
}

impl EnableDisable {
    pub fn is_enabled(self) -> bool {
        self == EnableDisable::Enable
    }
}

impl fmt::Display for EnableDisable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl FromStr for EnableDisable {
    type Err = InvalidValueError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Enable" => Ok(Self::Enable),
            "Disable" => Ok(Self::Disable),
            x => Err(InvalidValueError(format!(
                "Invalid EnableDisable value: {x}"
            ))),
        }
    }
}

impl From<EnableDisable> for serde_json::Value {
    fn from(val: EnableDisable) -> Self {
        serde_json::Value::String(val.to_string())
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Copy, Clone, Eq, PartialEq)]
pub enum YesNo {
    #[default]
    Yes,
    No,
}

impl YesNo {
    pub fn is_enabled(self) -> bool {
        self == YesNo::Yes
    }
}

impl fmt::Display for YesNo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl FromStr for YesNo {
    type Err = InvalidValueError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Yes" => Ok(Self::Yes),
            "No" => Ok(Self::No),
            x => Err(InvalidValueError(format!("Invalid YesNo value: {x}"))),
        }
    }
}

impl From<YesNo> for serde_json::Value {
    fn from(val: YesNo) -> Self {
        serde_json::Value::String(val.to_string())
    }
}

#[derive(Debug)]
pub struct InvalidValueError(pub String);

impl std::error::Error for InvalidValueError {}

impl fmt::Display for InvalidValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, Eq, PartialEq, Default)]
pub enum OnOff {
    On,
    #[default]
    Off,
    Reset,
}

impl fmt::Display for OnOff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, Eq, PartialEq)]
pub enum LinkStatus {
    LinkUp,
    NoLink,
    LinkDown,
}

impl fmt::Display for LinkStatus {
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

#[derive(Default, Debug, Serialize, Deserialize, Copy, Clone)]
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
    Degraded,
    Standby,
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
    Unknown,
}

impl fmt::Display for ResourceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// https://redfish.dmtf.org/schemas/v1/Message.v1_1_2.json
/// The message that the Redfish service returns.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Message {
    pub message: String,
    #[serde(default)]
    pub message_args: Vec<String>,
    pub message_id: String,
    pub resolution: Option<String>,
    pub severity: Option<String>,
}
