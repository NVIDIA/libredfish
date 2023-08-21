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
use std::collections::{HashMap, HashSet};

use tracing::debug;

use crate::model::chassis::{Chassis, ChassisCollection};
use crate::model::oem::nvidia::{InternalCPUModel, HostPrivilegeLevel};
use crate::model::power::Power;
use crate::model::secure_boot::SecureBoot;
use crate::model::power::Power;
use crate::model::software_inventory::{SoftwareInventory, SoftwareInventoryCollection};
use crate::model::thermal::Thermal;
use crate::model::{power, storage, thermal};
use crate::model::thermal::Thermal;
use crate::model::{power, storage, thermal, BootOption};
use crate::network::{RedfishHttpClient, REDFISH_ENDPOINT};
use crate::{
    model, Boot, EnabledDisabled, NetworkDeviceFunction, NetworkDeviceFunctionCollection,
    NetworkPort, NetworkPortCollection, PowerState, Redfish, Status,
};
use crate::{BootOptions, PCIeDevice, RedfishError};
use crate::model::network_device_function::{NetworkDeviceFunction, NetworkDeviceFunctionCollection};
use crate::model::chassis::{Chassis, ChassisCollection};

/// The calls that use the Redfish standard without any OEM extensions.
pub struct RedfishStandard {
    pub client: RedfishHttpClient,
    pub vendor: Option<String>,
    manager_id: String,
    system_id: String,
}

impl Redfish for RedfishStandard {
    fn change_password(&self, user: &str, new: &str) -> Result<(), RedfishError> {
        let url = format!("AccountService/Accounts/{}", user);
        let mut data = HashMap::new();
        data.insert("Password", new);
        self.client.patch(&url, &data).map(|_status_code| Ok(()))?
    }

    fn get_power_state(&self) -> Result<PowerState, RedfishError> {
        let system = self.get_system()?;
        Ok(system.power_state)
    }

    fn get_power_metrics(&self) -> Result<Power, RedfishError> {
        let power = self.get_power_metrics()?;
        Ok(power)
    }

    fn power(&self, action: model::SystemPowerControl) -> Result<(), RedfishError> {
        let url = format!("Systems/{}/Actions/ComputerSystem.Reset", self.system_id);
        let mut arg = HashMap::new();
        arg.insert("ResetType", action.to_string());
        // Lenovo: The expected HTTP response code is 204 No Content
        self.client.post(&url, arg).map(|_status_code| Ok(()))?
    }

    fn get_thermal_metrics(&self) -> Result<Thermal, RedfishError> {
        let thermal = self.get_thermal_metrics()?;
        Ok(thermal)
    }

