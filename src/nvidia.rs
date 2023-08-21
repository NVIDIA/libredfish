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
use crate::{
    model::BootOption, standard::RedfishStandard, NetworkDeviceFunction,
    NetworkDeviceFunctionCollection, Redfish, RedfishError,
};
use std::collections::HashMap;

use crate::model::boot::{BootSourceOverrideEnabled, BootSourceOverrideTarget};
use crate::model::oem::nvidia::{InternalCPUModel, HostPrivilegeLevel};

pub struct Bmc {
    s: RedfishStandard,
}

pub enum BootOptionName {
    Http,
    Pxe,
    Disk,
}
impl BootOptionName {
    fn to_string(&self) -> &str {
        match self {
            BootOptionName::Http => "UEFI HTTPv4",
            BootOptionName::Pxe => "UEFI PXEv4",
            BootOptionName::Disk => "UEFI Non-Block Boot Device",
        }
    }
}

impl Bmc {
    pub fn new(s: RedfishStandard) -> Result<Bmc, RedfishError> {
        Ok(Bmc { s })
    }
}

impl Redfish for Bmc {
    fn change_password(&self, user: &str, new: &str) -> Result<(), RedfishError> {
        self.s.change_password(user, new)
    }

    fn get_firmware(
        &self,
        id: &str,
    ) -> Result<crate::model::software_inventory::SoftwareInventory, RedfishError> {
        self.s.get_firmware(id)
    }

    fn get_software_inventories(
        &self,
    ) -> Result<crate::model::software_inventory::SoftwareInventoryCollection, RedfishError> {
        self.s.get_software_inventories()
    }

    fn get_task(&self, id: &str) -> Result<crate::model::task::Task, RedfishError> {
        self.s.get_task(id)
    }

    fn get_power_state(&self) -> Result<crate::PowerState, RedfishError> {
        self.s.get_power_state()
    }

    fn get_power_metrics(&self) -> Result<crate::Power, RedfishError> {
        self.s.get_power_metrics()
    }

    fn power(&self, action: crate::SystemPowerControl) -> Result<(), RedfishError> {
        self.s.power(action)
    }

    fn machine_setup(&self) -> Result<(), RedfishError> {
        self.s.machine_setup()
    }

    fn get_thermal_metrics(&self) -> Result<crate::Thermal, RedfishError> {
        self.s.get_thermal_metrics()
    }

    fn lockdown(&self, target: crate::EnabledDisabled) -> Result<(), RedfishError> {
        self.s.lockdown(target)
    }

    fn lockdown_status(&self) -> Result<crate::Status, RedfishError> {
        self.s.lockdown_status()
    }

    fn setup_serial_console(&self) -> Result<(), RedfishError> {
        self.s.setup_serial_console()
    }

    fn serial_console_status(&self) -> Result<crate::Status, RedfishError> {
        self.s.serial_console_status()
    }

    fn get_boot_options(&self) -> Result<crate::BootOptions, RedfishError> {
        self.s.get_boot_options()
    }

    fn get_boot_option(&self, option_id: &str) -> Result<BootOption, RedfishError> {
        self.s.get_boot_option(option_id)
    }

    fn boot_once(&self, target: crate::Boot) -> Result<(), RedfishError> {
        match target {
            crate::Boot::Pxe => self.change_boot_settings(
                BootSourceOverrideTarget::Pxe,
                BootSourceOverrideEnabled::Once,
            ),
            crate::Boot::HardDisk => self.change_boot_settings(
                BootSourceOverrideTarget::Hdd,
                BootSourceOverrideEnabled::Once,
            ),
            crate::Boot::UefiHttp => self.change_boot_settings(
                BootSourceOverrideTarget::UefiHttp,
                BootSourceOverrideEnabled::Once,
            ),
        }
    }

    fn boot_first(&self, target: crate::Boot) -> Result<(), RedfishError> {
        match target {
            crate::Boot::Pxe => self.set_boot_first(&BootOptionName::Pxe),
            crate::Boot::HardDisk => self.set_boot_first(&BootOptionName::Disk),
            crate::Boot::UefiHttp => self.set_boot_first(&BootOptionName::Http),
        }
    }

    fn clear_tpm(&self) -> Result<(), RedfishError> {
        self.s.clear_tpm()
    }

    fn pcie_devices(&self) -> Result<Vec<crate::PCIeDevice>, RedfishError> {
        self.s.pcie_devices()
    }

    fn update_firmware(
        &self,
        firmware: std::fs::File,
    ) -> Result<crate::model::task::Task, RedfishError> {
        self.s.update_firmware(firmware)
    }

    fn bios(&self) -> Result<std::collections::HashMap<String, serde_json::Value>, RedfishError> {
        self.s.bios()
    }

    fn pending(
        &self,
    ) -> Result<std::collections::HashMap<String, serde_json::Value>, RedfishError> {
        self.s.pending()
    }

    fn clear_pending(&self) -> Result<(), RedfishError> {
        self.s.clear_pending()
    }

    fn get_system(&self) -> Result<crate::model::ComputerSystem, RedfishError> {
        self.s.get_system()
    }

    fn get_secure_boot(&self) -> Result<crate::model::secure_boot::SecureBoot, RedfishError> {
        self.s.get_secure_boot()
    }

    fn disable_secure_boot(&self) -> Result<(), RedfishError> {
        self.s.disable_secure_boot()
    }

    fn get_chassises(&self) -> Result<crate::ChassisCollection, RedfishError> {
        self.s.get_chassises()
    }

    fn get_chassis(&self, id: &str) -> Result<crate::Chassis, RedfishError> {
        self.s.get_chassis(id)
    }

