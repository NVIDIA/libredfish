/*
 * SPDX-FileCopyrightText: Copyright (c) 2023 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
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

use crate::{
    model::{BiosCommon, ODataId},
    EnabledDisabled,
};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Manager {
    pub agentless_capabilities: Vec<String>,

    #[serde(rename = "KCSEnabled")]
    pub kcs_enabled: bool,

    pub recipients_settings: RecipientSettings,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct RecipientSettings {
    pub retry_count: i64,
    pub retry_interval: f64,
    pub rntry_retry_interval: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct System {
    pub scheduled_power_actions: ODataId,
    #[serde(rename = "FrontPanelUSB")]
    pub front_panel_usb: FrontPanelUSB,
    pub metrics: ODataId,
    pub system_status: String,
    pub number_of_reboots: i64,
    pub history_sys_perf: ODataId,
    #[serde(rename = "@odata.type")]
    pub odata_type: String,
    pub total_power_on_hours: i64,
    pub sensors: ODataId,
    pub boot_settings: ODataId,
}

/* Front Panel USB Port Management mapping from UI to redfish API:

 - UI: Host Only Mode
    The front panel USB port is always connected only to the server.
   API: fp_mode=Server, port_switching_to=Server

 - UI: BMC Only Mode
    The front panel USB port is always connected only to the XClarity Controller.
   API: fp_mode=BMC, port_switching_to=BMC

 - UI: Shared Mode: owned by BMC
    The front panel USB port is shared by both the server and the XClarity Controller, but the port is switched to the XClarity Controller.
   API: fp_mode=Shared, port_switching_to=BMC

 - UI: Shared Mode: owned by Host
    The front panel USB port is shared by both the server and the XClarity Controller, but the port is switched to the host.
   API: fp_mode=Shared, port_switching_to=Server
*/

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct FrontPanelUSB {
    inactivity_timeout_mins: i64,
    #[serde(rename = "IDButton")]
    id_button: String,
    pub port_switching_to: PortSwitchingMode,
    #[serde(rename = "FPMode")]
    pub fp_mode: FrontPanelUSBMode,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum PortSwitchingMode {
    BMC,
    Server,
}

impl fmt::Display for PortSwitchingMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BMC => f.write_str("BMC"),
            Self::Server => f.write_str("Server"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum FrontPanelUSBMode {
    Server,
    Shared,
    BMC,
}

impl fmt::Display for FrontPanelUSBMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Server => f.write_str("Server"),
            Self::Shared => f.write_str("Shared"),
            Self::BMC => f.write_str("BMC"),
        }
    }
}

impl FromStr for FrontPanelUSBMode {
    type Err = FrontPanelUSBModeParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Server" => Ok(Self::Server),
            "Shared" => Ok(Self::Shared),
            "BMC" => Ok(Self::BMC),
            x => Err(FrontPanelUSBModeParseError(format!(
                "Invalid FrontPanelUSBMode value: {x}"
            ))),
        }
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct FrontPanelUSBModeParseError(String);

impl fmt::Display for FrontPanelUSBModeParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[derive(Debug, Copy, Clone)]
// I think this is actually a string (e.g. "ubuntu" is valid), and there are more variants.
// We only use these two, so use typing checking.
pub enum BootOptionName {
    HardDisk,
    Network,
}

impl fmt::Display for BootOptionName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[derive(Debug, Deserialize, Serialize, Copy, Clone)]
pub enum BootSource {
    None,
    Pxe,
    Cd,
    Usb,
    Hdd,
    BiosSetup,
    Diags,
    UefiTarget,
}

impl fmt::Display for BootSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// Attributes part of response from Lenovo server for Systems/:id/Bios
/// There are many more attributes, see tests/bios_lenovo.json
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BiosAttributes {
    #[serde(flatten)]
    pub tpm: BiosAttributesTPM,
    #[serde(flatten)]
    pub processors: BiosAttributesProcessors,

    #[serde(rename = "Memory_MirrorMode")]
    pub memory_mirror_mode: EnabledDisabled,

    #[serde(rename = "LegacyBIOS_LegacyBIOS")]
    pub legacy_bios: EnabledDisabled,

    #[serde(rename = "BootModes_SystemBootMode")]
    pub boot_modes_system_boot_mode: BootMode,

    #[serde(rename = "SecureBootConfiguration_SecureBootStatus")]
    pub secure_boot_configuration_secure_boot_status: EnabledDisabled,
    #[serde(rename = "SecureBootConfiguration_SecureBootSetting")]
    pub secure_boot_configuration_secure_boot_setting: EnabledDisabled,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum BootMode {
    UEFIMode,
    LegacyMode,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BiosAttributesProcessors {
    #[serde(rename = "Processors_CPUPstateControl")]
    pub cpu_state_control: String,
    #[serde(rename = "Processors_AdjacentCachePrefetch")]
    pub adjacent_cache_prefetch: String,
    #[serde(rename = "Processors_HyperThreading")]
    pub hyper_threading: String,
    #[serde(rename = "Processors_IntelVirtualizationTechnology")]
    pub intel_virtualization_technology: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BiosAttributesTPM {
    #[serde(rename = "TrustedComputingGroup_DeviceOperation")]
    pub device_operation: TPMOperation,
    #[serde(rename = "TrustedComputingGroup_SHA_1PCRBank")]
    pub sha1_pcrbank: EnabledDisabled,
    #[serde(rename = "TrustedComputingGroup_DeviceStatus")]
    pub device_status: String, // "TPM2.0 Device present."
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
pub enum TPMOperation {
    None,
    UpdateToTPM2_0FirmwareVersion7_2_2_0,
    Clear, // reset
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Bios {
    #[serde(flatten)]
    pub common: BiosCommon,
    pub attributes: BiosAttributes,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct LenovoBootOrder {
    pub boot_order_current: Vec<String>,
    pub boot_order_next: Vec<String>,
    pub boot_order_supported: Vec<String>,
}

#[cfg(test)]
mod test {
    #[test]
    fn test_bios_parser_lenovo() {
        let test_data = include_str!("../testdata/bios_lenovo.json");
        let result: super::Bios = serde_json::from_str(test_data).unwrap();
        println!("result: {result:#?}");
    }
}
