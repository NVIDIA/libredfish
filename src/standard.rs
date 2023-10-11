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

use reqwest::Method;
use tracing::debug;

use crate::model::chassis::{Chassis, ChassisCollection};
use crate::model::oem::nvidia::{HostPrivilegeLevel, InternalCPUModel};
use crate::model::power::Power;
use crate::model::secure_boot::SecureBoot;
use crate::model::sel::LogEntry;
use crate::model::service_root::ServiceRoot;
use crate::model::software_inventory::{SoftwareInventory, SoftwareInventoryCollection};
use crate::model::task::{Task, TaskCollection};
use crate::model::thermal::Thermal;
use crate::model::{power, storage, thermal, BootOption};
use crate::model::service_root::ServiceRoot;
use crate::network::{RedfishHttpClient, REDFISH_ENDPOINT};
use crate::{
    model, Boot, EnabledDisabled, EthernetInterfaceCollection, NetworkDeviceFunction, NetworkPort,
    PowerState, Redfish, RoleId, Status, Systems,
};
use crate::{BootOptions, PCIeDevice, RedfishError};
use crate::model::network_device_function::{NetworkDeviceFunction, NetworkDeviceFunctionCollection};
use crate::model::chassis::{Chassis, ChassisCollection};

/// The calls that use the Redfish standard without any OEM extensions.
#[derive(Clone)]
pub struct RedfishStandard {
    pub client: RedfishHttpClient,
    pub vendor: Option<String>,
    manager_id: String,
    system_id: String,
}

impl Redfish for RedfishStandard {
    fn create_user(
        &self,
        username: &str,
        password: &str,
        role_id: RoleId,
    ) -> Result<(), RedfishError> {
        let mut data = HashMap::new();
        data.insert("UserName", username.to_string());
        data.insert("Password", password.to_string());
        data.insert("RoleId", role_id.to_string());
        self.client
            .post("AccountService/Accounts", data)
            .map(|_status_code| Ok(()))?
    }

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

    fn bmc_reset(&self) -> Result<(), RedfishError> {
        let url = format!("Managers/{}/Actions/Manager.Reset", self.manager_id);
        let mut arg = HashMap::new();
        // Dell only has GracefulRestart. The spec, and Lenovo, also have ForceRestart.
        // Response code 204 No Content is fine.
        arg.insert("ResetType", "GracefulRestart".to_string());
        self.client.post(&url, arg).map(|_status_code| Ok(()))?
    }

    fn get_thermal_metrics(&self) -> Result<Thermal, RedfishError> {
        let thermal = self.get_thermal_metrics()?;
        Ok(thermal)
    }

    fn get_system_event_log(&self) -> Result<Vec<LogEntry>, RedfishError> {
        Err(RedfishError::NotSupported("SEL".to_string()))
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
        Err(RedfishError::NotSupported("machine_setup".to_string()))
    }

