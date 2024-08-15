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
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use tracing::debug;

use super::resource::OData;
use super::{ODataId, ODataLinks, OnOff, PCIeFunction, ResourceStatus};
use crate::NetworkDeviceFunction;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChassisActions {
    #[serde(rename = "#Chassis.Reset")]
    pub chassis_reset: Option<ChassisAction>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChassisAction {
    #[serde(rename = "@Redfish.ActionInfo")]
    pub title: Option<String>,
    pub target: Option<String>, // URL path of the action
}

#[derive(Debug, Serialize, Deserialize, Default, Copy, Clone, Eq, PartialEq)]
pub enum ChassisType {
    Rack,
    Blade,
    Enclosure,
    StandAlone,
    RackMount,
    Card,
    Cartridge,
    Row,
    Pod,
    Expansion,
    Sidecar,
    Zone,
    Sled,
    Shelf,
    Drawer,
    Module,
    Component,
    IPBasedDrive,
    RackGroup,
    StorageEnclosure,
    ImmersionTank,
    HeatExchanger,
    #[default]
    Other,
}

// A custom deserializer. If serialization fails then use the default value of the type.
fn ok_or_default<'a, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: Deserialize<'a> + Default,
    D: Deserializer<'a>,
{
    let v: Value = Deserialize::deserialize(deserializer)?;
    Ok(T::deserialize(v).unwrap_or_else(|e1| {
        debug!("Deserialization err: {}. Using default", e1);
        T::default()
    }))
}

impl std::fmt::Display for ChassisType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

/// http://redfish.dmtf.org/schemas/v1/Chassis.v1_23_0.json
/// The Chassis schema contains an inventory of chassis components.
/// This can include chassis parameters such as chassis type, model, etc.
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Chassis {
    #[serde(flatten)]
    pub odata: Option<ODataLinks>,
    pub actions: Option<ChassisActions>,
    pub assembly: Option<ODataId>,
    // Use default is missing or invalid enum value
    #[serde(default, deserialize_with = "ok_or_default")]
    pub chassis_type: Option<ChassisType>,
    pub controls: Option<ODataId>,
    pub environment_metrics: Option<ODataId>,
    pub id: Option<String>,
    pub location: Option<Location>,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub name: Option<String>,
    pub network_adapters: Option<ODataId>,
    #[serde(rename = "PCIeDevices")]
    pub pcie_devices: Option<ODataId>,
    #[serde(rename = "PCIeSlots")]
    pub pcie_slots: Option<ODataId>,
    pub part_number: Option<String>,
    pub power: Option<ODataId>,
    #[serde(default)] // Viking returns Chassis w.o power_state, so default will be used
    pub power_state: Option<OnOff>,
    pub power_subsystem: Option<ODataId>,
    pub sensors: Option<ODataId>,
    pub serial_number: Option<String>,
    pub status: Option<ResourceStatus>,
    pub thermal: Option<ODataId>,
    pub thermal_subsystem: Option<ODataId>,
    pub trusted_components: Option<ODataId>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct NetworkAdapter {
    #[serde(flatten)]
    pub odata: OData,
    pub id: String,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub part_number: Option<String>,
    pub serial_number: Option<String>,
    pub ports: Option<ODataId>,
    pub network_device_functions: Option<ODataId>,
    pub name: Option<String>,
    pub status: Option<ResourceStatus>,
    pub controllers: Option<Vec<NetworkAdapterController>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct NetworkAdapterController {
    pub firmware_package_version: Option<String>,
    pub links: Option<NetworkAdapterControllerLinks>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct NetworkAdapterControllerLinks {
    pub network_device_functions: Option<Vec<ODataId>>,
    pub ports: Option<Vec<ODataId>>,
    // Deprecated, but some old systems still use them
    pub network_ports: Option<Vec<ODataId>>,
    #[serde(default, rename = "PCIeDevices")]
    pub pcie_devices: Option<Vec<ODataId>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Location {
    pub part_location: Option<PartLocation>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PartLocation {
    pub location_type: Option<String>,
}
// This is a convenient container struct to hold
// details of a network interface.
pub struct MachineNetworkAdapter {
    pub is_dpu: bool,
    pub mac_address: Option<String>,
    pub network_device_function: NetworkDeviceFunction,
    pub pcie_function: PCIeFunction,
}
