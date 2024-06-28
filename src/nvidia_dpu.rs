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
use std::{collections::HashMap, path::Path, time::Duration};

use serde::Deserialize;

use crate::model::account_service::ManagerAccount;
use crate::model::task::Task;
use crate::Boot::UefiHttp;
use crate::HostPrivilegeLevel::Restricted;
use crate::InternalCPUModel::Embedded;
use crate::{
    model::{
        boot::{BootSourceOverrideEnabled, BootSourceOverrideTarget},
        chassis::NetworkAdapter,
        oem::nvidia_dpu::{HostPrivilegeLevel, InternalCPUModel},
        sel::{LogEntry, LogEntryCollection},
        service_root::ServiceRoot,
        BootOption, ComputerSystem, Manager,
    },
    standard::RedfishStandard,
    NetworkDeviceFunction, Redfish, RedfishError,
};
use crate::{MachineSetupDiff, MachineSetupStatus, JobState, RoleId};

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

#[async_trait::async_trait]
impl Redfish for Bmc {
    async fn create_user(
        &self,
        username: &str,
        password: &str,
        role_id: RoleId,
    ) -> Result<(), RedfishError> {
        self.s.create_user(username, password, role_id).await
    }

    async fn change_username(&self, old_name: &str, new_name: &str) -> Result<(), RedfishError> {
        self.s.change_username(old_name, new_name).await
    }

    async fn change_password(&self, user: &str, new: &str) -> Result<(), RedfishError> {
        self.s.change_password(user, new).await
    }

    /// Note that DPU account_ids are not numbers but usernames: "root", "admin", etc
    async fn change_password_by_id(
        &self,
        account_id: &str,
        new_pass: &str,
    ) -> Result<(), RedfishError> {
        self.s.change_password_by_id(account_id, new_pass).await
    }

    async fn get_accounts(&self) -> Result<Vec<ManagerAccount>, RedfishError> {
        self.s.get_accounts().await
    }

    async fn get_firmware(
        &self,
        id: &str,
    ) -> Result<crate::model::software_inventory::SoftwareInventory, RedfishError> {
        self.s.get_firmware(id).await
    }

    async fn get_software_inventories(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_software_inventories().await
    }

    async fn get_tasks(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_tasks().await
    }

    async fn get_task(&self, id: &str) -> Result<crate::model::task::Task, RedfishError> {
        self.s.get_task(id).await
    }

    async fn get_power_state(&self) -> Result<crate::PowerState, RedfishError> {
        self.s.get_power_state().await
    }

    async fn get_power_metrics(&self) -> Result<crate::Power, RedfishError> {
        self.s.get_power_metrics().await
    }

    async fn power(&self, action: crate::SystemPowerControl) -> Result<(), RedfishError> {
        self.s.power(action).await
    }

    async fn bmc_reset(&self) -> Result<(), RedfishError> {
        self.s.bmc_reset().await
    }

    async fn get_thermal_metrics(&self) -> Result<crate::Thermal, RedfishError> {
        self.s.get_thermal_metrics().await
    }

    async fn get_system_event_log(&self) -> Result<Vec<LogEntry>, RedfishError> {
        self.get_system_event_log().await
    }

    async fn machine_setup(&self) -> Result<(), RedfishError> {
        self.disable_secure_boot().await?;
        self.set_host_privilege_level(Restricted).await?;
        self.set_internal_cpu_model(Embedded).await?;
        self.boot_once(UefiHttp).await
    }