    fn lockdown(&self, _target: EnabledDisabled) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported("lockdown".to_string()))
    }

    fn lockdown_status(&self) -> Result<Status, RedfishError> {
        Err(RedfishError::NotSupported("lockdown_status".to_string()))
    }

    fn setup_serial_console(&self) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported(
            "setup_serial_console".to_string(),
        ))
    }

    fn serial_console_status(&self) -> Result<Status, RedfishError> {
        Err(RedfishError::NotSupported(
            "setup_serial_console".to_string(),
        ))
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
        Err(RedfishError::NotSupported("boot_once".to_string()))
    }

    fn boot_first(&self, _target: Boot) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported("boot_first".to_string()))
    }

    fn clear_tpm(&self) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported("clear_tpm".to_string()))
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

    fn update_firmware(&self, firmware: std::fs::File) -> Result<Task, RedfishError> {
        let (_status_code, body) = self.client.post_file("UpdateService", firmware)?;
        Ok(body)
    }

    fn get_tasks(&self) -> Result<Vec<String>, RedfishError> {
        let (_status_code, tasks): (_, TaskCollection) = self.client.get("TaskService/Tasks/")?;
        if tasks.members.is_empty() {
            return Ok(vec![]);
        }
        let v: Vec<String> = tasks
            .members
            .into_iter()
            .map(|d| d.odata_id.split('/').last().unwrap().to_string())
            .collect();
        Ok(v)
    }

    fn get_task(&self, id: &str) -> Result<Task, RedfishError> {
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

    fn get_chassis_all(&self) -> Result<Vec<String>, RedfishError> {
        let (_status_code, chassises): (_, ChassisCollection) = self.client.get("Chassis/")?;
        if chassises.members.is_empty() {
            return Ok(vec![]);
        }
        let v: Vec<String> = chassises
            .members
            .into_iter()
            .map(|d| d.odata_id.split('/').last().unwrap().to_string())
            .collect();
        Ok(v)
    }

    fn get_chassis(&self, id: &str) -> Result<Chassis, RedfishError> {
        let url = format!("Chassis/{}", id);
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    fn get_ethernet_interfaces(&self) -> Result<Vec<String>, RedfishError> {
        let url = format!("Managers/{}/EthernetInterfaces", self.manager_id);
        let (_status_code, eth_ifaces): (_, EthernetInterfaceCollection) = self.client.get(&url)?;

        if eth_ifaces.members.is_empty() {
            return Ok(vec![]);
        }
        let v: Vec<String> = eth_ifaces
            .members
            .into_iter()
            .map(|d| d.odata_id.split('/').last().unwrap().to_string())
            .collect();

        Ok(v)
    }

    fn get_ethernet_interface(&self, id: &str) -> Result<crate::EthernetInterface, RedfishError> {
        let url = format!("Managers/{}/EthernetInterfaces/{}", self.manager_id(), id);
        let (_status_code, body) = self.client.get(&url)?;
        Ok(body)
    }

    fn get_software_inventories(&self) -> Result<Vec<String>, RedfishError> {
        let (_status_code, sw_inventories): (_, SoftwareInventoryCollection) =
            self.client.get("UpdateService/FirmwareInventory")?;

        if sw_inventories.members.is_empty() {
            return Ok(vec![]);
        }
        let v: Vec<String> = sw_inventories
            .members
            .into_iter()
            .map(|d| d.odata_id.split('/').last().unwrap().to_string())
            .collect();

        Ok(v)
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

    fn enable_secure_boot(&self) -> Result<(), RedfishError> {
        let mut data = HashMap::new();
        data.insert("SecureBootEnable", true);
        let url = format!("Systems/{}/SecureBoot", self.system_id());
        let _status_code = self.client.patch(&url, data)?;
        Ok(())
    }

    fn add_secure_boot_certificate(&self, pem_cert: &str) -> Result<Task, RedfishError> {
        let mut data = HashMap::new();
        data.insert("CertificateString", pem_cert);
        data.insert("CertificateType", "PEM");
        let url = format!(
            "Systems/{}/SecureBoot/SecureBootDatabases/db/Certificates",
            self.system_id()
        );
        let (_status_code, resp_opt) =
            self.client
                .req::<Task, _>(Method::POST, &url, Some(data), None, None)?;
        match resp_opt {
            Some(response_body) => Ok(response_body),
            None => Err(RedfishError::NoContent),
        }
    }

    fn disable_secure_boot(&self) -> Result<(), RedfishError> {
        let mut data = HashMap::new();
        data.insert("SecureBootEnable", false);
        let url = format!("Systems/{}/SecureBoot", self.system_id());
        let _status_code = self.client.patch(&url, data)?;
        Ok(())
    }

    fn get_network_device_functions(&self, _chassis_id: &str) -> Result<Vec<String>, RedfishError> {
        Err(RedfishError::NotSupported(
            "get_network_device_functions".to_string(),
        ))
    }

    fn get_network_device_function(
        &self,
        _chassis_id: &str,
        _id: &str,
    ) -> Result<NetworkDeviceFunction, RedfishError> {
        Err(RedfishError::NotSupported(
            "get_network_device_function".to_string(),
        ))
    }

    fn get_ports(&self, _chassis_id: &str) -> Result<Vec<String>, RedfishError> {
        Err(RedfishError::NotSupported("get_ports".to_string()))
    }

    fn get_port(&self, _chassis_id: &str, _id: &str) -> Result<NetworkPort, RedfishError> {
        Err(RedfishError::NotSupported("get_port".to_string()))
    }

    fn change_uefi_password(
        &self,
        _current_uefi_password: &str,
        _new_uefi_password: &str,
    ) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported(
            "change_uefi_password".to_string(),
        ))
    }

    fn change_boot_order(&self, _boot_array: Vec<String>) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported("change_boot_order".to_string()))
    }

    fn set_internal_cpu_model(&self, _model: InternalCPUModel) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported(
            "set_internal_cpu_model".to_string(),
        ))
    }

    fn set_host_privilege_level(&self, _level: HostPrivilegeLevel) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported(
            "set_host_privilege_level".to_string(),
        ))
    }

    fn get_service_root(&self) -> Result<ServiceRoot, RedfishError> {
        let (_status_code, body) = self.client.get("")?;
        Ok(body)
    }

    fn get_systems(&self) -> Result<Vec<String>, RedfishError> {
        let (_, systems): (_, Systems) = self.client.get("Systems/")?;
        if systems.members.is_empty() {
            return Ok(vec!["1".to_string()]); // default to DMTF standard suggested
        }
        let v: Vec<String> = systems
            .members
            .into_iter()
            .map(|d| d.odata_id.split('/').last().unwrap().to_string())
            .collect();

        Ok(v)
    }

    fn get_manager(&self) -> Result<Manager, RedfishError> {
        let (_, manager): (_, Manager) = self
            .client
            .get(&format!("Managers/{}", self.manager_id()))?;
        Ok(manager)
    }

    fn get_managers(&self) -> Result<Vec<String>, RedfishError> {
        let (_, bmcs): (_, Managers) = self.client.get("Managers/")?;
        if bmcs.members.is_empty() {
            return Ok(vec!["1".to_string()]);
        }
        let v: Vec<String> = bmcs
            .members
            .into_iter()
            .map(|d| d.odata_id.split('/').last().unwrap().to_string())
            .collect();
        Ok(v)
    }

    fn bmc_reset_to_defaults(&self) -> Result<(), RedfishError> {
        let url = format!(
            "Managers/{}/Actions/Manager.ResetToDefaults",
            self.manager_id
        );
        let mut arg = HashMap::new();
        arg.insert("ResetToDefaultsType", "ResetAll".to_string());
        self.client.post(&url, arg).map(|_status_code| Ok(()))?
    }
}

