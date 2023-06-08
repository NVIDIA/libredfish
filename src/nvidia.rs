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
use crate::{model::BootOption, standard::RedfishStandard, Redfish, RedfishError};

pub struct Bmc {
    s: RedfishStandard,
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

    fn power(&self, action: crate::SystemPowerControl) -> Result<(), RedfishError> {
        self.s.power(action)
    }

    fn machine_setup(&self) -> Result<(), RedfishError> {
        self.s.machine_setup()
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
        self.s.boot_once(target)
    }

    fn boot_first(&self, target: crate::Boot) -> Result<(), RedfishError> {
        self.s.boot_first(target)
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

    fn get_network_device_functions(
        &self,
        chassis_id: &str,
    ) -> Result<crate::NetworkDeviceFunctionCollection, RedfishError> {
        self.s.get_network_device_functions(chassis_id)
    }

    fn get_network_device_function(
        &self,
        chassis_id: &str,
        id: &str,
    ) -> Result<crate::NetworkDeviceFunction, RedfishError> {
        self.s.get_network_device_function(chassis_id, id)
    }

    fn get_chassises(&self) -> Result<crate::ChassisCollection, RedfishError> {
        self.s.get_chassises()
    }

    fn get_chassis(&self, id: &str) -> Result<crate::Chassis, RedfishError> {
        self.s.get_chassis(id)
    }

    fn get_ports(&self, chassis_id: &str) -> Result<crate::NetworkPortCollection, RedfishError> {
        self.s.get_ports(chassis_id)
    }

    fn get_port(&self, chassis_id: &str, id: &str) -> Result<crate::NetworkPort, RedfishError> {
        self.s.get_port(chassis_id, id)
    }

    fn get_ethernet_interfaces(&self) -> Result<crate::EthernetInterfaceCollection, RedfishError> {
        self.s.get_ethernet_interfaces()
    }

    fn get_ethernet_interface(&self, id: &str) -> Result<crate::EthernetInterface, RedfishError> {
        self.s.get_ethernet_interface(id)
    }
}