    async fn machine_setup_status(&self) -> Result<MachineSetupStatus, RedfishError> {
        let mut diffs = vec![];

        let sb = self.get_secure_boot().await?;
        if !sb.secure_boot_enable {
            diffs.push(MachineSetupDiff {
                key: "SecureBoot".to_string(),
                expected: "true".to_string(),
                actual: "false".to_string(),
            });
        }

        let bios = self.s.bios_attributes().await?;
        let key = "Host Privilege Level";
        let Some(hpl) = bios.get(key) else {
            return Err(RedfishError::MissingKey {
                key: key.to_string(),
                url: "Systems/{}/Bios".to_string(),
            });
        };
        let actual = HostPrivilegeLevel::deserialize(hpl).map_err(|e| {
            RedfishError::JsonDeserializeError {
                url: "Systems/{}/Bios".to_string(),
                body: hpl.to_string(),
                source: e,
            }
        })?;
        let expected = HostPrivilegeLevel::Restricted;
        if actual != expected {
            diffs.push(MachineSetupDiff {
                key: key.to_string(),
                actual: actual.to_string(),
                expected: expected.to_string(),
            });
        }

        let key = "Internal CPU Model";
        let Some(icm) = bios.get(key) else {
            return Err(RedfishError::MissingKey {
                key: key.to_string(),
                url: "Systems/{}/Bios".to_string(),
            });
        };
        let actual =
            InternalCPUModel::deserialize(icm).map_err(|e| RedfishError::JsonDeserializeError {
                url: "Systems/{}/Bios".to_string(),
                body: hpl.to_string(),
                source: e,
            })?;
        let expected = InternalCPUModel::Embedded;
        if actual != expected {
            diffs.push(MachineSetupDiff {
                key: key.to_string(),
                actual: actual.to_string(),
                expected: expected.to_string(),
            });
        }

        Ok(MachineSetupStatus {
            is_done: diffs.is_empty(),
            diffs,
        })
    }

    async fn set_machine_password_policy(&self) -> Result<(), RedfishError> {
        use serde_json::Value::Number;
        let body = HashMap::from([
            ("AccountLockoutThreshold", Number(0.into())),
            ("AccountLockoutDuration", Number(0.into())),
        ]);
        self.s
            .client
            .patch("AccountService", body)
            .await
            .map(|_status_code| ())
    }

    async fn lockdown(&self, target: crate::EnabledDisabled) -> Result<(), RedfishError> {
        self.s.lockdown(target).await
    }

    async fn lockdown_status(&self) -> Result<crate::Status, RedfishError> {
        self.s.lockdown_status().await
    }

    async fn setup_serial_console(&self) -> Result<(), RedfishError> {
        self.s.setup_serial_console().await
    }

    async fn serial_console_status(&self) -> Result<crate::Status, RedfishError> {
        self.s.serial_console_status().await
    }

    async fn get_boot_options(&self) -> Result<crate::BootOptions, RedfishError> {
        self.s.get_boot_options().await
    }

    async fn get_boot_option(&self, option_id: &str) -> Result<BootOption, RedfishError> {
        self.s.get_boot_option(option_id).await
    }

    async fn boot_once(&self, target: crate::Boot) -> Result<(), RedfishError> {
        match target {
            crate::Boot::Pxe => {
                self.set_boot_override(
                    BootSourceOverrideTarget::Pxe,
                    BootSourceOverrideEnabled::Once,
                )
                .await
            }
            crate::Boot::HardDisk => {
                self.set_boot_override(
                    BootSourceOverrideTarget::Hdd,
                    BootSourceOverrideEnabled::Once,
                )
                .await
            }
            crate::Boot::UefiHttp => {
                self.set_boot_override(
                    BootSourceOverrideTarget::UefiHttp,
                    BootSourceOverrideEnabled::Once,
                )
                .await
            }
        }
    }

    async fn boot_first(&self, target: crate::Boot) -> Result<(), RedfishError> {
        match target {
            crate::Boot::Pxe => self.set_boot_order(&BootOptionName::Pxe).await,
            crate::Boot::HardDisk => self.set_boot_order(&BootOptionName::Disk).await,
            crate::Boot::UefiHttp => self.set_boot_order(&BootOptionName::Http).await,
        }
    }

    async fn clear_tpm(&self) -> Result<(), RedfishError> {
        self.s.clear_tpm().await
    }

    async fn pcie_devices(&self) -> Result<Vec<crate::PCIeDevice>, RedfishError> {
        self.s.pcie_devices().await
    }

    async fn update_firmware(
        &self,
        firmware: tokio::fs::File,
    ) -> Result<crate::model::task::Task, RedfishError> {
        self.s.update_firmware(firmware).await
    }

    async fn update_firmware_multipart(
        &self,
        filename: &Path,
        reboot: bool,
        timeout: Duration,
    ) -> Result<String, RedfishError> {
        self.s
            .update_firmware_multipart(filename, reboot, timeout)
            .await
    }

    async fn bios(
        &self,
    ) -> Result<std::collections::HashMap<String, serde_json::Value>, RedfishError> {
        self.s.bios().await
    }