    fn get_ethernet_interfaces(&self) -> Result<crate::EthernetInterfaceCollection, RedfishError> {
        self.s.get_ethernet_interfaces()
    }

    fn get_ethernet_interface(&self, id: &str) -> Result<crate::EthernetInterface, RedfishError> {
        self.s.get_ethernet_interface(id)
    }

    fn get_ports(&self, chassis_id: &str) -> Result<crate::NetworkPortCollection, RedfishError> {
        let url = format!(
            "Chassis/{}/NetworkAdapters/NvidiaNetworkAdapter/Ports",
            chassis_id
        );
        let (_status_code, body) = self.s.client.get(&url)?;
        Ok(body)
    }

    fn get_port(&self, chassis_id: &str, id: &str) -> Result<crate::NetworkPort, RedfishError> {
        let url = format!(
            "Chassis/{}/NetworkAdapters/NvidiaNetworkAdapter/Ports/{}",
            chassis_id, id
        );
        let (_status_code, body) = self.s.client.get(&url)?;
        Ok(body)
    }

    fn get_network_device_function(
        &self,
        chassis_id: &str,
        id: &str,
    ) -> Result<NetworkDeviceFunction, RedfishError> {
        let url = format!(
            "Chassis/{}/NetworkAdapters/NvidiaNetworkAdapter/NetworkDeviceFunctions/{}",
            chassis_id, id
        );
        let (_status_code, body) = self.s.client.get(&url)?;
        Ok(body)
    }

    fn get_network_device_functions(
        &self,
        chassis_id: &str,
    ) -> Result<NetworkDeviceFunctionCollection, RedfishError> {
        let url = format!(
            "Chassis/{}/NetworkAdapters/NvidiaNetworkAdapter/NetworkDeviceFunctions",
            chassis_id
        );
        let (_status_code, body) = self.s.client.get(&url)?;
        Ok(body)
    }

    fn change_uefi_password(
        &self,
        current_uefi_password: &str,
        new_uefi_password: &str,
    ) -> Result<(), RedfishError> {
        let mut attributes = HashMap::new();
        let mut data = HashMap::new();
        data.insert("CurrentUefiPassword", current_uefi_password.to_string());
        data.insert("UefiPassword", new_uefi_password.to_string());
        attributes.insert("Attributes", data);
        let url = format!("Systems/{}/Bios/Settings", self.s.system_id());
        let _status_code = self.s.client.patch(&url, attributes)?;
        Ok(())
    }

    fn change_boot_order(&self, boot_array: Vec<String>) -> Result<(), RedfishError> {
        let body = HashMap::from([("Boot", HashMap::from([("BootOrder", boot_array)]))]);
        let url = format!("Systems/{}/Settings", self.s.system_id());
        self.s.client.patch(&url, body)?;
        Ok(())
    }

    fn set_host_privilege_level(&self, level: HostPrivilegeLevel)-> Result<(), RedfishError> {
        let data = HashMap::from([("Attributes", HashMap::from([("Host Privilege Level", level.to_string())]))]);
        let url = format!("Systems/{}/Bios/Settings", self.s.system_id());
        self.s.client.patch(&url, data).map(|_status_code| Ok(()))?
    }

    fn set_internal_cpu_model(&self, model: InternalCPUModel)-> Result<(), RedfishError> {
        let data = HashMap::from([("Attributes", HashMap::from([("Internal CPU Model", model.to_string())]))]);
        let url = format!("Systems/{}/Bios/Settings", self.s.system_id());
        self.s.client.patch(&url, data).map(|_status_code| Ok(()))?
    }
}

impl Bmc {
    fn change_boot_settings(
        &self,
        override_taget: BootSourceOverrideTarget,
        override_enabled: BootSourceOverrideEnabled,
    ) -> Result<(), RedfishError> {
        let mut data: HashMap<String, String> = HashMap::new();
        data.insert("BootSourceOverrideMode".to_string(), "UEFI".to_string());
        data.insert(
            "BootSourceOverrideEnabled".to_string(),
            format!("{}", override_enabled),
        );
        data.insert(
            "BootSourceOverrideTarget".to_string(),
            format!("{}", override_taget),
        );
        let url = format!("Systems/{}/Settings ", self.s.system_id());
        self.s.client.patch(&url, HashMap::from([("Boot", data)]))?;
        Ok(())
    }

    // name: The name of the device you want to make the first boot choice.
    fn set_boot_first(&self, name: &BootOptionName) -> Result<(), RedfishError> {
        let boot_array = match self.get_boot_options_ids_with_first(name)? {
            None => {
                return Err(RedfishError::MissingBootOption(name.to_string().to_owned()));
            }
            Some(b) => b,
        };
        self.change_boot_order(boot_array)
    }

    // A Vec of string boot option names, with the one you want first.
    //
    // Example: get_boot_options_ids_with_first(lenovo::BootOptionName::Network) might return
    // ["Boot0003", "Boot0002", "Boot0001", "Boot0004"] where Boot0003 is Network. It has been
    // moved to the front ready for sending as an update.
    // The order of the other boot options does not change.
    //
    // If the boot option you want is not found returns Ok(None)
    fn get_boot_options_ids_with_first(
        &self,
        with_name: &BootOptionName,
    ) -> Result<Option<Vec<String>>, RedfishError> {
        let with_name_str = with_name.to_string();
        let mut ordered = Vec::new(); // the final boot options
        let boot_options = self.s.get_system()?.boot.boot_order;
        for member in boot_options {
            let b: BootOption = self.s.get_boot_option(member.as_str())?;
            if b.display_name.starts_with(with_name_str) {
                ordered.insert(0, b.id);
            } else {
                ordered.push(b.id);
            }
        }
        Ok(Some(ordered))
    }
}
