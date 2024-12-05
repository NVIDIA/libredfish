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
use serde::{Deserialize, Serialize};

use super::{Firmware, ODataId, ODataLinks, ResourceStatus};

pub trait Hardware {
    fn odata_context(&self) -> String;
    fn odata_id(&self) -> String;
    fn odata_type(&self) -> String;
    fn description(&self) -> String;
    fn firmware_version(&self) -> Firmware;
    fn id(&self) -> String;
    fn location(&self) -> String;
    fn location_format(&self) -> String;
    fn model(&self) -> String;
    fn name(&self) -> String;
    fn serial_number(&self) -> String;
    fn status(&self) -> ResourceStatus;
    fn get_type(&self) -> HardwareType;
}

#[derive(Debug, Clone, Copy)]
pub enum HardwareType {
    ArrayController,
    DiskDrive,
    SmartArray,
    StorageEnclosure,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct HardwareCommon {
    #[serde(flatten)]
    pub odata: ODataLinks,
    pub description: String,
    pub id: String,
    pub firmware_version: Firmware,
    pub location: String,
    pub location_format: String,
    pub model: String,
    pub name: String,
    pub serial_number: String,
    pub status: ResourceStatus,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct ArrayController {
    pub adapter_type: String,
    pub backup_power_source_status: String,
    pub current_operating_mode: String,
    pub encryption_crypto_officer_password_set: bool,
    pub encryption_enabled: bool,
    pub encryption_fw_locked: bool,
    pub encryption_has_locked_volumes_missing_boot_password: bool,
    pub encryption_mixed_volumes_enabled: bool,
    pub encryption_standalone_mode_enabled: bool,
    pub external_port_count: i64,
    #[serde(flatten)]
    pub hardware_common: HardwareCommon,
    pub hardware_revision: String,
    pub internal_port_count: i64,

    #[serde(rename = "Type")]
    pub controller_type: String,
}

impl Hardware for ArrayController {
    fn odata_context(&self) -> String {
        self.hardware_common
            .odata
            .odata_context
            .as_deref()
            .unwrap_or("")
            .to_owned()
    }
    fn odata_id(&self) -> String {
        self.hardware_common.odata.odata_id.to_owned()
    }
    fn odata_type(&self) -> String {
        self.hardware_common.odata.odata_type.to_owned()
    }
    fn description(&self) -> String {
        self.hardware_common.description.to_owned()
    }
    fn firmware_version(&self) -> Firmware {
        self.hardware_common.firmware_version.to_owned()
    }
    fn id(&self) -> String {
        self.hardware_common.id.to_owned()
    }
    fn location(&self) -> String {
        self.hardware_common.location.to_owned()
    }
    fn location_format(&self) -> String {
        self.hardware_common.location_format.to_owned()
    }
    fn model(&self) -> String {
        self.hardware_common.model.to_owned()
    }
    fn name(&self) -> String {
        self.hardware_common.name.to_owned()
    }
    fn serial_number(&self) -> String {
        self.hardware_common.serial_number.to_owned()
    }
    fn status(&self) -> ResourceStatus {
        self.hardware_common.status
    }
    fn get_type(&self) -> HardwareType {
        HardwareType::ArrayController
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct MultHardware {
    #[serde(flatten)]
    pub odata: ODataLinks,
    pub description: String,
    pub member_type: String,
    pub members: Vec<ODataId>,
    #[serde(rename = "Members@odata.count")]
    pub members_odata_count: i64,
    pub name: String,
    pub total: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct ArrayControllers {
    #[serde(flatten)]
    pub mult_hardware: MultHardware,
    #[serde(rename = "Type")]
    pub controller_type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct SmartArray {
    pub adapter_type: String,
    pub backup_power_source_status: String,
    pub current_operating_mode: String,
    pub encryption_crypto_officer_password_set: bool,
    pub encryption_enabled: bool,
    pub encryption_fw_locked: bool,
    pub encryption_has_locked_volumes_missing_boot_password: bool,
    pub encryption_mixed_volumes_enabled: bool,
    pub encryption_standalone_mode_enabled: bool,
    pub external_port_count: i64,
    pub hardware_revision: String,
    #[serde(flatten)]
    pub hardware_common: HardwareCommon,
    pub internal_port_count: i64,
    #[serde(rename = "Type")]
    pub array_type: String,
}

impl Hardware for SmartArray {
    fn odata_context(&self) -> String {
        self.hardware_common
            .odata
            .odata_context
            .as_deref()
            .unwrap_or("")
            .to_owned()
    }
    fn odata_id(&self) -> String {
        self.hardware_common.odata.odata_id.to_owned()
    }
    fn odata_type(&self) -> String {
        self.hardware_common.odata.odata_type.to_owned()
    }
    fn description(&self) -> String {
        self.hardware_common.description.to_owned()
    }
    fn firmware_version(&self) -> Firmware {
        self.hardware_common.firmware_version.to_owned()
    }
    fn id(&self) -> String {
        self.hardware_common.id.to_owned()
    }
    fn location(&self) -> String {
        self.hardware_common.location.to_owned()
    }
    fn location_format(&self) -> String {
        self.hardware_common.location_format.to_owned()
    }
    fn model(&self) -> String {
        self.hardware_common.model.to_owned()
    }
    fn name(&self) -> String {
        self.hardware_common.name.to_owned()
    }
    fn serial_number(&self) -> String {
        self.hardware_common.serial_number.to_owned()
    }
    fn status(&self) -> ResourceStatus {
        self.hardware_common.status
    }
    fn get_type(&self) -> HardwareType {
        HardwareType::SmartArray
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct StorageEnclosure {
    pub drive_bay_count: i64,
    #[serde(flatten)]
    pub hardware_common: HardwareCommon,
    #[serde(rename = "Type")]
    pub enclosure_type: String,
}

impl Hardware for StorageEnclosure {
    fn odata_context(&self) -> String {
        self.hardware_common
            .odata
            .odata_context
            .as_deref()
            .unwrap_or("")
            .to_owned()
    }
    fn odata_id(&self) -> String {
        self.hardware_common.odata.odata_id.to_owned()
    }
    fn odata_type(&self) -> String {
        self.hardware_common.odata.odata_type.to_owned()
    }
    fn description(&self) -> String {
        self.hardware_common.description.to_owned()
    }
    fn firmware_version(&self) -> Firmware {
        self.hardware_common.firmware_version.to_owned()
    }
    fn id(&self) -> String {
        self.hardware_common.id.to_owned()
    }
    fn location(&self) -> String {
        self.hardware_common.location.to_owned()
    }
    fn location_format(&self) -> String {
        self.hardware_common.location_format.to_owned()
    }
    fn model(&self) -> String {
        self.hardware_common.model.to_owned()
    }
    fn name(&self) -> String {
        self.hardware_common.name.to_owned()
    }
    fn serial_number(&self) -> String {
        self.hardware_common.serial_number.to_owned()
    }
    fn status(&self) -> ResourceStatus {
        self.hardware_common.status
    }
    fn get_type(&self) -> HardwareType {
        HardwareType::StorageEnclosure
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct StorageEnclosures {
    #[serde(flatten)]
    pub mult_hardware: MultHardware,
    #[serde(rename = "Type")]
    pub enclosure_type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct DiskDrive {
    pub block_size_bytes: i64,
    #[serde(rename = "CapacityGB")]
    pub capacity_gb: i64,
    pub capacity_logical_blocks: i64,
    pub capacity_mi_b: i64,
    pub carrier_application_version: String,
    pub carrier_authentication_status: String,
    pub current_temperature_celsius: i64,
    pub disk_drive_status_reasons: Vec<String>,
    pub encrypted_drive: bool,
    #[serde(flatten)]
    pub hardware_common: HardwareCommon,
    pub interface_speed_mbps: i64,
    pub interface_type: String,
    pub maximum_temperature_celsius: i64,
    pub media_type: String,
    pub power_on_hours: Option<i64>,
    pub rotational_speed_rpm: i64,
    pub ssd_endurance_utilization_percentage: Option<f64>,
    #[serde(rename = "Type")]
    pub drive_type: String,
}

impl Hardware for DiskDrive {
    fn odata_context(&self) -> String {
        self.hardware_common
            .odata
            .odata_context
            .as_deref()
            .unwrap_or("")
            .to_owned()
    }
    fn odata_id(&self) -> String {
        self.hardware_common.odata.odata_id.to_owned()
    }
    fn odata_type(&self) -> String {
        self.hardware_common.odata.odata_type.to_owned()
    }
    fn description(&self) -> String {
        self.hardware_common.description.to_owned()
    }
    fn firmware_version(&self) -> Firmware {
        self.hardware_common.firmware_version.to_owned()
    }
    fn id(&self) -> String {
        self.hardware_common.id.to_owned()
    }
    fn location(&self) -> String {
        self.hardware_common.location.to_owned()
    }
    fn location_format(&self) -> String {
        self.hardware_common.location_format.to_owned()
    }
    fn model(&self) -> String {
        self.hardware_common.model.to_owned()
    }
    fn name(&self) -> String {
        self.hardware_common.name.to_owned()
    }
    fn serial_number(&self) -> String {
        self.hardware_common.serial_number.to_owned()
    }
    fn status(&self) -> ResourceStatus {
        self.hardware_common.status
    }
    fn get_type(&self) -> HardwareType {
        HardwareType::DiskDrive
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct DiskDrives {
    #[serde(flatten)]
    pub mult_hardware: MultHardware,
    #[serde(rename = "Type")]
    pub drive_type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct LogicalDrives {
    #[serde(flatten)]
    pub odata: ODataLinks,
    pub description: String,
    pub member_type: String,
    #[serde(rename = "Members@odata.count")]
    pub members_odata_count: i64,
    pub name: String,
    pub total: i64,
    #[serde(rename = "Type")]
    pub drive_type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct StorageSubsystem {
    #[serde(flatten)]
    pub odata: ODataLinks,
    pub description: String,
    pub members: Option<Vec<ODataId>>,
    #[serde(rename = "Members@odata.count")]
    pub members_odata_count: i64,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Storage {
    #[serde(flatten)]
    pub odata: ODataLinks,
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub drives: Option<Vec<ODataId>>,
    pub volumes: Option<ODataId>,
    pub status: Option<ResourceStatus>,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct OemSmc {
    #[serde(flatten)]
    pub temperature: Option<f64>,
    pub percentage_drive_life_used: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Oem {
    #[serde(flatten)]
    pub supermicro: Option<OemSmc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Drives {
    #[serde(flatten)]
    pub odata: ODataLinks,
    pub capacity_bytes: Option<i64>,
    pub failure_predicted: Option<bool>,
    pub predicted_media_life_left_percent: Option<f64>,
    pub oem: Option<Oem>,
    pub id: Option<String>,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub name: Option<String>,
    pub revision: Option<String>,
    pub serial_number: Option<String>,
    pub status: Option<ResourceStatus>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct SimpleStorage {
    #[serde(flatten)]
    pub odata: ODataLinks,
    pub id: String,
    pub name: String,
    pub devices: Option<Vec<StorageDevice>>,
    pub description: Option<String>,
    pub status: Option<ResourceStatus>,
    pub uefi_device_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct StorageDevice {
    capacity_bytes: Option<i64>,
    manufacturer: Option<String>,
    model: Option<String>,
    name: String,
    status: Option<ResourceStatus>,
}

#[cfg(test)]
mod test {
    #[test]
    fn test_storage_logical_drives_parser() {
        let test_data = include_str!("testdata/logical-drives.json");
        let result: super::LogicalDrives = serde_json::from_str(test_data).unwrap();
        println!("result: {result:#?}");
    }

    #[test]
    fn test_array_controller_parser() {
        let test_data = include_str!("testdata/array-controller.json");
        let result: super::ArrayController = serde_json::from_str(test_data).unwrap();
        println!("result: {result:#?}");
    }

    #[test]
    fn test_storage_drives_parser() {
        let test_data = include_str!("testdata/disk-drives.json");
        let result: super::DiskDrives = serde_json::from_str(test_data).unwrap();
        println!("result: {result:#?}");
    }

    #[test]
    fn test_storage_drive_parser() {
        let test_data = include_str!("testdata/disk-drive.json");
        let result: super::DiskDrive = serde_json::from_str(test_data).unwrap();
        println!("result: {result:#?}");
    }

    #[test]
    fn test_array_controllers_parser() {
        let test_data = include_str!("testdata/array-controllers.json");
        let result: super::ArrayControllers = serde_json::from_str(test_data).unwrap();
        println!("result: {result:#?}");
    }

    #[test]
    fn test_smart_array_parser() {
        let test_data = include_str!("testdata/smart-array.json");
        let result: super::SmartArray = serde_json::from_str(test_data).unwrap();
        println!("result: {result:#?}");
    }

    #[test]
    fn test_storage_enclosure_parser() {
        let test_data = include_str!("testdata/storage-enclosure.json");
        let result: super::StorageEnclosure = serde_json::from_str(test_data).unwrap();
        println!("result: {result:#?}");
    }

    #[test]
    fn test_storage_enclosures_parser() {
        let test_data = include_str!("testdata/storage-enclosures.json");
        let result: super::StorageEnclosures = serde_json::from_str(test_data).unwrap();
        println!("result: {result:#?}");
    }
}