    async fn pending(
        &self,
    ) -> Result<std::collections::HashMap<String, serde_json::Value>, RedfishError> {
        self.s.pending().await
    }

    async fn clear_pending(&self) -> Result<(), RedfishError> {
        self.s.clear_pending().await
    }

    async fn get_system(&self) -> Result<ComputerSystem, RedfishError> {
        self.s.get_system().await
    }

    async fn get_secure_boot(&self) -> Result<crate::model::secure_boot::SecureBoot, RedfishError> {
        self.s.get_secure_boot().await
    }

    async fn enable_secure_boot(&self) -> Result<(), RedfishError> {
        self.s.enable_secure_boot().await
    }

    async fn disable_secure_boot(&self) -> Result<(), RedfishError> {
        self.s.disable_secure_boot().await
    }

    async fn add_secure_boot_certificate(&self, pem_cert: &str) -> Result<Task, RedfishError> {
        self.s.add_secure_boot_certificate(pem_cert).await
    }

    async fn get_chassis_all(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_chassis_all().await
    }

    async fn get_chassis(&self, id: &str) -> Result<crate::Chassis, RedfishError> {
        self.s.get_chassis(id).await
    }

    async fn get_chassis_network_adapters(
        &self,
        chassis_id: &str,
    ) -> Result<Vec<String>, RedfishError> {
        self.s.get_chassis_network_adapters(chassis_id).await
    }

    async fn get_chassis_network_adapter(
        &self,
        chassis_id: &str,
        id: &str,
    ) -> Result<NetworkAdapter, RedfishError> {
        self.s.get_chassis_network_adapter(chassis_id, id).await
    }

    async fn get_manager_ethernet_interfaces(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_manager_ethernet_interfaces().await
    }

    async fn get_manager_ethernet_interface(
        &self,
        id: &str,
    ) -> Result<crate::EthernetInterface, RedfishError> {
        self.s.get_manager_ethernet_interface(id).await
    }

    async fn get_system_ethernet_interfaces(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_system_ethernet_interfaces().await
    }

    async fn get_system_ethernet_interface(
        &self,
        id: &str,
    ) -> Result<crate::EthernetInterface, RedfishError> {
        self.s.get_system_ethernet_interface(id).await
    }

    async fn get_ports(&self, chassis_id: &str) -> Result<Vec<String>, RedfishError> {
        // http://redfish.dmtf.org/schemas/v1/NetworkPortCollection.json
        let url = format!(
            "Chassis/{}/NetworkAdapters/NvidiaNetworkAdapter/Ports",
            chassis_id
        );
        self.s.get_members(&url).await
    }

    async fn get_port(
        &self,
        chassis_id: &str,
        id: &str,
    ) -> Result<crate::NetworkPort, RedfishError> {
        let url = format!(
            "Chassis/{}/NetworkAdapters/NvidiaNetworkAdapter/Ports/{}",
            chassis_id, id
        );
        let (_status_code, body) = self.s.client.get(&url).await?;
        Ok(body)
    }

    async fn get_network_device_function(
        &self,
        chassis_id: &str,
        id: &str,
    ) -> Result<NetworkDeviceFunction, RedfishError> {
        let url = format!(
            "Chassis/{}/NetworkAdapters/NvidiaNetworkAdapter/NetworkDeviceFunctions/{}",
            chassis_id, id
        );
        let (_status_code, body) = self.s.client.get(&url).await?;
        Ok(body)
    }

    /// http://redfish.dmtf.org/schemas/v1/NetworkDeviceFunctionCollection.json
    async fn get_network_device_functions(
        &self,
        chassis_id: &str,
    ) -> Result<Vec<String>, RedfishError> {
        let url = format!(
            "Chassis/{}/NetworkAdapters/NvidiaNetworkAdapter/NetworkDeviceFunctions",
            chassis_id
        );
        self.s.get_members(&url).await
    }

    async fn change_uefi_password(
        &self,
        current_uefi_password: &str,
        new_uefi_password: &str,
    ) -> Result<Option<String>, RedfishError> {
        let mut attributes = HashMap::new();
        let mut data = HashMap::new();
        data.insert("CurrentUefiPassword", current_uefi_password.to_string());
        data.insert("UefiPassword", new_uefi_password.to_string());
        attributes.insert("Attributes", data);
        let url = format!("Systems/{}/Bios/Settings", self.s.system_id());
        let _status_code = self.s.client.patch(&url, attributes).await?;
        Ok(None)
    }

