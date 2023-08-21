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
use std::collections::HashMap;

use crate::{
    model::{
        chassis::{Chassis, ChassisCollection},
        network_device_function::{NetworkDeviceFunction, NetworkDeviceFunctionCollection},
        oem::dell,
        power::Power,
        secure_boot::SecureBoot,
        software_inventory::{SoftwareInventory, SoftwareInventoryCollection},
        network_device_function::{NetworkDeviceFunction, NetworkDeviceFunctionCollection}, 
        chassis::{Chassis, ChassisCollection},
        power::Power,
        thermal::Thermal,
        BootOption, ComputerSystem, OnOff,
    },
    standard::RedfishStandard,
    Boot, BootOptions, EnabledDisabled, PCIeDevice, PowerState, Redfish, RedfishError, Status,
    StatusInternal, SystemPowerControl,
};

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

    fn get_power_state(&self) -> Result<PowerState, RedfishError> {
        self.s.get_power_state()
    }

    fn get_power_metrics(&self) -> Result<Power, RedfishError> {
        self.s.get_power_metrics()
    }

    fn power(&self, action: SystemPowerControl) -> Result<(), RedfishError> {
        self.s.power(action)
    }

    fn get_thermal_metrics(&self) -> Result<Thermal, RedfishError> {
        self.s.get_thermal_metrics()
    }

    fn bios(&self) -> Result<HashMap<String, serde_json::Value>, RedfishError> {
        self.s.bios()
    }

    fn machine_setup(&self) -> Result<(), RedfishError> {
        self.delete_job_queue()?;

        let apply_time = dell::SetSettingsApplyTime {
            apply_time: dell::RedfishSettingsApplyTime::OnReset, // requires reboot to apply
        };
        let machine_settings = dell::MachineBiosAttrs {
            in_band_manageability_interface: EnabledDisabled::Disabled,
            uefi_variable_access: dell::UefiVariableAccessSettings::Controlled,
            serial_comm: dell::SerialCommSettings::OnConRedir,
            serial_port_address: dell::SerialPortSettings::Com1,
            ext_serial_connector: dell::SerialPortExtSettings::Serial1,
            fail_safe_baud: "115200".to_string(),
            con_term_type: dell::SerialPortTermSettings::Vt100Vt220,
            redir_after_boot: EnabledDisabled::Enabled,
            tpm_security: OnOff::On,
            tpm2_hierarchy: dell::Tpm2HierarchySettings::Clear,
        };
        let set_machine_attrs = dell::SetMachineBiosAttrs {
            redfish_settings_apply_time: apply_time,
            attributes: machine_settings,
        };

        let url = format!("Systems/{}/Bios/Settings/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_machine_attrs)
            .map(|_status_code| ())?;

        self.setup_bmc_remote_access()?;
        // always do system lockdown last.
        self.enable_bmc_lockdown(dell::BootDevices::PXE, false)
    }

    fn lockdown(&self, target: EnabledDisabled) -> Result<(), RedfishError> {
        use EnabledDisabled::*;
        match target {
            Enabled => {
                self.delete_job_queue()?;
                self.enable_bios_lockdown()?;
                self.enable_bmc_lockdown(dell::BootDevices::PXE, false)
            }
            Disabled => {
                // ideally we'd delete the job queue here, but we can't when lockdown is enabled
                self.disable_bmc_lockdown(dell::BootDevices::PXE, false)?;
                self.disable_bios_lockdown()
            }
        }
    }

    fn lockdown_status(&self) -> Result<Status, RedfishError> {
        let mut message = String::new();
        let enabled = EnabledDisabled::Enabled.to_string();
        let disabled = EnabledDisabled::Disabled.to_string();

        // BIOS lockdown

        let url = format!("Systems/{}/Bios", self.s.system_id());
        let (_status_code, bios): (_, dell::Bios) = self.s.client.get(&url)?;

        let in_band = bios.attributes.in_band_manageability_interface;
        let uefi_var = bios.attributes.uefi_variable_access;
        message.push_str(&format!(
            "BIOS: in_band_manageability_interface={in_band}, uefi_variable_access={uefi_var}. "
        ));

        let is_bios_locked = in_band == disabled
            && uefi_var == dell::UefiVariableAccessSettings::Controlled.to_string();
        let is_bios_unlocked = in_band == enabled
            && uefi_var == dell::UefiVariableAccessSettings::Standard.to_string();

        // BMC lockdown

        let (attrs, url) = self.manager_attributes()?;

        let key = "Lockdown.1.SystemLockdown";
        let system_lockdown = attrs
            .get(key)
            .ok_or_else(|| RedfishError::MissingKey {
                key: key.to_string(),
                url: url.to_string(),
            })?
            .as_str()
            .ok_or_else(|| RedfishError::InvalidKeyType {
                key: key.to_string(),
                expected_type: "&str".to_string(),
                url: url.to_string(),
            })?;

        let key = "Racadm.1.Enable";
        let racadm = attrs
            .get(key)
            .ok_or_else(|| RedfishError::MissingKey {
                key: key.to_string(),
                url: url.to_string(),
            })?
            .as_str()
            .ok_or_else(|| RedfishError::InvalidKeyType {
                key: key.to_string(),
                expected_type: "&str".to_string(),
                url: url.to_string(),
            })?;

        message.push_str(&format!(
            "BMC: system_lockdown={system_lockdown}, racadm={racadm}."
        ));

        let is_bmc_locked = system_lockdown == enabled && racadm == disabled;
        let is_bmc_unlocked = system_lockdown == disabled && racadm == enabled;

        Ok(Status {
            message,
            status: if is_bios_locked && is_bmc_locked {
                StatusInternal::Enabled
            } else if is_bios_unlocked && is_bmc_unlocked {
                StatusInternal::Disabled
            } else {
                StatusInternal::Partial
            },
        })
    }

    fn setup_serial_console(&self) -> Result<(), RedfishError> {
        self.delete_job_queue()?;

        self.setup_bmc_remote_access()?;

        let apply_time = dell::SetSettingsApplyTime {
            apply_time: dell::RedfishSettingsApplyTime::OnReset, // requires reboot to apply
        };
        let serial_console = dell::BiosSerialAttrs {
            serial_comm: dell::SerialCommSettings::OnConRedir,
            serial_port_address: dell::SerialPortSettings::Com1,
            ext_serial_connector: dell::SerialPortExtSettings::Serial1,
            fail_safe_baud: "115200".to_string(),
            con_term_type: dell::SerialPortTermSettings::Vt100Vt220,
            redir_after_boot: EnabledDisabled::Enabled,
        };
        let set_serial_attrs = dell::SetBiosSerialAttrs {
            redfish_settings_apply_time: apply_time,
            attributes: serial_console,
        };

        let url = format!("Systems/{}/Bios/Settings/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_serial_attrs)
            .map(|_status_code| ())
    }

    fn serial_console_status(&self) -> Result<Status, RedfishError> {
        let Status {
            status: remote_access_status,
            message: remote_access_message,
        } = self.bmc_remote_access_status()?;
        let Status {
            status: bios_serial_status,
            message: bios_serial_message,
        } = self.bios_serial_console_status()?;

        let final_status = {
            use StatusInternal::*;
            match (remote_access_status, bios_serial_status) {
                (Enabled, Enabled) => Enabled,
                (Disabled, Disabled) => Disabled,
                _ => Partial,
            }
        };
        Ok(Status {
            status: final_status,
            message: format!("BMC: {remote_access_message}. BIOS: {bios_serial_message}."),
        })
    }

    fn get_boot_options(&self) -> Result<BootOptions, RedfishError> {
        self.s.get_boot_options()
    }

    fn get_boot_option(&self, option_id: &str) -> Result<BootOption, RedfishError> {
        self.s.get_boot_option(option_id)
    }

    fn boot_once(&self, target: Boot) -> Result<(), RedfishError> {
        match target {
            Boot::Pxe => self.set_boot_first(dell::BootDevices::PXE, true),
            Boot::HardDisk => self.set_boot_first(dell::BootDevices::HDD, true),
            Boot::UefiHttp => unimplemented!("No dell UefiHttp implementation"),
        }
    }

    fn boot_first(&self, target: Boot) -> Result<(), RedfishError> {
        match target {
            Boot::Pxe => self.set_boot_first(dell::BootDevices::PXE, false),
            Boot::HardDisk => self.set_boot_first(dell::BootDevices::HDD, false),
            Boot::UefiHttp => unimplemented!("No dell UefiHttp implementation"),
        }
    }

    fn clear_tpm(&self) -> Result<(), RedfishError> {
        self.delete_job_queue()?;

        let apply_time = dell::SetSettingsApplyTime {
            apply_time: dell::RedfishSettingsApplyTime::OnReset,
        };
        let tpm = dell::BiosTpmAttrs {
            tpm_security: OnOff::On,
            tpm2_hierarchy: dell::Tpm2HierarchySettings::Clear,
        };
        let set_tpm_clear = dell::SetBiosTpmAttrs {
            redfish_settings_apply_time: apply_time,
            attributes: tpm,
        };
        let url = format!("Systems/{}/Bios/Settings/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_tpm_clear)
            .map(|_status_code| ())
    }

    fn pending(&self) -> Result<HashMap<String, serde_json::Value>, RedfishError> {
        self.s.pending()
    }

    fn clear_pending(&self) -> Result<(), RedfishError> {
        self.delete_job_queue()
    }

    fn pcie_devices(&self) -> Result<Vec<PCIeDevice>, RedfishError> {
        self.s.pcie_devices()
    }

    fn update_firmware(
        &self,
        firmware: std::fs::File,
    ) -> Result<crate::model::task::Task, RedfishError> {
        self.s.update_firmware(firmware)
    }

    fn get_task(&self, id: &str) -> Result<crate::model::task::Task, RedfishError> {
        self.s.get_task(id)
    }

    fn get_firmware(&self, id: &str) -> Result<SoftwareInventory, RedfishError> {
        self.s.get_firmware(id)
    }

    fn get_software_inventories(&self) -> Result<SoftwareInventoryCollection, RedfishError> {
        self.s.get_software_inventories()
    }

    fn get_system(&self) -> Result<ComputerSystem, RedfishError> {
        self.s.get_system()
    }

    fn get_secure_boot(&self) -> Result<SecureBoot, RedfishError> {
        self.s.get_secure_boot()
    }

    fn disable_secure_boot(&self) -> Result<(), RedfishError> {
        self.s.disable_secure_boot()
    }

    fn get_network_device_function(
        &self,
        chassis_id: &str,
        id: &str,
    ) -> Result<NetworkDeviceFunction, RedfishError> {
        self.s.get_network_device_function(chassis_id, id)
    }

    fn get_network_device_functions(
        &self,
        chassis_id: &str,
    ) -> Result<NetworkDeviceFunctionCollection, RedfishError> {
        self.s.get_network_device_functions(chassis_id)
    }

    fn get_chassises(&self) -> Result<ChassisCollection, RedfishError> {
        self.s.get_chassises()
    }

    fn get_chassis(&self, id: &str) -> Result<Chassis, RedfishError> {
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

    fn change_uefi_password(
        &self,
        _current_uefi_password: &str,
        _new_uefi_password: &str,
    ) -> Result<(), RedfishError> {
        unimplemented!()
    }

    fn change_boot_order(&self, boot_array: Vec<String>) -> Result<(), RedfishError> {
        self.s.change_boot_order(boot_array)
    }
}

impl Bmc {
    // No changes can be applied if there are pending jobs
    fn delete_job_queue(&self) -> Result<(), RedfishError> {
        // The queue can't be cleared if system lockdown is enabled
        if self.is_lockdown()? {
            return Err(RedfishError::Lockdown);
        }

        let url = format!(
            "Managers/{}/Oem/Dell/DellJobService/Actions/DellJobService.DeleteJobQueue",
            self.s.manager_id()
        );
        let mut body = HashMap::new();
        body.insert("JobID", "JID_CLEARALL".to_string());
        self.s.client.post(&url, body).map(|_status_code| ())
    }

    // Is system lockdown enabled?
    fn is_lockdown(&self) -> Result<bool, RedfishError> {
        let (attrs, url) = self.manager_attributes()?;

        let key = "Lockdown.1.SystemLockdown";
        let system_lockdown = attrs
            .get(key)
            .ok_or_else(|| RedfishError::MissingKey {
                key: key.to_string(),
                url: url.to_string(),
            })?
            .as_str()
            .ok_or_else(|| RedfishError::InvalidKeyType {
                key: key.to_string(),
                expected_type: "&str".to_string(),
                url: url.to_string(),
            })?;

        let enabled = EnabledDisabled::Enabled.to_string();
        Ok(system_lockdown == enabled)
    }

    fn set_boot_first(&self, entry: dell::BootDevices, once: bool) -> Result<(), RedfishError> {
        let apply_time = dell::SetSettingsApplyTime {
            apply_time: dell::RedfishSettingsApplyTime::OnReset,
        };
        let boot_entry = dell::ServerBoot {
            first_boot_device: entry,
            boot_once: if once {
                EnabledDisabled::Enabled
            } else {
                EnabledDisabled::Disabled
            },
        };
        let boot = dell::ServerBootAttrs {
            server_boot: boot_entry,
        };
        let set_boot = dell::SetFirstBootDevice {
            redfish_settings_apply_time: apply_time,
            attributes: boot,
        };
        let url = format!("Managers/{}/Attributes", self.s.manager_id());
        self.s.client.patch(&url, set_boot).map(|_status_code| ())
    }

    fn enable_bios_lockdown(&self) -> Result<(), RedfishError> {
        let apply_time = dell::SetSettingsApplyTime {
            apply_time: dell::RedfishSettingsApplyTime::OnReset, // requires reboot to apply
        };
        let lockdown = dell::BiosLockdownAttrs {
            in_band_manageability_interface: EnabledDisabled::Disabled,
            uefi_variable_access: dell::UefiVariableAccessSettings::Controlled,
        };
        let set_lockdown_attrs = dell::SetBiosLockdownAttrs {
            redfish_settings_apply_time: apply_time,
            attributes: lockdown,
        };
        let url = format!("Systems/{}/Bios/Settings/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_lockdown_attrs)
            .map(|_status_code| ())
    }

    fn enable_bmc_lockdown(
        &self,
        entry: dell::BootDevices,
        once: bool,
    ) -> Result<(), RedfishError> {
        let apply_time = dell::SetSettingsApplyTime {
            apply_time: dell::RedfishSettingsApplyTime::OnReset,
        };

        // First change all settings except lockdown, because that applies immediately
        // and prevents the other settings being applied.
        let boot_entry = dell::ServerBoot {
            first_boot_device: entry,
            boot_once: if once {
                EnabledDisabled::Enabled
            } else {
                EnabledDisabled::Disabled
            },
        };
        let lockdown = dell::BmcLockdown {
            system_lockdown: None,
            racadm_enable: Some(EnabledDisabled::Disabled),
            server_boot: Some(boot_entry),
        };
        let set_bmc_lockdown = dell::SetBmcLockdown {
            redfish_settings_apply_time: apply_time,
            attributes: lockdown,
        };
        let manager_id = self.s.manager_id();
        let url = format!("Managers/{manager_id}/Oem/Dell/DellAttributes/{manager_id}");
        self.s
            .client
            .patch(&url, set_bmc_lockdown)
            .map(|_status_code| ())?;

        // Now lockdown
        let lockdown = dell::BmcLockdown {
            system_lockdown: Some(EnabledDisabled::Enabled),
            racadm_enable: None,
            server_boot: None,
        };
        let set_bmc_lockdown = dell::SetBmcLockdown {
            redfish_settings_apply_time: apply_time,
            attributes: lockdown,
        };
        self.s
            .client
            .patch(&url, set_bmc_lockdown)
            .map(|_status_code| ())
    }

    fn disable_bios_lockdown(&self) -> Result<(), RedfishError> {
        let apply_time = dell::SetSettingsApplyTime {
            apply_time: dell::RedfishSettingsApplyTime::OnReset, // requires reboot to apply
        };
        let lockdown = dell::BiosLockdownAttrs {
            in_band_manageability_interface: EnabledDisabled::Enabled,
            uefi_variable_access: dell::UefiVariableAccessSettings::Standard,
        };
        let set_lockdown_attrs = dell::SetBiosLockdownAttrs {
            redfish_settings_apply_time: apply_time,
            attributes: lockdown,
        };
        let url = format!("Systems/{}/Bios/Settings/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_lockdown_attrs)
            .map(|_status_code| ())
    }

    fn disable_bmc_lockdown(
        &self,
        entry: dell::BootDevices,
        once: bool,
    ) -> Result<(), RedfishError> {
        let apply_time = dell::SetSettingsApplyTime {
            apply_time: dell::RedfishSettingsApplyTime::Immediate, // bmc settings don't require reboot
        };
        let boot_entry = dell::ServerBoot {
            first_boot_device: entry,
            boot_once: if once {
                EnabledDisabled::Enabled
            } else {
                EnabledDisabled::Disabled
            },
        };
        let lockdown = dell::BmcLockdown {
            system_lockdown: Some(EnabledDisabled::Disabled),
            racadm_enable: Some(EnabledDisabled::Enabled),
            server_boot: Some(boot_entry),
        };
        let set_bmc_lockdown = dell::SetBmcLockdown {
            redfish_settings_apply_time: apply_time,
            attributes: lockdown,
        };
        let manager_id = self.s.manager_id();
        let url = format!("Managers/{manager_id}/Oem/Dell/DellAttributes/{manager_id}");
        self.s
            .client
            .patch(&url, set_bmc_lockdown)
            .map(|_status_code| ())
    }

    fn setup_bmc_remote_access(&self) -> Result<(), RedfishError> {
        let apply_time = dell::SetSettingsApplyTime {
            apply_time: dell::RedfishSettingsApplyTime::Immediate,
        };
        let serial_redirect = dell::SerialRedirection {
            enable: EnabledDisabled::Enabled,
        };
        let ipmi_sol_settings = dell::IpmiSol {
            enable: EnabledDisabled::Enabled,
            baud_rate: "115200".to_string(),
            min_privilege: "Administrator".to_string(),
        };
        let remote_access = dell::BmcRemoteAccess {
            ssh_enable: EnabledDisabled::Enabled,
            serial_redirection: serial_redirect,
            ipmi_lan_enable: EnabledDisabled::Enabled,
            ipmi_sol: ipmi_sol_settings,
        };
        let set_remote_access = dell::SetBmcRemoteAccess {
            redfish_settings_apply_time: apply_time,
            attributes: remote_access,
        };
        let url = format!("Managers/{}/Attributes", self.s.manager_id());
        self.s
            .client
            .patch(&url, set_remote_access)
            .map(|_status_code| ())
    }

    fn bmc_remote_access_status(&self) -> Result<Status, RedfishError> {
        let (attrs, _) = self.manager_attributes()?;
        let expected = vec![
            // "any" means any value counts as correctly disabled
            ("SerialRedirection.1.Enable", "Enabled", "Disabled"),
            ("IPMISOL.1.BaudRate", "115200", "any"),
            ("IPMISOL.1.Enable", "Enabled", "Disabled"),
            ("IPMISOL.1.MinPrivilege", "Administrator", "any"),
            ("SSH.1.Enable", "Enabled", "Disabled"),
            ("IPMILan.1.Enable", "Enabled", "Disabled"),
        ];

        // url is for error messages only
        let manager_id = self.s.manager_id();
        let url = &format!("Managers/{manager_id}/Oem/Dell/DellAttributes/{manager_id}");

        let mut message = String::new();
        let mut enabled = true;
        let mut disabled = true;
        for (key, val_enabled, val_disabled) in expected {
            let val_current = attrs
                .get(key)
                .ok_or_else(|| RedfishError::MissingKey {
                    key: key.to_string(),
                    url: url.to_string(),
                })?
                .as_str()
                .ok_or_else(|| RedfishError::InvalidKeyType {
                    key: key.to_string(),
                    expected_type: "&str".to_string(),
                    url: url.to_string(),
                })?;
            message.push_str(&format!("{key}={val_current} "));
            if val_current != val_enabled {
                enabled = false;
            }
            if val_current != val_disabled && val_disabled != "any" {
                disabled = false;
            }
        }

        Ok(Status {
            message,
            status: match (enabled, disabled) {
                (true, _) => StatusInternal::Enabled,
                (_, true) => StatusInternal::Disabled,
                _ => StatusInternal::Partial,
            },
        })
    }

    fn bios_serial_console_status(&self) -> Result<Status, RedfishError> {
        let mut message = String::new();

        // Start with true, then check every value to see whether it means things are not setup
        // correctly, and set the value to false.
        // Note that there are three results: Enabled, Disabled, and Partial, so enabled and
        // disabled can both be false by the end. They cannot both be true.
        let mut enabled = true;
        let mut disabled = true;

        let url = &format!("Systems/{}/Bios", self.s.system_id());
        let (_status_code, bios): (_, dell::Bios) = self.s.client.get(url)?;
        let bios = bios.attributes;

        let val = bios.serial_comm;
        message.push_str(&format!("serial_comm={val} "));
        match val.parse().map_err(|err| RedfishError::InvalidValue {
            err,
            url: url.to_string(),
            field: "serial_comm".to_string(),
        })? {
            dell::SerialCommSettings::OnConRedir | dell::SerialCommSettings::OnConRedirAuto => {
                // enabled
                disabled = false;
            }
            dell::SerialCommSettings::Off => {
                // disabled
                enabled = false;
            }
            _ => {
                // someone messed with it manually
                enabled = false;
                disabled = false;
            }
        }

        let val = bios.redir_after_boot;
        message.push_str(&format!("redir_after_boot={val} "));
        match val.parse().map_err(|err| RedfishError::InvalidValue {
            err,
            url: url.to_string(),
            field: "redir_after_boot".to_string(),
        })? {
            EnabledDisabled::Enabled => {
                disabled = false;
            }
            EnabledDisabled::Disabled => {
                enabled = false;
            }
        }

        // All of these need a specific value for serial console access to work.
        // Any other value counts as correctly disabled.

        let val = bios.serial_port_address;
        message.push_str(&format!("serial_port_address={val} "));
        if val != dell::SerialPortSettings::Com1.to_string() {
            enabled = false;
        }

        let val = bios.ext_serial_connector;
        message.push_str(&format!("ext_serial_connector={val} "));
        if val != dell::SerialPortExtSettings::Serial1.to_string() {
            enabled = false;
        }

        let val = bios.fail_safe_baud;
        message.push_str(&format!("fail_safe_baud={val} "));
        if &val != "115200" {
            enabled = false;
        }

        let val = bios.con_term_type;
        message.push_str(&format!("con_term_type={val} "));
        if val != dell::SerialPortTermSettings::Vt100Vt220.to_string() {
            enabled = false;
        }

        Ok(Status {
            message,
            status: match (enabled, disabled) {
                (true, _) => StatusInternal::Enabled,
                (_, true) => StatusInternal::Disabled,
                _ => StatusInternal::Partial,
            },
        })
    }

    // Second value in tuple is URL we used to fetch attributes, for diagnostics
    fn manager_attributes(
        &self,
    ) -> Result<(serde_json::Map<String, serde_json::Value>, String), RedfishError> {
        let manager_id = self.s.manager_id();
        let url = &format!("Managers/{manager_id}/Oem/Dell/DellAttributes/{manager_id}");
        let (_status_code, body): (_, HashMap<String, serde_json::Value>) =
            self.s.client.get(url)?;
        let key = "Attributes";
        let v = body
            .get(key)
            .ok_or_else(|| RedfishError::MissingKey {
                key: key.to_string(),
                url: url.to_string(),
            })?
            .as_object()
            .ok_or_else(|| RedfishError::InvalidKeyType {
                key: key.to_string(),
                expected_type: "Object".to_string(),
                url: url.to_string(),
            })
            .cloned()?;
        Ok((v, url.to_string()))
    }

    // TPM is enabled by default so we never call this.
    #[allow(dead_code)]
    fn enable_tpm(&self) -> Result<(), RedfishError> {
        let apply_time = dell::SetSettingsApplyTime {
            apply_time: dell::RedfishSettingsApplyTime::OnReset, // requires reboot to apply
        };
        let tpm = dell::BiosTpmAttrs {
            tpm_security: OnOff::On,
            tpm2_hierarchy: dell::Tpm2HierarchySettings::Enabled,
        };
        let set_tpm_enabled = dell::SetBiosTpmAttrs {
            redfish_settings_apply_time: apply_time,
            attributes: tpm,
        };
        let url = format!("Systems/{}/Bios/Settings/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_tpm_enabled)
            .map(|_status_code| ())
    }

    // Dell supports disabling the TPM. Why would we do this?
    // Lenovo does not support disabling TPM2.0
    #[allow(dead_code)]
    fn disable_tpm(&self) -> Result<(), RedfishError> {
        let apply_time = dell::SetSettingsApplyTime {
            apply_time: dell::RedfishSettingsApplyTime::OnReset, // requires reboot to apply
        };
        let tpm = dell::BiosTpmAttrs {
            tpm_security: OnOff::Off,
            tpm2_hierarchy: dell::Tpm2HierarchySettings::Disabled,
        };
        let set_tpm_disabled = dell::SetBiosTpmAttrs {
            redfish_settings_apply_time: apply_time,
            attributes: tpm,
        };
        let url = format!("Systems/{}/Bios/Settings/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_tpm_disabled)
            .map(|_status_code| ())
    }
}