    fn bios(&self) -> Result<HashMap<String, serde_json::Value>, RedfishError> {
        let url = format!("Systems/{}/Bios", self.system_id());
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    fn pending(&self) -> Result<HashMap<String, serde_json::Value>, RedfishError> {
        let url = format!("Systems/{}/Bios/Settings", self.system_id());
        self.pending_with_url(&url)
    }

    fn clear_pending(&self) -> Result<(), RedfishError> {
        let url = format!("Systems/{}/Bios/Settings", self.system_id());
        self.clear_pending_with_url(&url)
    }

    fn machine_setup(&self) -> Result<(), RedfishError> {
        unimplemented!("No standard implementation");
    }

    fn lockdown(&self, _target: EnabledDisabled) -> Result<(), RedfishError> {
        unimplemented!("No standard implementation");
    }

    fn lockdown_status(&self) -> Result<Status, RedfishError> {
        unimplemented!("No standard implementation");
    }

    fn setup_serial_console(&self) -> Result<(), RedfishError> {
        unimplemented!("No standard implementation");
    }

    fn serial_console_status(&self) -> Result<Status, RedfishError> {
        unimplemented!("No standard implementation");
    }

    fn get_boot_options(&self) -> Result<BootOptions, RedfishError> {
        self.get_boot_options()
    }

    fn get_boot_option(&self, option_id: &str) -> Result<BootOption, RedfishError> {
        let url = format!("Systems/{}/BootOptions/{}", self.system_id(), option_id);
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    fn boot_once(&self, _target: Boot) -> Result<(), RedfishError> {
        unimplemented!("No standard implementation");
    }

    fn boot_first(&self, _target: Boot) -> Result<(), RedfishError> {
        unimplemented!("No standard implementation");
    }

    fn clear_tpm(&self) -> Result<(), RedfishError> {
        unimplemented!("No standard implementation");
    }

    fn pcie_devices(&self) -> Result<Vec<PCIeDevice>, RedfishError> {
        let mut out = Vec::new();
        let mut seen = HashSet::new(); // Dell redfish response has duplicates
        let system = self.get_system()?;
        debug!("Listing {} PCIe devices..", system.pcie_devices.len());
        for member in system.pcie_devices {
            let url = member
                .odata_id
                .replace(&format!("/{REDFISH_ENDPOINT}/"), "");
            if seen.contains(&url) {
                continue;
            }
            let p: PCIeDevice = self.client.get(&url)?.1;
            seen.insert(url);
            if p.id.is_none() || p.manufacturer.is_none() {
                // Lenovo has lots of all-null devices with name "Adapater". Ignore those.
                continue;
            }
            out.push(p);
        }
        out.sort_unstable_by(|a, b| a.manufacturer.partial_cmp(&b.manufacturer).unwrap());
        Ok(out)
    }

    fn get_firmware(&self, id: &str) -> Result<SoftwareInventory, RedfishError> {
        let url = format!("UpdateService/FirmwareInventory/{}", id);
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    fn update_firmware(&self, firmware: std::fs::File) -> Result<model::task::Task, RedfishError> {
        let (_status_code, body) = self.client.post_file("UpdateService", firmware)?;
        Ok(body)
    }

    fn get_task(&self, id: &str) -> Result<model::task::Task, RedfishError> {
        let url = format!("TaskService/Tasks/{}", id);
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }
    
    fn get_network_device_function(&self, chassis_id: &str, id: &str) -> Result<NetworkDeviceFunction, RedfishError> {
        let url = format!("Chassis/{}/NetworkAdapters/NvidiaNetworkAdapter/NetworkDeviceFunctions/{}", chassis_id, id);
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    fn get_network_device_functions(&self, chassis_id: &str) -> Result<NetworkDeviceFunctionCollection, RedfishError> {
        let url = format!("Chassis/{}/NetworkAdapters/NvidiaNetworkAdapter/NetworkDeviceFunctions", chassis_id);
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    fn get_chassises(&self) -> Result<ChassisCollection, RedfishError> {
        let url =  "Chassis".to_string();
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    fn get_chassis(&self, id: &str) -> Result<Chassis, RedfishError> {
        let url = format!("Chassis/{}", id);
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    fn get_ports(&self, chassis_id: &str) -> Result<crate::NetworkPortCollection, RedfishError> {
        let url = format!("Chassis/{}/NetworkAdapters/NvidiaNetworkAdapter/Ports", chassis_id);
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    fn get_port(&self, chassis_id: &str, id: &str) -> Result<crate::NetworkPort, RedfishError> {
        let url = format!("Chassis/{}/NetworkAdapters/NvidiaNetworkAdapter/Ports/{}", chassis_id, id);
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    fn get_ethernet_interfaces(&self) -> Result<crate::EthernetInterfaceCollection, RedfishError> {
        let url = format!("Managers/{}/EthernetInterfaces", self.manager_id());
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    fn get_ethernet_interface(&self, id: &str) -> Result<crate::EthernetInterface, RedfishError> {
        let url = format!("Managers/{}/EthernetInterfaces/{}", self.manager_id(), id);
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    fn get_chassises(&self) -> Result<ChassisCollection, RedfishError> {
        let url = "Chassis".to_string();
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    fn get_chassis(&self, id: &str) -> Result<Chassis, RedfishError> {
        let url = format!("Chassis/{}", id);
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    fn get_ethernet_interfaces(&self) -> Result<crate::EthernetInterfaceCollection, RedfishError> {
        let url = format!("Managers/{}/EthernetInterfaces", self.manager_id());
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    fn get_ethernet_interface(&self, id: &str) -> Result<crate::EthernetInterface, RedfishError> {
        let url = format!("Managers/{}/EthernetInterfaces/{}", self.manager_id(), id);
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    fn get_software_inventories(&self) -> Result<SoftwareInventoryCollection, RedfishError> {
        let (_status_code, body) = self.client.get("UpdateService/FirmwareInventory")?;
        Ok(body)
    }

    fn get_system(&self) -> Result<model::ComputerSystem, RedfishError> {
        let url = format!("Systems/{}/", self.system_id);
        let host: model::ComputerSystem = self.client.get(&url)?.1;
        Ok(host)
    }

    fn get_secure_boot(&self) -> Result<SecureBoot, RedfishError> {
        let url = format!("Systems/{}/SecureBoot", self.system_id());
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    fn disable_secure_boot(&self) -> Result<(), RedfishError> {
        let mut data = HashMap::new();
        data.insert("SecureBootEnable", false);
        let url = format!("Systems/{}/SecureBoot", self.system_id());
        let _status_code = self.client.patch(&url, data)?;
        Ok(())
    }

    fn get_network_device_functions(
        &self,
        _chassis_id: &str,
    ) -> Result<NetworkDeviceFunctionCollection, RedfishError> {
        unimplemented!()
    }

    fn get_network_device_function(
        &self,
        _chassis_id: &str,
        _id: &str,
    ) -> Result<NetworkDeviceFunction, RedfishError> {
        unimplemented!()
    }

    fn get_ports(&self, _chassis_id: &str) -> Result<NetworkPortCollection, RedfishError> {
        unimplemented!()
    }

    fn get_port(&self, _chassis_id: &str, _id: &str) -> Result<NetworkPort, RedfishError> {
        unimplemented!()
    }

    fn change_uefi_password(
        &self,
        _current_uefi_password: &str,
        _new_uefi_password: &str,
    ) -> Result<(), RedfishError> {
        unimplemented!()
    }

    fn change_boot_order(&self, _boot_array: Vec<String>) -> Result<(), RedfishError> {
        unimplemented!("No standard implementation for change boot order");
    }

    fn set_internal_cpu_model(&self, model: InternalCPUModel)-> Result<(), RedfishError> {
        unimplemented!()
    }

    fn set_host_privilege_level(&self, level: HostPrivilegeLevel)-> Result<(), RedfishError> {
        unimplemented!()
    }

}

impl RedfishStandard {
    //
    // PUBLIC
    //

    /// Create and setup a connection to BMC.
    /// Issues two HTTP calls to get intial data.
    pub fn new(client: RedfishHttpClient) -> Result<Self, RedfishError> {
        let mut r = Self {
            client,
            manager_id: "".to_string(),
            system_id: "".to_string(),
            vendor: None,
        };
        r.set_vendor()?;
        r.set_system_id()?;
        r.set_manager_id()?;
        Ok(r)
    }

    pub fn system_id(&self) -> &str {
        &self.system_id
    }

    pub fn manager_id(&self) -> &str {
        &self.manager_id
    }

    pub fn get_boot_options(&self) -> Result<model::BootOptions, RedfishError> {
        let url = format!("Systems/{}/BootOptions", self.system_id());
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    // The URL differs for Lenovo, but the rest is the same
    pub fn pending_with_url(
        &self,
        pending_url: &str,
    ) -> Result<HashMap<String, serde_json::Value>, RedfishError> {
        let pending_attrs = self.pending_attributes(pending_url)?;
        let current_attrs = self.bios_attributes()?;
        Ok(attr_diff(&pending_attrs, &current_attrs))
    }

    // There's no standard Redfish way to clear pending BIOS settings, so we find the
    // pending changes and set them back to their existing values
    pub fn clear_pending_with_url(&self, pending_url: &str) -> Result<(), RedfishError> {
        let pending_attrs = self.pending_attributes(pending_url)?;
        let current_attrs = self.bios_attributes()?;
        let diff = attr_diff(&pending_attrs, &current_attrs);

        let mut reset_attrs = HashMap::new();
        for k in diff.keys() {
            reset_attrs.insert(k, current_attrs.get(k));
        }
        let mut body = HashMap::new();
        body.insert("Attributes", reset_attrs);
        let url = format!("Systems/{}/Bios/Pending", self.system_id());
        self.client.patch(&url, body).map(|_status_code| ())
    }

    //
    // PRIVATE
    //

    // Current BIOS attributes
    fn bios_attributes(&self) -> Result<serde_json::Value, RedfishError> {
        let mut b = self.bios()?;
        b.remove("Attributes")
            .ok_or_else(|| RedfishError::MissingKey {
                key: "Attributes".to_string(),
                url: format!("Systems/{}/Bios", self.system_id()),
            })
    }

    // BIOS attributes that will be applied on next restart
    fn pending_attributes(
        &self,
        pending_url: &str,
    ) -> Result<serde_json::Map<String, serde_json::Value>, RedfishError> {
        let (_sc, mut body): (reqwest::StatusCode, HashMap<String, serde_json::Value>) =
            self.client.get(pending_url)?;
        let mut attrs = body
            .remove("Attributes")
            .ok_or_else(|| RedfishError::MissingKey {
                key: "Attributes".to_string(),
                url: pending_url.to_string(),
            })?;
        let attrs_map = match attrs.as_object_mut() {
            Some(m) => m,
            None => {
                return Err(RedfishError::InvalidKeyType {
                    key: "Attributes".to_string(),
                    expected_type: "Map".to_string(),
                    url: pending_url.to_string(),
                })
            }
        };
        Ok(core::mem::take(attrs_map))
    }

    /// Fetch root URL and record the vendor, if any
    fn set_vendor(&mut self) -> Result<(), RedfishError> {
        let (_, out): (_, HashMap<String, serde_json::Value>) = self.client.get("")?;
        self.vendor = match out.get("Vendor") {
            Some(v) => v.as_str().map(|s| s.to_string()),
            None => None,
        };
        debug!(
            "BMC Vendor: {}",
            self.vendor.as_deref().unwrap_or("Unknown")
        );
        Ok(())
    }

    /// Fetch and set System number. Needed for all `Systems/{system_id}/...` calls
    fn set_system_id(&mut self) -> Result<(), RedfishError> {
        let (_, systems): (_, model::Systems) = self.client.get("Systems/")?;
        if systems.members.is_empty() {
            self.system_id = "1".to_string(); // default to DMTF standard suggested
            return Ok(());
        }
        let v: Vec<&str> = systems.members[0].odata_id.split('/').collect();
        self.system_id = v.last().unwrap().to_string();
        Ok(())
    }

    /// Fetch and set Manager number. Needed for all `Managers/{system_id}/...` calls
    fn set_manager_id(&mut self) -> Result<(), RedfishError> {
        let (_, bmcs): (_, model::Managers) = self.client.get("Managers/")?;
        if bmcs.members.is_empty() {
            self.manager_id = "1".to_string(); // default to dmtf standard suggested
            return Ok(());
        }
        let v: Vec<&str> = bmcs.members[0].odata_id.split('/').collect();
        self.manager_id = v.last().unwrap().to_string();
        Ok(())
    }

    //
    // NOT CURRENTLY USED
    //

    #[allow(dead_code)]
    pub fn get_manager(&self) -> Result<model::Manager, RedfishError> {
        let (_, manager): (_, model::Manager) = self
            .client
            .get(&format!("Managers/{}", self.manager_id()))?;
        Ok(manager)
    }

    #[allow(dead_code)]
    pub fn get_array_controller(
        &self,
        controller_id: u64,
    ) -> Result<storage::ArrayController, RedfishError> {
        let url = format!(
            "Systems/{}/SmartStorage/ArrayControllers/{}/",
            self.system_id(),
            controller_id
        );
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    #[allow(dead_code)]
    pub fn get_array_controllers(&self) -> Result<storage::ArrayControllers, RedfishError> {
        let url = format!(
            "Systems/{}/SmartStorage/ArrayControllers/",
            self.system_id()
        );
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    /// Query the power status from the server
    #[allow(dead_code)]
    pub fn get_power_status(&self) -> Result<power::Power, RedfishError> {
        let url = format!("Chassis/{}/Power/", self.system_id());
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    /// Query the power supplies and voltages stats from the server
    pub fn get_power_metrics(&self) -> Result<power::Power, RedfishError> {
        let url = format!("Chassis/{}/Power/", self.system_id());
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    /// Query the thermal status from the server
    pub fn get_thermal_metrics(&self) -> Result<thermal::Thermal, RedfishError> {
        let url = format!("Chassis/{}/Thermal/", self.system_id());
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    /// Query the smart array status from the server
    #[allow(dead_code)]
    pub fn get_smart_array_status(
        &self,
        controller_id: u64,
    ) -> Result<storage::SmartArray, RedfishError> {
        let url = format!(
            "Systems/{}/SmartStorage/ArrayControllers/{}/",
            self.system_id(),
            controller_id
        );
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    #[allow(dead_code)]
    pub fn get_logical_drives(
        &self,
        controller_id: u64,
    ) -> Result<storage::LogicalDrives, RedfishError> {
        let url = format!(
            "Systems/{}/SmartStorage/ArrayControllers/{}/LogicalDrives/",
            self.system_id(),
            controller_id
        );
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    #[allow(dead_code)]
    pub fn get_physical_drive(
        &self,
        drive_id: u64,
        controller_id: u64,
    ) -> Result<storage::DiskDrive, RedfishError> {
        let url = format!(
            "Systems/{}/SmartStorage/ArrayControllers/{}/DiskDrives/{}/",
            self.system_id(),
            controller_id,
            drive_id,
        );
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    #[allow(dead_code)]
    pub fn get_physical_drives(
        &self,
        controller_id: u64,
    ) -> Result<storage::DiskDrives, RedfishError> {
        let url = format!(
            "Systems/{}/SmartStorage/ArrayControllers/{}/DiskDrives/",
            self.system_id(),
            controller_id
        );
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    #[allow(dead_code)]
    pub fn get_storage_enclosures(
        &self,
        controller_id: u64,
    ) -> Result<storage::StorageEnclosures, RedfishError> {
        let url = format!(
            "Systems/{}/SmartStorage/ArrayControllers/{}/StorageEnclosures/",
            self.system_id(),
            controller_id
        );
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    #[allow(dead_code)]
    pub fn get_storage_enclosure(
        &self,
        controller_id: u64,
        enclosure_id: u64,
    ) -> Result<storage::StorageEnclosure, RedfishError> {
        let url = format!(
            "Systems/{}/SmartStorage/ArrayControllers/{}/StorageEnclosures/{}/",
            self.system_id(),
            controller_id,
            enclosure_id,
        );
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }
}

// Key/value pairs that different between these two sets of attributes
// The left needs to be a full map, but the right side only needs to support `get`.
fn attr_diff(
    l: &serde_json::Map<String, serde_json::Value>,
    r: &serde_json::Value,
) -> HashMap<String, serde_json::Value> {
    l.iter()
        .filter(|(k, v)| r.get(k) != Some(v))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}