    async fn change_boot_order(&self, boot_array: Vec<String>) -> Result<(), RedfishError> {
        let body = HashMap::from([("Boot", HashMap::from([("BootOrder", boot_array)]))]);
        let url = format!("Systems/{}/Settings", self.s.system_id());
        self.s.client.patch(&url, body).await?;
        Ok(())
    }

    async fn get_service_root(&self) -> Result<ServiceRoot, RedfishError> {
        self.s.get_service_root().await
    }

    async fn get_systems(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_systems().await
    }

    async fn get_managers(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_managers().await
    }

    async fn get_manager(&self) -> Result<Manager, RedfishError> {
        self.s.get_manager().await
    }

    async fn bmc_reset_to_defaults(&self) -> Result<(), RedfishError> {
        self.s.bmc_reset_to_defaults().await
    }

    async fn get_job_state(&self, job_id: &str) -> Result<JobState, RedfishError> {
        self.s.get_job_state(job_id).await
    }
}

impl Bmc {
    async fn set_host_privilege_level(
        &self,
        level: HostPrivilegeLevel,
    ) -> Result<(), RedfishError> {
        let data = HashMap::from([(
            "Attributes",
            HashMap::from([("Host Privilege Level", level.to_string())]),
        )]);
        let url = format!("Systems/{}/Bios/Settings", self.s.system_id());
        self.s
            .client
            .patch(&url, data)
            .await
            .map(|_status_code| Ok(()))?
    }

    async fn set_internal_cpu_model(&self, model: InternalCPUModel) -> Result<(), RedfishError> {
        let data = HashMap::from([(
            "Attributes",
            HashMap::from([("Internal CPU Model", model.to_string())]),
        )]);
        let url = format!("Systems/{}/Bios/Settings", self.s.system_id());
        self.s
            .client
            .patch(&url, data)
            .await
            .map(|_status_code| Ok(()))?
    }

    async fn set_boot_override(
        &self,
        override_target: BootSourceOverrideTarget,
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
            format!("{}", override_target),
        );
        let url = format!("Systems/{}/Settings ", self.s.system_id());
        self.s
            .client
            .patch(&url, HashMap::from([("Boot", data)]))
            .await?;
        Ok(())
    }

    // name: The name of the device you want to make the first boot choice.
    async fn set_boot_order(&self, name: &BootOptionName) -> Result<(), RedfishError> {
        let boot_array = match self.get_boot_options_ids_with_first(name).await? {
            None => {
                return Err(RedfishError::MissingBootOption(name.to_string().to_owned()));
            }
            Some(b) => b,
        };
        self.change_boot_order(boot_array).await
    }

    // A Vec of string boot option names, with the one you want first.
    //
    // Example: get_boot_options_ids_with_first(lenovo::BootOptionName::Network) might return
    // ["Boot0003", "Boot0002", "Boot0001", "Boot0004"] where Boot0003 is Network. It has been
    // moved to the front ready for sending as an update.
    // The order of the other boot options does not change.
    //
    // If the boot option you want is not found returns Ok(None)
    async fn get_boot_options_ids_with_first(
        &self,
        with_name: &BootOptionName,
    ) -> Result<Option<Vec<String>>, RedfishError> {
        let with_name_str = with_name.to_string();
        let mut ordered = Vec::new(); // the final boot options
        let boot_options = self.s.get_system().await?.boot.boot_order;
        for member in boot_options {
            let b: BootOption = self.s.get_boot_option(member.as_str()).await?;
            if b.display_name.starts_with(with_name_str) {
                ordered.insert(0, b.id);
            } else {
                ordered.push(b.id);
            }
        }
        Ok(Some(ordered))
    }

    // dpu stores the sel as part of the system? there's a LogServices for the bmc too, but no sel
    async fn get_system_event_log(&self) -> Result<Vec<LogEntry>, RedfishError> {
        let url = format!("Systems/{}/LogServices/SEL/Entries", self.s.system_id());
        let (_status_code, log_entry_collection): (_, LogEntryCollection) =
            self.s.client.get(&url).await?;
        let log_entries = log_entry_collection.members;
        Ok(log_entries)
    }
}