impl RedfishStandard {
    //
    // PUBLIC
    //

    /// Fetch root URL and record the vendor, if any
    pub fn set_vendor(&mut self, vendor_id: &str) -> Result<Box<dyn crate::Redfish>, RedfishError> {
        self.vendor = Some(vendor_id.to_string());
        debug!(
            "BMC Vendor: {}",
            self.vendor.as_deref().unwrap_or("Unknown")
        );
        match self.vendor.as_deref() {
            Some("Dell") => Ok(Box::new(crate::dell::Bmc::new(self.clone())?)),
            Some("Lenovo") => Ok(Box::new(crate::lenovo::Bmc::new(self.clone())?)),
            Some("Nvidia") => Ok(Box::new(crate::nvidia::Bmc::new(self.clone())?)),
            _ => Ok(Box::new(self.clone())),
        }
    }

    /// Fetch and set System number. Needed for all `Systems/{system_id}/...` calls
    pub fn set_system_id(&mut self, system_id: &str) -> Result<(), RedfishError> {
        self.system_id = system_id.to_string();
        Ok(())
    }

    /// Fetch and set Manager number. Needed for all `Managers/{system_id}/...` calls
    pub fn set_manager_id(&mut self, manager_id: &str) -> Result<(), RedfishError> {
        self.manager_id = manager_id.to_string();
        Ok(())
    }

    /// Create and setup a connection to BMC.
    pub fn new(client: RedfishHttpClient) -> Result<Self, RedfishError> {
        let r = Self {
            client,
            manager_id: "".to_string(),
            system_id: "".to_string(),
            vendor: None,
        };
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
